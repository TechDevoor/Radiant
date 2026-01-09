use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Obligation};
use crate::constants::USD_SCALE;
use crate::events::ObligationRefreshed;

/// Accounts for refreshing an obligation
/// Note: In production, you'd pass all deposit/borrow reserves as remaining_accounts
#[derive(Accounts)]
pub struct RefreshObligation<'info> {
    /// The lending market
    #[account(
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The obligation to refresh
    #[account(
        mut,
        constraint = obligation.lending_market == lending_market.key() @ RefreshObligationError::InvalidObligation
    )]
    pub obligation: Account<'info, Obligation>,
    // In production, remaining_accounts would contain:
    // - All deposit reserves (to get supply indexes and prices)
    // - All borrow reserves (to get borrow indexes and prices)
}

/// Refresh obligation state
///
/// This permissionless instruction:
/// 1. Updates deposit values with accrued interest
/// 2. Updates borrow values with accrued interest
/// 3. Recalculates USD values using oracle prices
/// 4. Updates health factor cached values
///
/// Anyone can call this to keep the obligation state fresh.
/// Must be called before borrow, withdraw, or liquidate.
///
/// Note: This is a simplified version. In production, you would:
/// - Pass all deposit/borrow reserves as remaining_accounts
/// - Read oracle prices for each asset
/// - Calculate proper USD values
pub fn handler(ctx: Context<RefreshObligation>) -> Result<()> {
    let obligation = &mut ctx.accounts.obligation;
    let clock = Clock::get()?;

    // Reset cached values
    let mut deposited_value_usd: u128 = 0;
    let mut borrowed_value_usd: u128 = 0;
    let mut allowed_borrow_value_usd: u128 = 0;
    let mut unhealthy_borrow_value_usd: u128 = 0;

    // In production, you would iterate through remaining_accounts
    // to get each reserve's current index and oracle price
    //
    // For now, we use a simplified approach where the reserves
    // must be refreshed separately and we just update timestamps

    // Update each deposit's cached USD value
    // Note: In production, this would use oracle prices
    for deposit in obligation.deposits.iter_mut() {
        // Placeholder: In production, read from reserve account in remaining_accounts
        // let reserve = get_reserve_from_remaining_accounts(deposit.reserve)?;
        // let current_supply_index = reserve.liquidity.cumulative_supply_index;
        // let price_usd = get_oracle_price(reserve.oracle)?;

        // Calculate current deposit value with interest
        // current_amount = principal * (current_index / snapshot_index)
        // For now, we just use the stored amount (without index update)
        let deposit_amount = deposit.deposited_amount;

        // Placeholder USD value (in production: amount * price / 10^decimals)
        // Using 1:1 ratio for simplicity - replace with oracle price
        let deposit_usd = (deposit_amount as u128) * USD_SCALE / 1_000_000; // Assuming 6 decimals

        deposit.market_value_usd = deposit_usd;
        deposited_value_usd += deposit_usd;

        // Calculate borrowing capacity (LTV)
        // Placeholder: 80% LTV - in production, read from reserve config
        let ltv_bps: u128 = 8000;
        allowed_borrow_value_usd += deposit_usd * ltv_bps / 10000;

        // Calculate liquidation threshold value
        // Placeholder: 85% threshold - in production, read from reserve config
        let liq_threshold_bps: u128 = 8500;
        unhealthy_borrow_value_usd += deposit_usd * liq_threshold_bps / 10000;
    }

    // Update each borrow's cached USD value
    for borrow in obligation.borrows.iter_mut() {
        // Placeholder: In production, read from reserve account in remaining_accounts
        // let reserve = get_reserve_from_remaining_accounts(borrow.reserve)?;
        // let current_borrow_index = reserve.liquidity.cumulative_borrow_index;
        // let price_usd = get_oracle_price(reserve.oracle)?;

        // Calculate current borrow value with interest
        // current_amount = principal * (current_index / snapshot_index)
        let borrow_amount = borrow.borrowed_amount;

        // Placeholder USD value (in production: amount * price / 10^decimals)
        let borrow_usd = (borrow_amount as u128) * USD_SCALE / 1_000_000;

        borrow.market_value_usd = borrow_usd;
        borrowed_value_usd += borrow_usd;
    }

    // Update cached values
    obligation.deposited_value_usd = deposited_value_usd;
    obligation.borrowed_value_usd = borrowed_value_usd;
    obligation.allowed_borrow_value_usd = allowed_borrow_value_usd;
    obligation.unhealthy_borrow_value_usd = unhealthy_borrow_value_usd;

    // Update timestamp
    obligation.last_update_slot = clock.slot;

    // Calculate health factor
    let health_factor = obligation.calculate_health_factor();

    // Emit event
    emit!(ObligationRefreshed {
        obligation: obligation.key(),
        deposited_value_usd,
        borrowed_value_usd,
        allowed_borrow_value_usd,
        unhealthy_borrow_value_usd,
        health_factor,
        timestamp: clock.unix_timestamp,
    });

    msg!("Obligation refreshed for: {}", obligation.owner);
    msg!("Deposited: {} USD, Borrowed: {} USD",
        deposited_value_usd / USD_SCALE,
        borrowed_value_usd / USD_SCALE
    );
    msg!("Health factor: {:?}", health_factor);

    Ok(())
}

/// Refresh obligation errors
#[error_code]
pub enum RefreshObligationError {
    #[msg("Obligation does not belong to this lending market")]
    InvalidObligation,

    #[msg("Reserve not found in remaining accounts")]
    ReserveNotFound,

    #[msg("Invalid oracle price")]
    InvalidOraclePrice,

    #[msg("Math overflow")]
    MathOverflow,
}
