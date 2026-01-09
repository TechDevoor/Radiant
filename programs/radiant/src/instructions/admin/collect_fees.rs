use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::{LendingMarket, Reserve};
use crate::events::ProtocolFeesCollected;

/// Accounts for collecting accumulated protocol fees
#[derive(Accounts)]
pub struct CollectFees<'info> {
    /// Authority of the lending market (must sign)
    pub authority: Signer<'info>,

    /// The lending market
    #[account(
        has_one = authority,
        has_one = treasury,
        seeds = [LendingMarket::SEED_PREFIX, authority.key().as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve to collect fees from
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ CollectFeesError::InvalidReserve
    )]
    pub reserve: Account<'info, Reserve>,

    /// Reserve's token vault (source of fees)
    #[account(
        mut,
        constraint = reserve_vault.key() == reserve.token_vault @ CollectFeesError::InvalidVault
    )]
    pub reserve_vault: Account<'info, TokenAccount>,

    /// Treasury token account (destination for fees)
    /// Must be owned by the treasury and match reserve's token mint
    #[account(
        mut,
        constraint = treasury_token_account.mint == reserve.token_mint @ CollectFeesError::InvalidTokenMint,
        constraint = treasury_token_account.owner == treasury.key() @ CollectFeesError::InvalidTreasuryOwner
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    /// Treasury account (must match lending_market.treasury)
    /// CHECK: Validated by has_one constraint on lending_market
    pub treasury: UncheckedAccount<'info>,

    /// Token program
    pub token_program: Program<'info, Token>,
}

/// Collect accumulated protocol fees from a reserve
///
/// Transfers accumulated fees from the reserve vault to the treasury.
/// Only the lending market authority can call this.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `amount` - Amount of fees to collect (0 = collect all)
pub fn handler(ctx: Context<CollectFees>, amount: u64) -> Result<()> {
    let reserve = &mut ctx.accounts.reserve;

    // Get available fees
    let available_fees = reserve.liquidity.accumulated_protocol_fees;
    require!(available_fees > 0, CollectFeesError::NoFeesToCollect);

    // Determine amount to collect (0 = all)
    let collect_amount = if amount == 0 || amount > available_fees {
        available_fees
    } else {
        amount
    };

    // Transfer fees from vault to treasury using PDA signer
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
            from: ctx.accounts.reserve_vault.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: reserve.to_account_info(),
        },
        signer_seeds,
    );
    token::transfer(transfer_ctx, collect_amount)?;

    // Update accumulated fees
    reserve.liquidity.accumulated_protocol_fees = available_fees
        .checked_sub(collect_amount)
        .ok_or(CollectFeesError::MathOverflow)?;

    // Update timestamp
    let clock = Clock::get()?;
    reserve.last_update_slot = clock.slot;
    reserve.last_update_timestamp = clock.unix_timestamp;

    // Emit event
    emit!(ProtocolFeesCollected {
        reserve: reserve.key(),
        amount: collect_amount,
        recipient: ctx.accounts.treasury.key(),
        timestamp: clock.unix_timestamp,
    });

    msg!("Collected {} protocol fees from reserve {}", collect_amount, reserve.token_mint);
    msg!("Remaining fees: {}", reserve.liquidity.accumulated_protocol_fees);

    Ok(())
}

/// Collect fees errors
#[error_code]
pub enum CollectFeesError {
    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Token mint mismatch")]
    InvalidTokenMint,

    #[msg("Treasury token account owner mismatch")]
    InvalidTreasuryOwner,

    #[msg("No fees to collect")]
    NoFeesToCollect,

    #[msg("Math overflow")]
    MathOverflow,
}
