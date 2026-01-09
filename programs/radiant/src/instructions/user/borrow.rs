use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::{LendingMarket, Reserve, Obligation, ObligationLiquidity};
use crate::constants::{VAULT_SEED, MAX_OBLIGATION_BORROWS, MIN_BORROW_AMOUNT, MIN_HEALTH_FACTOR_AFTER_BORROW, MAX_RESERVE_STALENESS_SLOTS};
use crate::events::BorrowEvent;

/// Accounts for borrowing tokens
#[derive(Accounts)]
pub struct Borrow<'info> {
    /// User borrowing tokens
    pub owner: Signer<'info>,

    /// The lending market
    #[account(
        constraint = !lending_market.emergency_mode @ BorrowError::EmergencyModeActive,
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve to borrow from
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ BorrowError::InvalidReserve,
        constraint = reserve.config.borrows_enabled @ BorrowError::BorrowsDisabled
    )]
    pub reserve: Account<'info, Reserve>,

    /// User's obligation account
    #[account(
        mut,
        constraint = obligation.lending_market == lending_market.key() @ BorrowError::InvalidObligation,
        constraint = obligation.owner == owner.key() @ BorrowError::InvalidObligationOwner,
        seeds = [Obligation::SEED_PREFIX, lending_market.key().as_ref(), owner.key().as_ref()],
        bump = obligation.bump
    )]
    pub obligation: Account<'info, Obligation>,

    /// Reserve's vault (source)
    #[account(
        mut,
        seeds = [VAULT_SEED, reserve.key().as_ref()],
        bump,
        constraint = token_vault.key() == reserve.token_vault @ BorrowError::InvalidVault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    /// User's token account (destination)
    #[account(
        mut,
        constraint = user_token_account.mint == reserve.token_mint @ BorrowError::InvalidTokenMint,
        constraint = user_token_account.owner == owner.key() @ BorrowError::InvalidTokenOwner
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Token program
    pub token_program: Program<'info, Token>,
}

