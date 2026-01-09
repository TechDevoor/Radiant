use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::state::{
    LendingMarket,
    Reserve,
    ReserveConfig,
    ReserveLiquidity,
    InterestRateConfig,
};
use crate::constants::{
    INDEX_ONE,
    MAX_RESERVES,
    VAULT_SEED,
    FEE_RECEIVER_SEED,
    DEFAULT_OPTIMAL_UTILIZATION_BPS,
    DEFAULT_BASE_RATE_BPS,
    DEFAULT_SLOPE1_BPS,
    DEFAULT_SLOPE2_BPS,
    DEFAULT_RESERVE_FACTOR_BPS,
};
use crate::events::ReserveInitialized;

/// Accounts for initializing a new reserve
#[derive(Accounts)]
pub struct InitializeReserve<'info> {
    /// Authority of the lending market (must sign)
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The lending market this reserve belongs to
    #[account(
        mut,
        has_one = authority,
        seeds = [LendingMarket::SEED_PREFIX, authority.key().as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve account to initialize
    /// PDA: ["reserve", lending_market, token_mint]
    #[account(
        init,
        payer = authority,
        space = 8 + Reserve::INIT_SPACE,
        seeds = [Reserve::SEED_PREFIX, lending_market.key().as_ref(), token_mint.key().as_ref()],
        bump
    )]
    pub reserve: Account<'info, Reserve>,

    /// The token mint for this reserve (e.g., USDC, SOL)
    pub token_mint: Account<'info, Mint>,

    /// Token vault to hold deposited tokens
    /// PDA: ["vault", reserve]
    #[account(
        init,
        payer = authority,
        seeds = [VAULT_SEED, reserve.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = reserve
    )]
    pub token_vault: Account<'info, TokenAccount>,

    /// Fee receiver token account
    /// PDA: ["fee_receiver", reserve]
    #[account(
        init,
        payer = authority,
        seeds = [FEE_RECEIVER_SEED, reserve.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = lending_market
    )]
    pub fee_receiver: Account<'info, TokenAccount>,

    /// Pyth oracle price feed for this asset
    /// CHECK: Validated in handler (must be valid Pyth account)
    pub oracle: UncheckedAccount<'info>,

    /// Token program
    pub token_program: Program<'info, Token>,

    /// System program
    pub system_program: Program<'info, System>,

    /// Rent sysvar
    pub rent: Sysvar<'info, Rent>,
}

/// Parameters for initializing a reserve
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitializeReserveParams {
    /// Loan-to-Value ratio in BPS (e.g., 8000 = 80%)
    pub ltv_bps: u16,

    /// Liquidation threshold in BPS (e.g., 8500 = 85%)
    pub liquidation_threshold_bps: u16,

    /// Optional: Maximum deposit limit (0 = unlimited)
    pub deposit_limit: Option<u64>,

    /// Optional: Maximum borrow limit (0 = unlimited)
    pub borrow_limit: Option<u64>,

    /// Optional: Interest rate config (uses defaults if not provided)
    pub interest_rate_config: Option<InterestRateConfigParams>,
}

/// Interest rate configuration parameters
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InterestRateConfigParams {
    pub optimal_utilization_bps: u16,
    pub base_rate_bps: u16,
    pub slope1_bps: u16,
    pub slope2_bps: u16,
    pub reserve_factor_bps: u16,
}

