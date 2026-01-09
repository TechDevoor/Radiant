/// Radiant Protocol Constants

// ============================================================================
// SCALING CONSTANTS
// ============================================================================

/// Basis points denominator (100% = 10000 BPS)
pub const BPS_DENOMINATOR: u64 = 10_000;

/// Index scale factor (1e18) for compound interest tracking
pub const INDEX_ONE: u128 = 1_000_000_000_000_000_000; // 10^18

/// USD value scale factor (1e6) for price calculations
pub const USD_DECIMALS: u8 = 6;
pub const USD_SCALE: u128 = 1_000_000; // 10^6

/// Seconds per year (for interest rate calculations)
pub const SECONDS_PER_YEAR: u64 = 31_536_000; // 365 * 24 * 60 * 60

/// Slots per year (approximate, ~400ms per slot)
pub const SLOTS_PER_YEAR: u64 = 78_840_000; // 31_536_000 / 0.4

// ============================================================================
// PDA SEEDS
// ============================================================================

/// Seed prefix for LendingMarket PDA
pub const LENDING_MARKET_SEED: &[u8] = b"lending_market";

/// Seed prefix for Reserve PDA
pub const RESERVE_SEED: &[u8] = b"reserve";

/// Seed prefix for Obligation PDA
pub const OBLIGATION_SEED: &[u8] = b"obligation";

/// Seed prefix for Reserve token vault PDA
pub const VAULT_SEED: &[u8] = b"vault";

/// Seed prefix for Reserve fee receiver PDA
pub const FEE_RECEIVER_SEED: &[u8] = b"fee_receiver";

// ============================================================================
// DEFAULT VALUES
// ============================================================================

/// Default close factor (50% = 5000 BPS)
/// Maximum percentage of debt that can be liquidated at once
pub const DEFAULT_CLOSE_FACTOR_BPS: u16 = 5_000;

/// Default liquidation bonus (5% = 500 BPS)
/// Bonus collateral liquidator receives
pub const DEFAULT_LIQUIDATION_BONUS_BPS: u16 = 500;

/// Default protocol fee (10% = 1000 BPS)
/// Protocol's cut of liquidation bonus
pub const DEFAULT_PROTOCOL_FEE_BPS: u16 = 1_000;

/// Default optimal utilization (80% = 8000 BPS)
pub const DEFAULT_OPTIMAL_UTILIZATION_BPS: u16 = 8_000;

/// Default base borrow rate (2% = 200 BPS)
pub const DEFAULT_BASE_RATE_BPS: u16 = 200;

/// Default slope1 (10% = 1000 BPS)
pub const DEFAULT_SLOPE1_BPS: u16 = 1_000;

/// Default slope2 (100% = 10000 BPS)
pub const DEFAULT_SLOPE2_BPS: u16 = 10_000;

/// Default reserve factor (10% = 1000 BPS)
pub const DEFAULT_RESERVE_FACTOR_BPS: u16 = 1_000;

// ============================================================================
// LIMITS
// ============================================================================

/// Maximum number of reserves per lending market
pub const MAX_RESERVES: u8 = 32;

/// Maximum number of deposits per obligation
pub const MAX_OBLIGATION_DEPOSITS: usize = 8;

/// Maximum number of borrows per obligation
pub const MAX_OBLIGATION_BORROWS: usize = 8;

/// Maximum LTV allowed (95% = 9500 BPS)
pub const MAX_LTV_BPS: u16 = 9_500;

/// Maximum liquidation threshold (98% = 9800 BPS)
pub const MAX_LIQUIDATION_THRESHOLD_BPS: u16 = 9_800;

/// Maximum liquidation bonus (25% = 2500 BPS)
pub const MAX_LIQUIDATION_BONUS_BPS: u16 = 2_500;

/// Maximum reserve factor (50% = 5000 BPS)
pub const MAX_RESERVE_FACTOR_BPS: u16 = 5_000;

/// Maximum staleness for oracle price (slots)
/// ~60 seconds at 400ms per slot
pub const MAX_ORACLE_STALENESS_SLOTS: u64 = 150;

/// Maximum staleness for reserve refresh (slots)
/// ~10 minutes
pub const MAX_RESERVE_STALENESS_SLOTS: u64 = 1_500;

// ============================================================================
// HEALTH FACTOR
// ============================================================================

/// Health factor scale (1.0 = 10000)
pub const HEALTH_FACTOR_ONE: u64 = 10_000;

/// Minimum health factor after borrow (1.0 = 10000)
/// User cannot borrow if it would drop health below this
pub const MIN_HEALTH_FACTOR_AFTER_BORROW: u64 = 10_000;

// ============================================================================
// MINIMUM AMOUNTS
// ============================================================================

/// Minimum deposit amount (to prevent dust attacks)
pub const MIN_DEPOSIT_AMOUNT: u64 = 1_000;

/// Minimum borrow amount (to prevent dust attacks)
pub const MIN_BORROW_AMOUNT: u64 = 1_000;

/// Minimum collateral value in USD to open a borrow position ($10)
pub const MIN_COLLATERAL_VALUE_USD: u128 = 10 * USD_SCALE;