/// Borrow tokens from the reserve
///
/// User must have sufficient collateral to cover the borrow.
/// The borrow amount is limited by:
/// - User's borrowing capacity (collateral * LTV)
/// - Available liquidity in the reserve
/// - Reserve's borrow limit
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `amount` - Amount of tokens to borrow (in native units)
pub fn handler(ctx: Context<Borrow>, amount: u64) -> Result<()> {
    // Validate amount
    require!(amount > 0, BorrowError::AmountZero);
    require!(amount >= MIN_BORROW_AMOUNT, BorrowError::AmountTooSmall);

    let reserve = &mut ctx.accounts.reserve;
    let obligation = &mut ctx.accounts.obligation;
    let clock = Clock::get()?;

    // Check reserve is not stale
    require!(
        !reserve.is_stale(clock.slot, MAX_RESERVE_STALENESS_SLOTS),
        BorrowError::ReserveStale
    );

    // User must have deposits (collateral)
    require!(
        obligation.has_deposits(),
        BorrowError::NoCollateral
    );

    // Check borrow limit if set
    if reserve.config.borrow_limit > 0 {
        let new_total_borrows = reserve.liquidity.total_borrows
            .checked_add(amount)
            .ok_or(BorrowError::MathOverflow)?;
        require!(
            new_total_borrows <= reserve.config.borrow_limit,
            BorrowError::BorrowLimitExceeded
        );
    }

    // Check available liquidity
    let available_liquidity = reserve.available_liquidity();
    require!(
        amount <= available_liquidity,
        BorrowError::InsufficientLiquidity
    );

    // Check borrowing capacity
    // Note: In production, this should use oracle prices for proper USD calculations
    // For now, we use the cached values from refresh_obligation
    let remaining_capacity = obligation.remaining_borrow_capacity_usd();
    require!(
        remaining_capacity > 0,
        BorrowError::InsufficientBorrowingCapacity
    );

    // Verify vault has sufficient balance
    require!(
        ctx.accounts.token_vault.amount >= amount,
        BorrowError::InsufficientVaultBalance
    );

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
    token::transfer(transfer_ctx, amount)?;

    // Update reserve liquidity
    reserve.liquidity.total_borrows = reserve.liquidity.total_borrows
        .checked_add(amount)
        .ok_or(BorrowError::MathOverflow)?;

    // Update obligation
    let reserve_key = reserve.key();
    let current_borrow_index = reserve.liquidity.cumulative_borrow_index;

    // Check if user already has a borrow from this reserve
    if let Some(borrow_index) = obligation.find_borrow(&reserve_key) {
        // Update existing borrow
        let borrow = &mut obligation.borrows[borrow_index];

        // Calculate current value with interest, then add new borrow
        let current_borrow_amount = (borrow.borrowed_amount as u128 * current_borrow_index)
            / borrow.borrow_index_snapshot;

        let new_amount = current_borrow_amount
            .checked_add(amount as u128)
            .ok_or(BorrowError::MathOverflow)?;

        // Store new amount with current index as snapshot
        borrow.borrowed_amount = new_amount as u64;
        borrow.borrow_index_snapshot = current_borrow_index;
    } else {
        // Create new borrow entry
        require!(
            obligation.borrows.len() < MAX_OBLIGATION_BORROWS,
            BorrowError::MaxBorrowsReached
        );

        obligation.borrows.push(ObligationLiquidity::new(
            reserve_key,
            amount,
            current_borrow_index,
        ));
    }

    // Validate final health factor after borrow
    // This ensures user maintains a safe distance from liquidation
    if obligation.borrowed_value_usd > 0 {
        let health_factor = obligation.calculate_health_factor();
        match health_factor {
            Some(hf) => {
                require!(
                    hf >= MIN_HEALTH_FACTOR_AFTER_BORROW,
                    BorrowError::InsufficientHealthFactor
                );
            },
            None => {
                // No debt, should not happen here but safe
            }
        }
    }

    // Update interest rates based on new utilization
    let utilization_bps = reserve.calculate_utilization_bps();
    let borrow_rate = reserve.config.interest_rate_config.calculate_borrow_rate(utilization_bps);
    let supply_rate = reserve.config.interest_rate_config.calculate_supply_rate(borrow_rate, utilization_bps);

    reserve.liquidity.current_borrow_rate_bps = borrow_rate;
    reserve.liquidity.current_supply_rate_bps = supply_rate;

    // Update timestamps
    reserve.last_update_slot = clock.slot;
    reserve.last_update_timestamp = clock.unix_timestamp;
    obligation.last_update_slot = clock.slot;

    // Get new borrow amount for event
    let new_borrow_amount = if let Some(idx) = obligation.find_borrow(&reserve_key) {
        obligation.borrows[idx].borrowed_amount
    } else {
        0
    };

    // Emit borrow event
    emit!(BorrowEvent {
        lending_market: ctx.accounts.lending_market.key(),
        reserve: reserve_key,
        obligation: obligation.key(),
        owner: ctx.accounts.owner.key(),
        amount,
        new_borrow_amount,
        new_utilization_bps: utilization_bps,
        new_borrow_rate_bps: borrow_rate,
        timestamp: clock.unix_timestamp,
    });

    msg!("Borrowed {} tokens from reserve {}", amount, reserve.token_mint);
    msg!("New utilization: {} bps, Borrow rate: {} bps", utilization_bps, borrow_rate);

    Ok(())
}

/// Borrow errors
#[error_code]
pub enum BorrowError {
    #[msg("Emergency mode is active, borrows disabled")]
    EmergencyModeActive,

    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Borrows are disabled for this reserve")]
    BorrowsDisabled,

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

    #[msg("Borrow amount cannot be zero")]
    AmountZero,

    #[msg("Borrow amount too small")]
    AmountTooSmall,

    #[msg("No collateral deposited")]
    NoCollateral,

    #[msg("Borrow limit exceeded")]
    BorrowLimitExceeded,

    #[msg("Insufficient liquidity in reserve")]
    InsufficientLiquidity,

    #[msg("Insufficient borrowing capacity")]
    InsufficientBorrowingCapacity,

    #[msg("Insufficient health factor after borrow - would be too close to liquidation")]
    InsufficientHealthFactor,

    #[msg("Maximum borrows per obligation reached")]
    MaxBorrowsReached,

    #[msg("Reserve data is stale, refresh required")]
    ReserveStale,

    #[msg("Insufficient balance in vault")]
    InsufficientVaultBalance,

    #[msg("Math overflow")]
    MathOverflow,
}
