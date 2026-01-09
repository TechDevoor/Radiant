use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Obligation};
use crate::events::ObligationInitialized;

/// Accounts for initializing a user's obligation
#[derive(Accounts)]
pub struct InitializeObligation<'info> {
    /// User who owns this obligation
    #[account(mut)]
    pub owner: Signer<'info>,

    /// The lending market
    #[account(
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The obligation account to initialize
    /// PDA: ["obligation", lending_market, owner]
    #[account(
        init,
        payer = owner,
        space = 8 + Obligation::INIT_SPACE,
        seeds = [Obligation::SEED_PREFIX, lending_market.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub obligation: Account<'info, Obligation>,

    /// System program
    pub system_program: Program<'info, System>,
}

/// Initialize a user's obligation account
///
/// An obligation tracks a user's deposits and borrows in the lending market.
/// Each user has one obligation per lending market.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
pub fn handler(ctx: Context<InitializeObligation>) -> Result<()> {
    let obligation = &mut ctx.accounts.obligation;

    // Set version
    obligation.version = 1;

    // Store bump for PDA verification
    obligation.bump = ctx.bumps.obligation;

    // Link to lending market and owner
    obligation.lending_market = ctx.accounts.lending_market.key();
    obligation.owner = ctx.accounts.owner.key();

    // Set last update slot
    let clock = Clock::get()?;
    obligation.last_update_slot = clock.slot;

    // Initialize empty deposits and borrows
    obligation.deposits = Vec::new();
    obligation.borrows = Vec::new();

    // Initialize cached values to zero
    obligation.deposited_value_usd = 0;
    obligation.borrowed_value_usd = 0;
    obligation.allowed_borrow_value_usd = 0;
    obligation.unhealthy_borrow_value_usd = 0;

    // Initialize padding
    obligation._padding = [0u8; 64];

    // Emit event
    emit!(ObligationInitialized {
        lending_market: obligation.lending_market,
        obligation: obligation.key(),
        owner: obligation.owner,
    });

    msg!("Obligation initialized for user: {}", obligation.owner);

    Ok(())
}
