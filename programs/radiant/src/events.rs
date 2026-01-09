use anchor_lang::prelude::*;

// ============================================================================
// LENDING MARKET EVENTS
// ============================================================================

/// Emitted when a new lending market is initialized
#[event]
pub struct LendingMarketInitialized {
    pub lending_market: Pubkey,
    pub authority: Pubkey,
    pub treasury: Pubkey,
    pub close_factor_bps: u16,
    pub liquidation_bonus_bps: u16,
}

/// Emitted when emergency mode is toggled
#[event]
pub struct EmergencyModeChanged {
    pub lending_market: Pubkey,
    pub emergency_mode: bool,
    pub timestamp: i64,
}

// ============================================================================
// RESERVE EVENTS
// ============================================================================

/// Emitted when a new reserve is initialized
#[event]
pub struct ReserveInitialized {
    pub lending_market: Pubkey,
    pub reserve: Pubkey,
    pub token_mint: Pubkey,
    pub ltv_bps: u16,
    pub liquidation_threshold_bps: u16,
}

/// Emitted when reserve config is updated
#[event]
pub struct ReserveConfigUpdated {
    pub reserve: Pubkey,
    pub ltv_bps: u16,
    pub liquidation_threshold_bps: u16,
    pub deposit_limit: u64,
    pub borrow_limit: u64,
}

/// Emitted when a reserve is refreshed (interest accrued)
#[event]
pub struct ReserveRefreshed {
    pub reserve: Pubkey,
    pub cumulative_borrow_index: u128,
    pub cumulative_supply_index: u128,
    pub current_borrow_rate_bps: u64,
    pub current_supply_rate_bps: u64,
    pub total_deposits: u64,
    pub total_borrows: u64,
    pub timestamp: i64,
}

// ============================================================================
// OBLIGATION EVENTS
// ============================================================================

/// Emitted when a new obligation is initialized
#[event]
pub struct ObligationInitialized {
    pub lending_market: Pubkey,
    pub obligation: Pubkey,
    pub owner: Pubkey,
}

/// Emitted when an obligation is refreshed
#[event]
pub struct ObligationRefreshed {
    pub obligation: Pubkey,
    pub deposited_value_usd: u128,
    pub borrowed_value_usd: u128,
    pub allowed_borrow_value_usd: u128,
    pub unhealthy_borrow_value_usd: u128,
    pub health_factor: Option<u64>,
    pub timestamp: i64,
}

// ============================================================================
// USER ACTION EVENTS
// ============================================================================

/// Emitted when a user deposits collateral
#[event]
pub struct DepositEvent {
    pub lending_market: Pubkey,
    pub reserve: Pubkey,
    pub obligation: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub new_deposit_amount: u64,
    pub timestamp: i64,
}

/// Emitted when a user withdraws collateral
#[event]
pub struct WithdrawEvent {
    pub lending_market: Pubkey,
    pub reserve: Pubkey,
    pub obligation: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub remaining_deposit: u64,
    pub timestamp: i64,
}

/// Emitted when a user borrows tokens
#[event]
pub struct BorrowEvent {
    pub lending_market: Pubkey,
    pub reserve: Pubkey,
    pub obligation: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub new_borrow_amount: u64,
    pub new_utilization_bps: u64,
    pub new_borrow_rate_bps: u64,
    pub timestamp: i64,
}

/// Emitted when a user repays borrowed tokens
#[event]
pub struct RepayEvent {
    pub lending_market: Pubkey,
    pub reserve: Pubkey,
    pub obligation: Pubkey,
    pub payer: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub remaining_borrow: u64,
    pub new_utilization_bps: u64,
    pub new_borrow_rate_bps: u64,
    pub timestamp: i64,
}

// ============================================================================
// LIQUIDATION EVENTS
// ============================================================================

/// Emitted when a position is liquidated
#[event]
pub struct LiquidationEvent {
    pub lending_market: Pubkey,
    pub obligation: Pubkey,
    pub liquidator: Pubkey,
    pub owner: Pubkey,
    pub repay_reserve: Pubkey,
    pub collateral_reserve: Pubkey,
    pub repay_amount: u64,
    pub collateral_seized: u64,
    pub liquidation_bonus: u64,
    pub protocol_fee: u64,
    pub timestamp: i64,
}

// ============================================================================
// PROTOCOL FEE EVENTS
// ============================================================================

/// Emitted when protocol fees are collected
#[event]
pub struct ProtocolFeesCollected {
    pub reserve: Pubkey,
    pub amount: u64,
    pub recipient: Pubkey,
    pub timestamp: i64,
}
