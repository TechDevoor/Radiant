use anchor_lang::prelude::*;

/// Per-asset liquidity pool configuration and state
/// PDA Seeds: ["reserve", lending_market, token_mint]
#[account]
#[derive(InitSpace)]
pub struct Reserve {
    /// Version for future upgrades
    pub version: u8,

    /// Bump seed for PDA derivation
    pub bump: u8,

    /// The lending market this reserve belongs to
    pub lending_market: Pubkey,

    /// Token mint for this reserve (e.g., USDC mint, SOL mint)
    pub token_mint: Pubkey,

    /// Token decimals (cached for calculations)
    pub token_decimals: u8,

    /// Vault holding the deposited tokens (PDA-owned token account)
    pub token_vault: Pubkey,

    /// Fee receiver token account for this reserve
    pub fee_receiver: Pubkey,

    /// Pyth oracle price feed for this asset
    pub oracle: Pubkey,

    /// Last slot when reserve was refreshed
    pub last_update_slot: u64,

    /// Last timestamp when reserve was refreshed
    pub last_update_timestamp: i64,

    /// Reserve configuration parameters
    pub config: ReserveConfig,

    /// Current liquidity state
    pub liquidity: ReserveLiquidity,

    /// Reserved space for future upgrades (128 bytes)
    pub _padding: [u8; 128],
}

/// Configuration parameters for a reserve
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace, Default)]
pub struct ReserveConfig {
    /// Loan-to-Value ratio in BPS (max borrow power)
    /// e.g., 8000 = 80% - can borrow up to 80% of collateral value
    pub ltv_bps: u16,

    /// Liquidation threshold in BPS
    /// e.g., 8500 = 85% - liquidation starts when debt/collateral > 85%
    pub liquidation_threshold_bps: u16,

    /// Maximum deposit limit for this reserve (0 = unlimited)
    pub deposit_limit: u64,

    /// Maximum borrow limit for this reserve (0 = unlimited)
    pub borrow_limit: u64,

    /// Whether deposits are enabled
    pub deposits_enabled: bool,

    /// Whether borrows are enabled
    pub borrows_enabled: bool,

    /// Interest rate model configuration
    pub interest_rate_config: InterestRateConfig,
}

/// Kinked interest rate model configuration
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace, Default)]
pub struct InterestRateConfig {
    /// Optimal utilization rate in BPS
    /// e.g., 8000 = 80%
    pub optimal_utilization_bps: u16,

    /// Base borrow rate in BPS (rate at 0% utilization)
    /// e.g., 200 = 2%
    pub base_rate_bps: u16,

    /// Slope 1: rate increase from 0% to optimal utilization (BPS)
    /// e.g., 1000 = 10%
    pub slope1_bps: u16,

    /// Slope 2: rate increase from optimal to 100% utilization (BPS)
    /// e.g., 10000 = 100%
    pub slope2_bps: u16,

    /// Reserve factor in BPS (protocol's cut of interest)
    /// e.g., 1000 = 10%
    pub reserve_factor_bps: u16,
}

/// Current liquidity state of a reserve
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace, Default)]
pub struct ReserveLiquidity {
    /// Total tokens deposited (in native token units)
    pub total_deposits: u64,

    /// Total tokens borrowed (in native token units)
    pub total_borrows: u64,

    /// Accumulated protocol fees (in native token units)
    pub accumulated_protocol_fees: u64,

    /// Cumulative borrow index (scaled by 10^18)
    /// Tracks compound interest for borrowers
    /// Starts at 1e18 (1_000_000_000_000_000_000)
    pub cumulative_borrow_index: u128,

    /// Cumulative supply index (scaled by 10^18)
    /// Tracks compound interest for depositors
    /// Starts at 1e18 (1_000_000_000_000_000_000)
    pub cumulative_supply_index: u128,

    /// Current borrow rate in BPS (annualized)
    pub current_borrow_rate_bps: u64,

    /// Current supply rate in BPS (annualized)
    pub current_supply_rate_bps: u64,
}

impl Reserve {
    pub const SEED_PREFIX: &'static [u8] = b"reserve";

    /// Calculate current utilization rate in BPS
    pub fn calculate_utilization_bps(&self) -> u64 {
        if self.liquidity.total_deposits == 0 {
            return 0;
        }

        // utilization = borrows / deposits * 10000
        ((self.liquidity.total_borrows as u128 * 10000) / self.liquidity.total_deposits as u128) as u64
    }

    /// Get available liquidity for borrowing
    pub fn available_liquidity(&self) -> u64 {
        self.liquidity
            .total_deposits
            .saturating_sub(self.liquidity.total_borrows)
    }

    /// Check if reserve needs refresh (stale data)
    pub fn is_stale(&self, current_slot: u64, max_age_slots: u64) -> bool {
        current_slot > self.last_update_slot + max_age_slots
    }

    /// Validate LTV is less than liquidation threshold
    pub fn validate_config(config: &ReserveConfig) -> bool {
        config.ltv_bps < config.liquidation_threshold_bps
            && config.liquidation_threshold_bps <= 10000
            && config.interest_rate_config.optimal_utilization_bps <= 10000
            && config.interest_rate_config.reserve_factor_bps <= 10000
    }
}

impl InterestRateConfig {
    /// Calculate borrow rate based on utilization
    /// Returns rate in BPS (annualized)
    pub fn calculate_borrow_rate(&self, utilization_bps: u64) -> u64 {
        if utilization_bps <= self.optimal_utilization_bps as u64 {
            // Below optimal: base + (util / optimal) * slope1
            let slope_rate = if self.optimal_utilization_bps == 0 {
                0
            } else {
                (utilization_bps * self.slope1_bps as u64) / self.optimal_utilization_bps as u64
            };
            self.base_rate_bps as u64 + slope_rate
        } else {
            // Above optimal: base + slope1 + ((util - optimal) / (1 - optimal)) * slope2
            let excess_utilization = utilization_bps - self.optimal_utilization_bps as u64;
            let remaining_utilization = 10000 - self.optimal_utilization_bps as u64;

            let steep_rate = if remaining_utilization == 0 {
                self.slope2_bps as u64
            } else {
                (excess_utilization * self.slope2_bps as u64) / remaining_utilization
            };

            self.base_rate_bps as u64 + self.slope1_bps as u64 + steep_rate
        }
    }

    /// Calculate supply rate based on borrow rate and utilization
    /// supply_rate = borrow_rate * utilization * (1 - reserve_factor)
    pub fn calculate_supply_rate(&self, borrow_rate_bps: u64, utilization_bps: u64) -> u64 {
        // supply_rate = borrow_rate * utilization * (1 - reserve_factor) / 10000
        let gross_supply_rate = (borrow_rate_bps * utilization_bps) / 10000;
        let protocol_cut = (gross_supply_rate * self.reserve_factor_bps as u64) / 10000;
        gross_supply_rate - protocol_cut
    }
}
