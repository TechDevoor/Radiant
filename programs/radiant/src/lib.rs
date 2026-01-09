use anchor_lang::prelude::*;

pub mod constants;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("3UUp4kNzq4ieBgfnfCSfASgLMagz51YK7fUc5eK9s8ir");

#[program]
pub mod radiant {
    use super::*;

    // ============================================================================
    // ADMIN INSTRUCTIONS
    // ============================================================================

    /// Initialize a new lending market
    pub fn initialize_lending_market(
        ctx: Context<InitializeLendingMarket>,
        params: InitializeLendingMarketParams,
    ) -> Result<()> {
        instructions::admin::initialize_lending_market::handler(ctx, params)
    }

    /// Initialize a new reserve (asset pool)
    pub fn initialize_reserve(
        ctx: Context<InitializeReserve>,
        params: InitializeReserveParams,
    ) -> Result<()> {
        instructions::admin::initialize_reserve::handler(ctx, params)
    }

    /// Update reserve configuration
    pub fn update_reserve_config(
        ctx: Context<UpdateReserveConfig>,
        params: UpdateReserveConfigParams,
    ) -> Result<()> {
        instructions::admin::update_reserve_config::handler(ctx, params)
    }

    /// Set emergency mode on/off
    pub fn set_emergency_mode(
        ctx: Context<SetEmergencyMode>,
        emergency: bool,
    ) -> Result<()> {
        instructions::admin::set_emergency_mode::handler(ctx, emergency)
    }

    /// Collect accumulated protocol fees from a reserve
    pub fn collect_fees(ctx: Context<CollectFees>, amount: u64) -> Result<()> {
        instructions::admin::collect_fees::handler(ctx, amount)
    }

    // ============================================================================
    // USER INSTRUCTIONS
    // ============================================================================

    /// Initialize a user's obligation account
    pub fn initialize_obligation(ctx: Context<InitializeObligation>) -> Result<()> {
        instructions::user::initialize_obligation::handler(ctx)
    }

    /// Deposit collateral into a reserve
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::user::deposit::handler(ctx, amount)
    }

    /// Withdraw collateral from a reserve
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        instructions::user::withdraw::handler(ctx, amount)
    }

    /// Borrow tokens from a reserve
    pub fn borrow(ctx: Context<Borrow>, amount: u64) -> Result<()> {
        instructions::user::borrow::handler(ctx, amount)
    }

    /// Repay borrowed tokens
    pub fn repay(ctx: Context<Repay>, amount: u64) -> Result<()> {
        instructions::user::repay::handler(ctx, amount)
    }

    // ============================================================================
    // PERMISSIONLESS INSTRUCTIONS
    // ============================================================================

    /// Refresh reserve state (accrue interest)
    pub fn refresh_reserve(ctx: Context<RefreshReserve>) -> Result<()> {
        instructions::permissionless::refresh_reserve::handler(ctx)
    }

    /// Refresh obligation state (update USD values)
    pub fn refresh_obligation(ctx: Context<RefreshObligation>) -> Result<()> {
        instructions::permissionless::refresh_obligation::handler(ctx)
    }

    /// Liquidate an unhealthy position
    pub fn liquidate(ctx: Context<Liquidate>, repay_amount: u64) -> Result<()> {
        instructions::permissionless::liquidate::handler(ctx, repay_amount)
    }
}
