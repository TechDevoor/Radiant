use anchor_lang::prelude::*;

use crate::state::LendingMarket;
use crate::constants::{
    DEFAULT_CLOSE_FACTOR_BPS,
    DEFAULT_LIQUIDATION_BONUS_BPS,
    DEFAULT_PROTOCOL_FEE_BPS,
};
use crate::events::LendingMarketInitialized;

/// Accounts for initializing a new lending market
#[derive(Accounts)]
pub struct InitializeLendingMarket<'info> {
    /// Authority who will manage the lending market
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The lending market account to initialize
    /// PDA: ["lending_market", authority]
    #[account(
        init,
        payer = authority,
        space = 8 + LendingMarket::INIT_SPACE,
        seeds = [LendingMarket::SEED_PREFIX, authority.key().as_ref()],
        bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// Treasury account that will receive protocol fees
    /// CHECK: This can be any account, validated by authority
    pub treasury: UncheckedAccount<'info>,

    /// System program for account creation
    pub system_program: Program<'info, System>,
}

/// Parameters for initializing a lending market
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitializeLendingMarketParams {
    /// Close factor in BPS (optional, defaults to 50%)
    pub close_factor_bps: Option<u16>,
    /// Liquidation bonus in BPS (optional, defaults to 5%)
    pub liquidation_bonus_bps: Option<u16>,
    /// Protocol fee in BPS (optional, defaults to 10%)
    pub protocol_fee_bps: Option<u16>,
}

/// Initialize a new lending market
///
/// This creates the global configuration for the lending protocol.
/// Only one lending market can exist per authority.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `params` - Optional configuration parameters
///
/// # Returns
/// * `Ok(())` if successful
pub fn handler(
    ctx: Context<InitializeLendingMarket>,
    params: InitializeLendingMarketParams,
) -> Result<()> {
    let lending_market = &mut ctx.accounts.lending_market;

    // Set version for future upgrades
    lending_market.version = 1;

    // Store bump for PDA verification
    lending_market.bump = ctx.bumps.lending_market;

    // Set authority and treasury
    lending_market.authority = ctx.accounts.authority.key();
    lending_market.treasury = ctx.accounts.treasury.key();

    // Initialize with defaults or provided values
    lending_market.emergency_mode = false;

    // Close factor: max % of debt liquidatable at once
    let close_factor = params.close_factor_bps.unwrap_or(DEFAULT_CLOSE_FACTOR_BPS);
    require!(
        LendingMarket::validate_close_factor(close_factor),
        LendingMarketError::InvalidCloseFactor
    );
    lending_market.close_factor_bps = close_factor;

    // Liquidation bonus: reward for liquidators
    let liq_bonus = params.liquidation_bonus_bps.unwrap_or(DEFAULT_LIQUIDATION_BONUS_BPS);
    require!(
        LendingMarket::validate_liquidation_bonus(liq_bonus),
        LendingMarketError::InvalidLiquidationBonus
    );
    lending_market.liquidation_bonus_bps = liq_bonus;

    // Protocol fee: protocol's cut of liquidation bonus
    let protocol_fee = params.protocol_fee_bps.unwrap_or(DEFAULT_PROTOCOL_FEE_BPS);
    require!(
        protocol_fee <= 10000,
        LendingMarketError::InvalidProtocolFee
    );
    lending_market.protocol_fee_bps = protocol_fee;

    // No reserves yet
    lending_market.reserves_count = 0;

    // Initialize padding to zeros
    lending_market._padding = [0u8; 128];

    // Emit event
    emit!(LendingMarketInitialized {
        lending_market: lending_market.key(),
        authority: lending_market.authority,
        treasury: lending_market.treasury,
        close_factor_bps: lending_market.close_factor_bps,
        liquidation_bonus_bps: lending_market.liquidation_bonus_bps,
    });

    msg!("Lending market initialized");
    msg!("Authority: {}", lending_market.authority);
    msg!("Treasury: {}", lending_market.treasury);
    msg!("Close factor: {} bps", lending_market.close_factor_bps);
    msg!("Liquidation bonus: {} bps", lending_market.liquidation_bonus_bps);

    Ok(())
}

/// Errors for lending market initialization
#[error_code]
pub enum LendingMarketError {
    #[msg("Close factor must be between 0 and 10000 bps (0-100%)")]
    InvalidCloseFactor,

    #[msg("Liquidation bonus must be between 0 and 2500 bps (0-25%)")]
    InvalidLiquidationBonus,

    #[msg("Protocol fee must be between 0 and 10000 bps (0-100%)")]
    InvalidProtocolFee,
}
