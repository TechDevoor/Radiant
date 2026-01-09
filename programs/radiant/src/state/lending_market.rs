use anchor_lang::prelude::*;

/// Global configuration for the lending protocol
/// PDA Seeds: ["lending_market", authority]
#[account]
#[derive(InitSpace)]
pub struct LendingMarket {
    /// Version for future upgrades
    pub version: u8,

    /// Bump seed for PDA derivation
    pub bump: u8,

    /// Authority that can manage the market (add reserves, update configs)
    pub authority: Pubkey,

    /// Treasury that receives protocol fees
    pub treasury: Pubkey,

    /// Emergency mode - when true, only withdrawals and repayments allowed
    pub emergency_mode: bool,

    /// Close factor in BPS (max % of debt that can be liquidated at once)
    /// e.g., 5000 = 50%
    pub close_factor_bps: u16,

    /// Liquidation bonus in BPS (bonus collateral liquidator receives)
    /// e.g., 500 = 5%
    pub liquidation_bonus_bps: u16,

    /// Protocol fee in BPS (cut of liquidation bonus to protocol)
    /// e.g., 1000 = 10%
    pub protocol_fee_bps: u16,

    /// Number of reserves in this market
    pub reserves_count: u8,

    /// Reserved space for future upgrades (128 bytes)
    pub _padding: [u8; 128],
}

impl LendingMarket {
    pub const SEED_PREFIX: &'static [u8] = b"lending_market";

    /// Check if market is in emergency mode
    pub fn is_emergency(&self) -> bool {
        self.emergency_mode
    }

    /// Validate close factor is within acceptable range (0-100%)
    pub fn validate_close_factor(close_factor_bps: u16) -> bool {
        close_factor_bps <= 10000
    }

    /// Validate liquidation bonus is within acceptable range (0-25%)
    pub fn validate_liquidation_bonus(bonus_bps: u16) -> bool {
        bonus_bps <= 2500
    }
}
