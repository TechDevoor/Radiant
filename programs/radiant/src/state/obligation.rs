use anchor_lang::prelude::*;

/// Maximum number of deposits per obligation
pub const MAX_DEPOSITS: usize = 8;

/// Maximum number of borrows per obligation
pub const MAX_BORROWS: usize = 8;

/// User's position in the lending market
/// PDA Seeds: ["obligation", lending_market, owner]
#[account]
#[derive(InitSpace)]
pub struct Obligation {
    /// Version for future upgrades
    pub version: u8,

    /// Bump seed for PDA derivation
    pub bump: u8,

    /// The lending market this obligation belongs to
    pub lending_market: Pubkey,

    /// Owner of this obligation
    pub owner: Pubkey,

    /// Last slot when obligation was refreshed
    pub last_update_slot: u64,

    /// Deposited assets used as collateral
    #[max_len(MAX_DEPOSITS)]
    pub deposits: Vec<ObligationCollateral>,

    /// Borrowed assets
    #[max_len(MAX_BORROWS)]
    pub borrows: Vec<ObligationLiquidity>,

    /// Cached total deposited value in USD (scaled by 10^6)
    /// Updated on refresh_obligation
    pub deposited_value_usd: u128,

    /// Cached total borrowed value in USD (scaled by 10^6)
    /// Updated on refresh_obligation
    pub borrowed_value_usd: u128,

    /// Cached allowed borrow value in USD (scaled by 10^6)
    /// = sum(deposit_value * LTV) for each deposit
    pub allowed_borrow_value_usd: u128,

    /// Cached liquidation threshold value in USD (scaled by 10^6)
    /// = sum(deposit_value * liquidation_threshold) for each deposit
    pub unhealthy_borrow_value_usd: u128,

    /// Reserved space for future upgrades (64 bytes)
    pub _padding: [u8; 64],
}

/// Collateral deposited by user
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace, Default)]
pub struct ObligationCollateral {
    /// Reserve account this deposit is for
    pub reserve: Pubkey,

    /// Amount deposited (in native token units)
    pub deposited_amount: u64,

    /// Supply index snapshot when deposit was made
    /// Used to calculate accrued interest
    pub supply_index_snapshot: u128,

    /// Cached market value in USD (scaled by 10^6)
    pub market_value_usd: u128,
}

/// Liquidity borrowed by user
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace, Default)]
pub struct ObligationLiquidity {
    /// Reserve account this borrow is from
    pub reserve: Pubkey,

    /// Amount borrowed (principal, in native token units)
    pub borrowed_amount: u64,

    /// Borrow index snapshot when loan was taken
    /// Used to calculate accrued interest
    pub borrow_index_snapshot: u128,

    /// Cached market value in USD (scaled by 10^6)
    pub market_value_usd: u128,
}

impl Obligation {
    pub const SEED_PREFIX: &'static [u8] = b"obligation";

    /// Calculate health factor (scaled by 10000 for precision)
    ///
    /// Formula: Health = unhealthy_borrow_value_usd / borrowed_value_usd
    ///
    /// Where unhealthy_borrow_value_usd = sum(deposit_value * liquidation_threshold)
    /// This is pre-calculated during refresh_obligation
    ///
    /// Returns:
    /// - None = No debt (infinite health)
    /// - Some(>10000) = Healthy (e.g., 12000 = 1.2 health factor)
    /// - Some(<=10000) = Liquidatable (e.g., 9500 = 0.95 health factor)
    pub fn calculate_health_factor(&self) -> Option<u64> {
        if self.borrowed_value_usd == 0 {
            return None; // No debt = infinite health
        }

        // health_factor = (unhealthy_borrow_value / borrowed_value) * 10000
        // Example: $85,000 threshold / $80,000 debt = 1.0625 → 10625
        Some(
            ((self.unhealthy_borrow_value_usd * 10000) / self.borrowed_value_usd) as u64
        )
    }

    /// Check if obligation is healthy (health factor > 1.0)
    pub fn is_healthy(&self) -> bool {
        match self.calculate_health_factor() {
            None => true, // No debt = healthy
            Some(health) => health > 10000,
        }
    }

    /// Check if obligation is liquidatable (health factor <= 1.0)
    pub fn is_liquidatable(&self) -> bool {
        !self.is_healthy()
    }

    /// Get remaining borrow capacity in USD
    pub fn remaining_borrow_capacity_usd(&self) -> u128 {
        self.allowed_borrow_value_usd
            .saturating_sub(self.borrowed_value_usd)
    }

    /// Find deposit index for a given reserve
    pub fn find_deposit(&self, reserve: &Pubkey) -> Option<usize> {
        self.deposits.iter().position(|d| &d.reserve == reserve)
    }

    /// Find borrow index for a given reserve
    pub fn find_borrow(&self, reserve: &Pubkey) -> Option<usize> {
        self.borrows.iter().position(|b| &b.reserve == reserve)
    }

    /// Check if user has any deposits
    pub fn has_deposits(&self) -> bool {
        !self.deposits.is_empty()
    }

    /// Check if user has any borrows
    pub fn has_borrows(&self) -> bool {
        !self.borrows.is_empty()
    }

    /// Get current borrow amount including accrued interest
    pub fn get_borrow_amount_with_interest(
        &self,
        borrow_index: usize,
        current_borrow_index: u128,
    ) -> Option<u64> {
        let borrow = self.borrows.get(borrow_index)?;

        if borrow.borrow_index_snapshot == 0 {
            return Some(0);
        }

        // current_amount = principal * (current_index / snapshot_index)
        let amount = (borrow.borrowed_amount as u128 * current_borrow_index)
            / borrow.borrow_index_snapshot;

        Some(amount as u64)
    }

    /// Get current deposit amount including accrued interest
    pub fn get_deposit_amount_with_interest(
        &self,
        deposit_index: usize,
        current_supply_index: u128,
    ) -> Option<u64> {
        let deposit = self.deposits.get(deposit_index)?;

        if deposit.supply_index_snapshot == 0 {
            return Some(0);
        }

        // current_amount = principal * (current_index / snapshot_index)
        let amount = (deposit.deposited_amount as u128 * current_supply_index)
            / deposit.supply_index_snapshot;

        Some(amount as u64)
    }
}

impl ObligationCollateral {
    /// Create new collateral entry
    pub fn new(reserve: Pubkey, amount: u64, supply_index: u128) -> Self {
        Self {
            reserve,
            deposited_amount: amount,
            supply_index_snapshot: supply_index,
            market_value_usd: 0,
        }
    }
}

impl ObligationLiquidity {
    /// Create new borrow entry
    pub fn new(reserve: Pubkey, amount: u64, borrow_index: u128) -> Self {
        Self {
            reserve,
            borrowed_amount: amount,
            borrow_index_snapshot: borrow_index,
            market_value_usd: 0,
        }
    }
}
