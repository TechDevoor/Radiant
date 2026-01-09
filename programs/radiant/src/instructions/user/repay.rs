use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::{LendingMarket, Reserve, Obligation};
use crate::constants::{VAULT_SEED, MAX_RESERVE_STALENESS_SLOTS};
use crate::events::RepayEvent;

/// Accounts for repaying borrowed tokens
#[derive(Accounts)]
pub struct Repay<'info> {
    /// User repaying the loan (can be anyone, not just the borrower)
    pub payer: Signer<'info>,

    /// The lending market
    #[account(
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve being repaid to
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ RepayError::InvalidReserve
    )]
    pub reserve: Account<'info, Reserve>,

    /// Borrower's obligation account
    #[account(
        mut,
        constraint = obligation.lending_market == lending_market.key() @ RepayError::InvalidObligation,
        seeds = [Obligation::SEED_PREFIX, lending_market.key().as_ref(), obligation.owner.as_ref()],
        bump = obligation.bump
    )]
    pub obligation: Account<'info, Obligation>,

    /// Payer's token account (source)
    #[account(
        mut,
        constraint = payer_token_account.mint == reserve.token_mint @ RepayError::InvalidTokenMint,
        constraint = payer_token_account.owner == payer.key() @ RepayError::InvalidTokenOwner
    )]
    pub payer_token_account: Account<'info, TokenAccount>,

    /// Reserve's vault (destination)
    #[account(
        mut,
        seeds = [VAULT_SEED, reserve.key().as_ref()],
        bump,
        constraint = token_vault.key() == reserve.token_vault @ RepayError::InvalidVault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    /// Token program
    pub token_program: Program<'info, Token>,
}

/// Repay borrowed tokens
///
/// Anyone can repay on behalf of a borrower.
/// If amount is 0 or greater than debt, repays full debt.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `amount` - Amount to repay (in native units), 0 = repay all
pub fn handler(ctx: Context<Repay>, amount: u64) -> Result<()> {
    let reserve = &mut ctx.accounts.reserve;
    let obligation = &mut ctx.accounts.obligation;
    let reserve_key = reserve.key();
    let clock = Clock::get()?;

    // Check reserve is not stale
    require!(
        !reserve.is_stale(clock.slot, MAX_RESERVE_STALENESS_SLOTS),
        RepayError::ReserveStale
    );

    // Find user's borrow in this reserve
    let borrow_index = obligation
        .find_borrow(&reserve_key)
        .ok_or(RepayError::NoBorrowFound)?;

    let current_borrow_index = reserve.liquidity.cumulative_borrow_index;

    // Calculate current borrow value with accrued interest
    let borrow = &obligation.borrows[borrow_index];
    let current_borrow_amount = if borrow.borrow_index_snapshot > 0 {
        (borrow.borrowed_amount as u128 * current_borrow_index / borrow.borrow_index_snapshot) as u64
    } else {
        borrow.borrowed_amount
    };

    require!(current_borrow_amount > 0, RepayError::NothingToRepay);

    // Determine repay amount (0 = repay all)
    let repay_amount = if amount == 0 || amount >= current_borrow_amount {
        current_borrow_amount
    } else {
        amount
    };

    // Transfer tokens from payer to vault
    let transfer_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.token_vault.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        },
    );
    token::transfer(transfer_ctx, repay_amount)?;

    // Update reserve liquidity
    reserve.liquidity.total_borrows = reserve.liquidity.total_borrows
        .checked_sub(repay_amount)
        .ok_or(RepayError::MathOverflow)?;

    // Calculate remaining borrow after repayment
    let remaining_borrow = current_borrow_amount
        .checked_sub(repay_amount)
        .ok_or(RepayError::MathOverflow)?;

    // Update or remove obligation borrow
    if remaining_borrow == 0 {
        // Remove the borrow entry
        obligation.borrows.remove(borrow_index);
    } else {
        // Update the borrow with remaining amount
        let borrow = &mut obligation.borrows[borrow_index];
        borrow.borrowed_amount = remaining_borrow;
        borrow.borrow_index_snapshot = current_borrow_index;
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

    // Emit repay event
    emit!(RepayEvent {
        lending_market: ctx.accounts.lending_market.key(),
        reserve: reserve_key,
        obligation: obligation.key(),
        payer: ctx.accounts.payer.key(),
        owner: obligation.owner,
        amount: repay_amount,
        remaining_borrow,
        new_utilization_bps: utilization_bps,
        new_borrow_rate_bps: borrow_rate,
        timestamp: clock.unix_timestamp,
    });

    msg!("Repaid {} tokens to reserve {}", repay_amount, reserve.token_mint);
    msg!("Remaining debt: {}", remaining_borrow);
    msg!("New utilization: {} bps, Borrow rate: {} bps", utilization_bps, borrow_rate);

    Ok(())
}

/// Repay errors
#[error_code]
pub enum RepayError {
    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Obligation does not belong to this lending market")]
    InvalidObligation,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Token mint mismatch")]
    InvalidTokenMint,

    #[msg("Token account owner mismatch")]
    InvalidTokenOwner,

    #[msg("No borrow found for this reserve")]
    NoBorrowFound,

    #[msg("Nothing to repay")]
    NothingToRepay,

    #[msg("Reserve data is stale, refresh required")]
    ReserveStale,

    #[msg("Math overflow")]
    MathOverflow,
}
