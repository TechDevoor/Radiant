use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Reserve};
use crate::constants::{INDEX_ONE, SECONDS_PER_YEAR};
use crate::events::ReserveRefreshed;

/// Accounts for refreshing a reserve
#[derive(Accounts)]
pub struct RefreshReserve<'info> {
    /// The lending market
    #[account(
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,

    /// The reserve to refresh
    #[account(
        mut,
        constraint = reserve.lending_market == lending_market.key() @ RefreshReserveError::InvalidReserve
    )]
    pub reserve: Account<'info, Reserve>,

    /// Pyth oracle price feed
    /// CHECK: Validated against reserve.oracle
    #[account(
        constraint = oracle.key() == reserve.oracle @ RefreshReserveError::InvalidOracle
    )]
    pub oracle: UncheckedAccount<'info>,
}

/// Refresh reserve state
///
/// This permissionless instruction:
/// 1. Accrues interest based on time elapsed
/// 2. Updates cumulative indexes
/// 3. Recalculates interest rates based on utilization
///
/// Anyone can call this to keep the reserve state fresh.
/// Must be called before any operation that depends on current state.
pub fn handler(ctx: Context<RefreshReserve>) -> Result<()> {
    let reserve = &mut ctx.accounts.reserve;
    let clock = Clock::get()?;

    let current_slot = clock.slot;
    let current_timestamp = clock.unix_timestamp;

    // Calculate time elapsed since last update
    let slots_elapsed = current_slot.saturating_sub(reserve.last_update_slot);
    let time_elapsed = current_timestamp.saturating_sub(reserve.last_update_timestamp);

    // Skip if already updated this slot
    if slots_elapsed == 0 {
        return Ok(());
    }

    // Only accrue interest if there are borrows
    if reserve.liquidity.total_borrows > 0 && time_elapsed > 0 {
        // Cap time elapsed to prevent extreme interest accrual (max 1 year)
        let time_elapsed_capped = time_elapsed.min(SECONDS_PER_YEAR as i64);

        // Calculate interest accrued
        // interest_factor = 1 + (borrow_rate * time_elapsed / seconds_per_year)

        let borrow_rate_bps = reserve.liquidity.current_borrow_rate_bps;

        // Calculate compound factor for borrow index
        // compound_factor = (rate_bps * time_elapsed) / (10000 * seconds_per_year)
        // We scale by INDEX_ONE for precision
        let borrow_compound_factor = calculate_compound_factor(
            borrow_rate_bps,
            time_elapsed_capped as u64,
        )?;

        // Update borrow index: new_index = old_index * (1 + compound_factor)
        let new_borrow_index = reserve.liquidity.cumulative_borrow_index
            .checked_mul(INDEX_ONE + borrow_compound_factor)
            .ok_or(RefreshReserveError::MathOverflow)?
            / INDEX_ONE;

        // Sanity check: new index should not be less than old index (compound factor >= 0)
        require!(
            new_borrow_index >= reserve.liquidity.cumulative_borrow_index,
            RefreshReserveError::InvalidIndexCalculation
        );

        // Calculate interest earned
        let interest_earned = calculate_interest_earned(
            reserve.liquidity.total_borrows,
            borrow_compound_factor,
        )?;

        // Update total borrows with accrued interest
        reserve.liquidity.total_borrows = reserve.liquidity.total_borrows
            .checked_add(interest_earned)
            .ok_or(RefreshReserveError::MathOverflow)?;

        // Calculate protocol fees (reserve factor)
        let protocol_fee = (interest_earned as u128
            * reserve.config.interest_rate_config.reserve_factor_bps as u128
            / 10000) as u64;

        reserve.liquidity.accumulated_protocol_fees = reserve.liquidity.accumulated_protocol_fees
            .checked_add(protocol_fee)
            .ok_or(RefreshReserveError::MathOverflow)?;

        // Update supply index (depositors earn interest minus protocol fee)
        let supply_interest = interest_earned.saturating_sub(protocol_fee);
        let supply_compound_factor = if reserve.liquidity.total_deposits > 0 {
            (supply_interest as u128 * INDEX_ONE) / reserve.liquidity.total_deposits as u128
        } else {
            0
        };

        let new_supply_index = reserve.liquidity.cumulative_supply_index
            .checked_add(
                (reserve.liquidity.cumulative_supply_index * supply_compound_factor) / INDEX_ONE
            )
            .ok_or(RefreshReserveError::MathOverflow)?;

        // Sanity check: new supply index should not be less than old index
        require!(
            new_supply_index >= reserve.liquidity.cumulative_supply_index,
            RefreshReserveError::InvalidIndexCalculation
        );

        // Apply new indexes
        reserve.liquidity.cumulative_borrow_index = new_borrow_index;
        reserve.liquidity.cumulative_supply_index = new_supply_index;
    }

    // Recalculate interest rates based on new utilization
    let utilization_bps = reserve.calculate_utilization_bps();
    let borrow_rate = reserve.config.interest_rate_config.calculate_borrow_rate(utilization_bps);
    let supply_rate = reserve.config.interest_rate_config.calculate_supply_rate(borrow_rate, utilization_bps);

    reserve.liquidity.current_borrow_rate_bps = borrow_rate;
    reserve.liquidity.current_supply_rate_bps = supply_rate;

    // Update timestamps
    reserve.last_update_slot = current_slot;
    reserve.last_update_timestamp = current_timestamp;

    // Emit event
    emit!(ReserveRefreshed {
        reserve: reserve.key(),
        cumulative_borrow_index: reserve.liquidity.cumulative_borrow_index,
        cumulative_supply_index: reserve.liquidity.cumulative_supply_index,
        current_borrow_rate_bps: borrow_rate,
        current_supply_rate_bps: supply_rate,
        total_deposits: reserve.liquidity.total_deposits,
        total_borrows: reserve.liquidity.total_borrows,
        timestamp: current_timestamp,
    });

    msg!("Reserve refreshed: {}", reserve.token_mint);
    msg!("Utilization: {} bps, Borrow rate: {} bps, Supply rate: {} bps",
        utilization_bps, borrow_rate, supply_rate);

    Ok(())
}

/// Calculate compound factor for a given rate and time
/// Returns the factor scaled by INDEX_ONE
fn calculate_compound_factor(rate_bps: u64, time_elapsed_seconds: u64) -> Result<u128> {
    // compound_factor = (rate_bps * time_elapsed) / (10000 * seconds_per_year) * INDEX_ONE
    // Simplified: (rate_bps * time_elapsed * INDEX_ONE) / (10000 * SECONDS_PER_YEAR)

    let numerator = (rate_bps as u128)
        .checked_mul(time_elapsed_seconds as u128)
        .ok_or(RefreshReserveError::MathOverflow)?
        .checked_mul(INDEX_ONE)
        .ok_or(RefreshReserveError::MathOverflow)?;

    let denominator = 10000u128 * SECONDS_PER_YEAR as u128;

    Ok(numerator / denominator)
}

/// Calculate interest earned based on principal and compound factor
fn calculate_interest_earned(principal: u64, compound_factor: u128) -> Result<u64> {
    let interest = (principal as u128)
        .checked_mul(compound_factor)
        .ok_or(RefreshReserveError::MathOverflow)?
        / INDEX_ONE;

    Ok(interest as u64)
}

/// Refresh reserve errors
#[error_code]
pub enum RefreshReserveError {
    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Invalid oracle account")]
    InvalidOracle,

    #[msg("Invalid index calculation - would decrease index")]
    InvalidIndexCalculation,

    #[msg("Math overflow")]
    MathOverflow,
}
