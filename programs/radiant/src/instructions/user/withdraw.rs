use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::{LendingMarket, Reserve, Obligation};
use crate::constants::{VAULT_SEED, MIN_HEALTH_FACTOR_AFTER_BORROW, MAX_RESERVE_STALENESS_SLOTS};
use crate::events::WithdrawEvent;

/// Accounts for withdrawing collateral
#[derive(Accounts)]
pub struct Withdraw<'info> {
    /// User withdrawing collateral
    pub owner: Signer<'info>,

    /// The lending market
    #[account(
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve to withdraw from
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ WithdrawError::InvalidReserve
    )]
    pub reserve: Account<'info, Reserve>,

    /// User's obligation account
    #[account(
        mut,
        constraint = obligation.lending_market == lending_market.key() @ WithdrawError::InvalidObligation,
        constraint = obligation.owner == owner.key() @ WithdrawError::InvalidObligationOwner,
        seeds = [Obligation::SEED_PREFIX, lending_market.key().as_ref(), owner.key().as_ref()],
        bump = obligation.bump
    )]
    pub obligation: Account<'info, Obligation>,

    /// Reserve's vault (source)
    #[account(
        mut,
        seeds = [VAULT_SEED, reserve.key().as_ref()],
        bump,
        constraint = token_vault.key() == reserve.token_vault @ WithdrawError::InvalidVault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    /// User's token account (destination)
    #[account(
        mut,
        constraint = user_token_account.mint == reserve.token_mint @ WithdrawError::InvalidTokenMint,
        constraint = user_token_account.owner == owner.key() @ WithdrawError::InvalidTokenOwner
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Token program
    pub token_program: Program<'info, Token>,
}