/// Initialize a new reserve (asset pool)
///
/// Creates a new liquidity pool for a specific token.
/// Each token can only have one reserve per lending market.
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `params` - Reserve configuration parameters
pub fn handler(
    ctx: Context<InitializeReserve>,
    params: InitializeReserveParams,
) -> Result<()> {
    // Validate LTV < liquidation threshold
    require!(
        params.ltv_bps < params.liquidation_threshold_bps,
        ReserveError::InvalidLtvThreshold
    );

    // Validate liquidation threshold <= 100%
    require!(
        params.liquidation_threshold_bps <= 10000,
        ReserveError::InvalidLiquidationThreshold
    );

    // Check max reserves limit
    require!(
        ctx.accounts.lending_market.reserves_count < MAX_RESERVES,
        ReserveError::MaxReservesReached
    );

    let reserve = &mut ctx.accounts.reserve;
    let clock = Clock::get()?;

    // Basic info
    reserve.version = 1;
    reserve.bump = ctx.bumps.reserve;
    reserve.lending_market = ctx.accounts.lending_market.key();
    reserve.token_mint = ctx.accounts.token_mint.key();
    reserve.token_decimals = ctx.accounts.token_mint.decimals;

    // Token accounts
    reserve.token_vault = ctx.accounts.token_vault.key();
    reserve.fee_receiver = ctx.accounts.fee_receiver.key();

    // Oracle
    reserve.oracle = ctx.accounts.oracle.key();

    // Timestamps
    reserve.last_update_slot = clock.slot;
    reserve.last_update_timestamp = clock.unix_timestamp;

    // Configuration
    let interest_config = params.interest_rate_config
        .map(|c| InterestRateConfig {
            optimal_utilization_bps: c.optimal_utilization_bps,
            base_rate_bps: c.base_rate_bps,
            slope1_bps: c.slope1_bps,
            slope2_bps: c.slope2_bps,
            reserve_factor_bps: c.reserve_factor_bps,
        })
        .unwrap_or(InterestRateConfig {
            optimal_utilization_bps: DEFAULT_OPTIMAL_UTILIZATION_BPS,
            base_rate_bps: DEFAULT_BASE_RATE_BPS,
            slope1_bps: DEFAULT_SLOPE1_BPS,
            slope2_bps: DEFAULT_SLOPE2_BPS,
            reserve_factor_bps: DEFAULT_RESERVE_FACTOR_BPS,
        });

    reserve.config = ReserveConfig {
        ltv_bps: params.ltv_bps,
        liquidation_threshold_bps: params.liquidation_threshold_bps,
        deposit_limit: params.deposit_limit.unwrap_or(0),
        borrow_limit: params.borrow_limit.unwrap_or(0),
        deposits_enabled: true,
        borrows_enabled: true,
        interest_rate_config: interest_config,
    };

    // Validate the config
    require!(
        Reserve::validate_config(&reserve.config),
        ReserveError::InvalidReserveConfig
    );

    // Initialize liquidity state
    reserve.liquidity = ReserveLiquidity {
        total_deposits: 0,
        total_borrows: 0,
        accumulated_protocol_fees: 0,
        cumulative_borrow_index: INDEX_ONE,  // Start at 1.0 (10^18)
        cumulative_supply_index: INDEX_ONE,  // Start at 1.0 (10^18)
        current_borrow_rate_bps: 0,
        current_supply_rate_bps: 0,
    };

    // Initialize padding
    reserve._padding = [0u8; 128];

    // Increment reserves count
    ctx.accounts.lending_market.reserves_count += 1;

    // Emit event
    emit!(ReserveInitialized {
        lending_market: reserve.lending_market,
        reserve: reserve.key(),
        token_mint: reserve.token_mint,
        ltv_bps: reserve.config.ltv_bps,
        liquidation_threshold_bps: reserve.config.liquidation_threshold_bps,
    });

    msg!("Reserve initialized for mint: {}", reserve.token_mint);
    msg!("LTV: {} bps, Liquidation threshold: {} bps",
        reserve.config.ltv_bps,
        reserve.config.liquidation_threshold_bps
    );

    Ok(())
}

/// Errors for reserve initialization
#[error_code]
pub enum ReserveError {
    #[msg("LTV must be less than liquidation threshold")]
    InvalidLtvThreshold,

    #[msg("Liquidation threshold must be <= 10000 bps (100%)")]
    InvalidLiquidationThreshold,

    #[msg("Maximum number of reserves reached")]
    MaxReservesReached,

    #[msg("Invalid reserve configuration")]
    InvalidReserveConfig,
}
