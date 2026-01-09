use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::{LendingMarket, Reserve, Obligation, ObligationCollateral};
use crate::constants::{VAULT_SEED, MAX_OBLIGATION_DEPOSITS, MIN_DEPOSIT_AMOUNT, MAX_RESERVE_STALENESS_SLOTS};
use crate::events::DepositEvent;

/// Accounts for depositing collateral
#[derive(Accounts)]
pub struct Deposit<'info> {
    /// User depositing collateral
    #[account(mut)]
    pub owner: Signer<'info>,

    /// The lending market
    #[account(
        constraint = !lending_market.emergency_mode @ DepositError::EmergencyModeActive,
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve to deposit into
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ DepositError::InvalidReserve,
        constraint = reserve.config.deposits_enabled @ DepositError::DepositsDisabled
    )]
    pub reserve: Account<'info, Reserve>,

    /// User's obligation account
    #[account(
        mut,
        constraint = obligation.lending_market == lending_market.key() @ DepositError::InvalidObligation,
        constraint = obligation.owner == owner.key() @ DepositError::InvalidObligationOwner,
        seeds = [Obligation::SEED_PREFIX, lending_market.key().as_ref(), owner.key().as_ref()],
        bump = obligation.bump
    )]
    pub obligation: Account<'info, Obligation>,

    /// User's token account (source)
    #[account(
        mut,
        constraint = user_token_account.mint == reserve.token_mint @ DepositError::InvalidTokenMint,
        constraint = user_token_account.owner == owner.key() @ DepositError::InvalidTokenOwner
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Reserve's vault (destination)
    #[account(
        mut,
        seeds = [VAULT_SEED, reserve.key().as_ref()],
        bump,
        constraint = token_vault.key() == reserve.token_vault @ DepositError::InvalidVault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    /// Token program
    pub token_program: Program<'info, Token>,
}

/// Deposit collateral into the reserve
///
/// Transfers tokens from user to reserve vault and tracks the deposit
/// in the user's obligation.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `amount` - Amount of tokens to deposit (in native units)
pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    // Validate amount
    require!(amount > 0, DepositError::AmountZero);
    require!(amount >= MIN_DEPOSIT_AMOUNT, DepositError::AmountTooSmall);

    let reserve = &mut ctx.accounts.reserve;
    let obligation = &mut ctx.accounts.obligation;
    let clock = Clock::get()?;

    // Check reserve is not stale
    require!(
        !reserve.is_stale(clock.slot, MAX_RESERVE_STALENESS_SLOTS),
        DepositError::ReserveStale
    );

    // Check deposit limit if set
    if reserve.config.deposit_limit > 0 {
        let new_total = reserve.liquidity.total_deposits
            .checked_add(amount)
            .ok_or(DepositError::MathOverflow)?;
        require!(
            new_total <= reserve.config.deposit_limit,
            DepositError::DepositLimitExceeded
        );
    }

    // Transfer tokens from user to vault
    let transfer_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.token_vault.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        },
    );
    token::transfer(transfer_ctx, amount)?;

    // Update reserve liquidity
    reserve.liquidity.total_deposits = reserve.liquidity.total_deposits
        .checked_add(amount)
        .ok_or(DepositError::MathOverflow)?;

    // Update obligation
    let reserve_key = reserve.key();
    let current_supply_index = reserve.liquidity.cumulative_supply_index;

    // Check if user already has a deposit in this reserve
    if let Some(deposit_index) = obligation.find_deposit(&reserve_key) {
        // Update existing deposit
        let deposit = &mut obligation.deposits[deposit_index];

        // Calculate current value with interest, then add new deposit
        let current_amount = (deposit.deposited_amount as u128 * current_supply_index)
            / deposit.supply_index_snapshot;

        let new_amount = current_amount
            .checked_add(amount as u128)
            .ok_or(DepositError::MathOverflow)?;

        // Store new amount with current index as snapshot
        deposit.deposited_amount = new_amount as u64;
        deposit.supply_index_snapshot = current_supply_index;
    } else {
        // Create new deposit entry
        require!(
            obligation.deposits.len() < MAX_OBLIGATION_DEPOSITS,
            DepositError::MaxDepositsReached
        );

        obligation.deposits.push(ObligationCollateral::new(
            reserve_key,
            amount,
            current_supply_index,
        ));
    }

    // Update timestamp
    reserve.last_update_slot = clock.slot;
    reserve.last_update_timestamp = clock.unix_timestamp;
    obligation.last_update_slot = clock.slot;

    // Get new deposit amount for event
    let new_deposit_amount = if let Some(idx) = obligation.find_deposit(&reserve_key) {
        obligation.deposits[idx].deposited_amount
    } else {
        0
    };

    // Emit deposit event
    emit!(DepositEvent {
        lending_market: ctx.accounts.lending_market.key(),
        reserve: reserve_key,
        obligation: obligation.key(),
        owner: ctx.accounts.owner.key(),
        amount,
        new_deposit_amount,
        timestamp: clock.unix_timestamp,
    });

    msg!("Deposited {} tokens into reserve {}", amount, reserve.token_mint);

    Ok(())
}

/// Deposit errors
#[error_code]
pub enum DepositError {
    #[msg("Emergency mode is active, deposits disabled")]
    EmergencyModeActive,

    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Deposits are disabled for this reserve")]
    DepositsDisabled,

    #[msg("Obligation does not belong to this lending market")]
    InvalidObligation,

    #[msg("Obligation owner mismatch")]
    InvalidObligationOwner,

    #[msg("Token mint mismatch")]
    InvalidTokenMint,

    #[msg("Token account owner mismatch")]
    InvalidTokenOwner,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Deposit amount cannot be zero")]
    AmountZero,

    #[msg("Deposit amount too small")]
    AmountTooSmall,

    #[msg("Deposit limit exceeded")]
    DepositLimitExceeded,

    #[msg("Maximum deposits per obligation reached")]
    MaxDepositsReached,

    #[msg("Reserve data is stale, refresh required")]
    ReserveStale,

    #[msg("Math overflow")]
    MathOverflow,
}
