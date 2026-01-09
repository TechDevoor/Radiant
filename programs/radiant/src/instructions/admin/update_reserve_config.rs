use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Reserve};
use crate::events::ReserveConfigUpdated;

/// Accounts for updating reserve configuration
#[derive(Accounts)]
pub struct UpdateReserveConfig<'info> {
    /// Authority of the lending market (must sign)
    pub authority: Signer<'info>,

    /// The lending market
    #[account(
        has_one = authority,
        seeds = [LendingMarket::SEED_PREFIX, authority.key().as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve to update
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ UpdateConfigError::InvalidReserve
    )]
    pub reserve: Account<'info, Reserve>,
}

/// Parameters for updating reserve config
/// All fields are optional - only provided fields will be updated
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct UpdateReserveConfigParams {
    /// New LTV ratio in BPS
    pub ltv_bps: Option<u16>,

    /// New liquidation threshold in BPS
    pub liquidation_threshold_bps: Option<u16>,

    /// New deposit limit (0 = unlimited)
    pub deposit_limit: Option<u64>,

    /// New borrow limit (0 = unlimited)
    pub borrow_limit: Option<u64>,

    /// Enable/disable deposits
    pub deposits_enabled: Option<bool>,

    /// Enable/disable borrows
    pub borrows_enabled: Option<bool>,

    /// New optimal utilization in BPS
    pub optimal_utilization_bps: Option<u16>,

    /// New base rate in BPS
    pub base_rate_bps: Option<u16>,

    /// New slope1 in BPS
    pub slope1_bps: Option<u16>,

    /// New slope2 in BPS
    pub slope2_bps: Option<u16>,

    /// New reserve factor in BPS
    pub reserve_factor_bps: Option<u16>,
}

/// Update reserve configuration
///
/// Allows admin to modify reserve parameters.
/// Only provided fields will be updated.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `params` - Optional configuration updates
pub fn handler(
    ctx: Context<UpdateReserveConfig>,
    params: UpdateReserveConfigParams,
) -> Result<()> {
    let reserve = &mut ctx.accounts.reserve;

    // Build new config with updates
    let mut new_config = reserve.config.clone();

    // Update risk parameters
    if let Some(ltv) = params.ltv_bps {
        new_config.ltv_bps = ltv;
    }

    if let Some(liq_threshold) = params.liquidation_threshold_bps {
        new_config.liquidation_threshold_bps = liq_threshold;
    }

    // Validate LTV < liquidation threshold
    require!(
        new_config.ltv_bps < new_config.liquidation_threshold_bps,
        UpdateConfigError::InvalidLtvThreshold
    );

    require!(
        new_config.liquidation_threshold_bps <= 10000,
        UpdateConfigError::InvalidLiquidationThreshold
    );

    // Update limits
    if let Some(deposit_limit) = params.deposit_limit {
        new_config.deposit_limit = deposit_limit;
    }

    if let Some(borrow_limit) = params.borrow_limit {
        new_config.borrow_limit = borrow_limit;
    }

    // Update flags
    if let Some(deposits_enabled) = params.deposits_enabled {
        new_config.deposits_enabled = deposits_enabled;
    }

    if let Some(borrows_enabled) = params.borrows_enabled {
        new_config.borrows_enabled = borrows_enabled;
    }

    // Update interest rate config
    let mut new_ir_config = new_config.interest_rate_config;

    if let Some(optimal_util) = params.optimal_utilization_bps {
        require!(optimal_util <= 10000, UpdateConfigError::InvalidOptimalUtilization);
        new_ir_config.optimal_utilization_bps = optimal_util;
    }

    if let Some(base_rate) = params.base_rate_bps {
        new_ir_config.base_rate_bps = base_rate;
    }

    if let Some(slope1) = params.slope1_bps {
        new_ir_config.slope1_bps = slope1;
    }

    if let Some(slope2) = params.slope2_bps {
        new_ir_config.slope2_bps = slope2;
    }

    if let Some(reserve_factor) = params.reserve_factor_bps {
        require!(reserve_factor <= 10000, UpdateConfigError::InvalidReserveFactor);
        new_ir_config.reserve_factor_bps = reserve_factor;
    }

    new_config.interest_rate_config = new_ir_config;

    // Final validation
    require!(
        Reserve::validate_config(&new_config),
        UpdateConfigError::InvalidReserveConfig
    );

    // Apply the new config
    reserve.config = new_config;

    // Emit event
    emit!(ReserveConfigUpdated {
        reserve: reserve.key(),
        ltv_bps: reserve.config.ltv_bps,
        liquidation_threshold_bps: reserve.config.liquidation_threshold_bps,
        deposit_limit: reserve.config.deposit_limit,
        borrow_limit: reserve.config.borrow_limit,
    });

    msg!("Reserve config updated for: {}", reserve.token_mint);

    Ok(())
}

/// Errors for config updates
#[error_code]
pub enum UpdateConfigError {
    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("LTV must be less than liquidation threshold")]
    InvalidLtvThreshold,

    #[msg("Liquidation threshold must be <= 10000 bps")]
    InvalidLiquidationThreshold,

    #[msg("Optimal utilization must be <= 10000 bps")]
    InvalidOptimalUtilization,

    #[msg("Reserve factor must be <= 10000 bps")]
    InvalidReserveFactor,

    #[msg("Invalid reserve configuration")]
    InvalidReserveConfig,
}