/// Withdraw collateral from the reserve
///
/// Transfers tokens from reserve vault to user.
/// Validates that withdrawal doesn't make position unhealthy.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `amount` - Amount of tokens to withdraw (in native units), 0 = withdraw all
pub fn handler(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    let reserve = &mut ctx.accounts.reserve;
    let obligation = &mut ctx.accounts.obligation;
    let reserve_key = reserve.key();
    let clock = Clock::get()?;

    // Check reserve is not stale
    require!(
        !reserve.is_stale(clock.slot, MAX_RESERVE_STALENESS_SLOTS),
        WithdrawError::ReserveStale
    );

    // Find user's deposit in this reserve
    let deposit_index = obligation
        .find_deposit(&reserve_key)
        .ok_or(WithdrawError::NoDepositFound)?;

    let current_supply_index = reserve.liquidity.cumulative_supply_index;

    // Calculate current deposit value with accrued interest
    let deposit = &obligation.deposits[deposit_index];
    let current_deposit_amount = if deposit.supply_index_snapshot > 0 {
        (deposit.deposited_amount as u128 * current_supply_index / deposit.supply_index_snapshot) as u64
    } else {
        deposit.deposited_amount
    };

    // Determine withdraw amount (0 = withdraw all)
    let withdraw_amount = if amount == 0 {
        current_deposit_amount
    } else {
        amount
    };

    // Validate withdrawal amount
    require!(
        withdraw_amount <= current_deposit_amount,
        WithdrawError::InsufficientDeposit
    );

    // Check available liquidity in vault
    let available_liquidity = reserve.available_liquidity();
    require!(
        withdraw_amount <= available_liquidity,
        WithdrawError::InsufficientLiquidity
    );

    // Verify vault has sufficient balance
    require!(
        ctx.accounts.token_vault.amount >= withdraw_amount,
        WithdrawError::InsufficientVaultBalance
    );

    // Calculate remaining deposit after withdrawal
    let remaining_deposit = current_deposit_amount
        .checked_sub(withdraw_amount)
        .ok_or(WithdrawError::MathOverflow)?;

    // If user has borrows, validate health factor after withdrawal
    if obligation.has_borrows() {
        // Calculate the USD value being withdrawn (simplified - in production use oracle)
        // This is an approximation using cached deposit market value
        let deposit = &obligation.deposits[deposit_index];
        let withdraw_ratio = if current_deposit_amount > 0 {
            (withdraw_amount as u128 * 10000) / current_deposit_amount as u128
        } else {
            0
        };

        let withdraw_value_usd = (deposit.market_value_usd * withdraw_ratio) / 10000;

        // Calculate new deposited value after withdrawal
        let new_deposited_value_usd = obligation.deposited_value_usd
            .saturating_sub(withdraw_value_usd);

        // Calculate new allowed borrow value (using LTV)
        // Note: In production, this should recalculate with proper LTV from reserve config
        let new_allowed_borrow_value_usd = (new_deposited_value_usd * reserve.config.ltv_bps as u128) / 10000;

        // Calculate new unhealthy threshold value
        let new_unhealthy_borrow_value_usd = (new_deposited_value_usd * reserve.config.liquidation_threshold_bps as u128) / 10000;

        // Ensure borrowed value doesn't exceed new allowed borrow value
        require!(
            obligation.borrowed_value_usd <= new_allowed_borrow_value_usd,
            WithdrawError::InsufficientBorrowCapacity
        );

        // Calculate health factor after withdrawal
        let new_health_factor = if obligation.borrowed_value_usd > 0 {
            ((new_unhealthy_borrow_value_usd * 10000) / obligation.borrowed_value_usd) as u64
        } else {
            u64::MAX // No debt = infinite health
        };

        // Require health factor stays above minimum threshold
        require!(
            new_health_factor >= MIN_HEALTH_FACTOR_AFTER_BORROW,
            WithdrawError::HealthFactorTooLow
        );

        // Also check current position is healthy before allowing withdrawal
        require!(
            obligation.is_healthy(),
            WithdrawError::PositionUnhealthy
        );
    }

    // Transfer tokens from vault to user using PDA signer
    let seeds = &[
        Reserve::SEED_PREFIX,
        reserve.lending_market.as_ref(),
        reserve.token_mint.as_ref(),
        &[reserve.bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let transfer_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.token_vault.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: reserve.to_account_info(),
        },
        signer_seeds,
    );
    token::transfer(transfer_ctx, withdraw_amount)?;

    // Update reserve liquidity
    reserve.liquidity.total_deposits = reserve.liquidity.total_deposits
        .checked_sub(withdraw_amount)
        .ok_or(WithdrawError::MathOverflow)?;

    // Update or remove obligation deposit
    if remaining_deposit == 0 {
        // Remove the deposit entry
        obligation.deposits.remove(deposit_index);
    } else {
        // Update the deposit with remaining amount
        let deposit = &mut obligation.deposits[deposit_index];
        deposit.deposited_amount = remaining_deposit;
        deposit.supply_index_snapshot = current_supply_index;
    }

    // Update timestamps
    reserve.last_update_slot = clock.slot;
    reserve.last_update_timestamp = clock.unix_timestamp;
    obligation.last_update_slot = clock.slot;

    // Emit withdraw event
    emit!(WithdrawEvent {
        lending_market: ctx.accounts.lending_market.key(),
        reserve: reserve_key,
        obligation: obligation.key(),
        owner: ctx.accounts.owner.key(),
        amount: withdraw_amount,
        remaining_deposit,
        timestamp: clock.unix_timestamp,
    });

    msg!("Withdrew {} tokens from reserve {}", withdraw_amount, reserve.token_mint);

    Ok(())
}

/// Withdraw errors
#[error_code]
pub enum WithdrawError {
    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Obligation does not belong to this lending market")]
    InvalidObligation,

    #[msg("Obligation owner mismatch")]
    InvalidObligationOwner,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Token mint mismatch")]
    InvalidTokenMint,

    #[msg("Token account owner mismatch")]
    InvalidTokenOwner,

    #[msg("No deposit found for this reserve")]
    NoDepositFound,

    #[msg("Insufficient deposit balance")]
    InsufficientDeposit,

    #[msg("Insufficient liquidity in reserve")]
    InsufficientLiquidity,

    #[msg("Position is unhealthy, cannot withdraw")]
    PositionUnhealthy,

    #[msg("Withdrawal would exceed borrow capacity")]
    InsufficientBorrowCapacity,

    #[msg("Health factor would be too low after withdrawal")]
    HealthFactorTooLow,

    #[msg("Reserve data is stale, refresh required")]
    ReserveStale,

    #[msg("Insufficient balance in vault")]
    InsufficientVaultBalance,

    #[msg("Math overflow")]
    MathOverflow,
}
