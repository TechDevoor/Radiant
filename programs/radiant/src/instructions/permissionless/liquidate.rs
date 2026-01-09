use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::{LendingMarket, Reserve, Obligation};
use crate::constants::VAULT_SEED;
use crate::events::LiquidationEvent;

/// Accounts for liquidating an unhealthy position
#[derive(Accounts)]
pub struct Liquidate<'info> {
    /// Liquidator performing the liquidation
    pub liquidator: Signer<'info>,

    /// The lending market
    #[account(
        seeds = [LendingMarket::SEED_PREFIX, lending_market.authority.as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Box<Account<'info, LendingMarket>>,

    /// The reserve of the debt being repaid
    #[account(
        mut,
        constraint = repay_reserve.lending_market == lending_market.key() @ LiquidateError::InvalidReserve
    )]
    pub repay_reserve: Box<Account<'info, Reserve>>,

    /// The reserve of the collateral being seized
    #[account(
        mut,
        constraint = collateral_reserve.lending_market == lending_market.key() @ LiquidateError::InvalidReserve
    )]
    pub collateral_reserve: Box<Account<'info, Reserve>>,

    /// The unhealthy obligation to liquidate
    #[account(
        mut,
        constraint = obligation.lending_market == lending_market.key() @ LiquidateError::InvalidObligation
    )]
    pub obligation: Box<Account<'info, Obligation>>,

    /// Repay reserve vault (receives repayment)
    #[account(
        mut,
        seeds = [VAULT_SEED, repay_reserve.key().as_ref()],
        bump,
        constraint = repay_vault.key() == repay_reserve.token_vault @ LiquidateError::InvalidVault
    )]
    pub repay_vault: Box<Account<'info, TokenAccount>>,

    /// Collateral reserve vault (source of seized collateral)
    #[account(
        mut,
        seeds = [VAULT_SEED, collateral_reserve.key().as_ref()],
        bump,
        constraint = collateral_vault.key() == collateral_reserve.token_vault @ LiquidateError::InvalidVault
    )]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,

    /// Fee receiver for protocol fees from liquidation
    #[account(
        mut,
        constraint = collateral_fee_receiver.key() == collateral_reserve.fee_receiver @ LiquidateError::InvalidFeeReceiver,
        constraint = collateral_fee_receiver.mint == collateral_reserve.token_mint @ LiquidateError::InvalidTokenMint
    )]
    pub collateral_fee_receiver: Box<Account<'info, TokenAccount>>,

    /// Liquidator's token account for repaying debt
    #[account(
        mut,
        constraint = liquidator_repay_account.mint == repay_reserve.token_mint @ LiquidateError::InvalidTokenMint,
        constraint = liquidator_repay_account.owner == liquidator.key() @ LiquidateError::InvalidTokenOwner
    )]
    pub liquidator_repay_account: Box<Account<'info, TokenAccount>>,

    /// Liquidator's token account for receiving collateral
    #[account(
        mut,
        constraint = liquidator_collateral_account.mint == collateral_reserve.token_mint @ LiquidateError::InvalidTokenMint,
        constraint = liquidator_collateral_account.owner == liquidator.key() @ LiquidateError::InvalidTokenOwner
    )]
    pub liquidator_collateral_account: Box<Account<'info, TokenAccount>>,

    /// Token program
    pub token_program: Program<'info, Token>,
}

/// Liquidate an unhealthy position
///
/// When a borrower's health factor falls below 1.0, their position can be liquidated.
/// The liquidator:
/// 1. Repays part of the borrower's debt
/// 2. Receives collateral worth more than the repayment (liquidation bonus)
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `repay_amount` - Amount of debt to repay (in debt token units)
pub fn handler(ctx: Context<Liquidate>, repay_amount: u64) -> Result<()> {
    let lending_market = &ctx.accounts.lending_market;
    let repay_reserve = &mut ctx.accounts.repay_reserve;
    let collateral_reserve = &mut ctx.accounts.collateral_reserve;
    let obligation = &mut ctx.accounts.obligation;

    // Verify obligation is liquidatable (health factor <= 1.0)
    require!(
        obligation.is_liquidatable(),
        LiquidateError::ObligationHealthy
    );

    let repay_reserve_key = repay_reserve.key();
    let collateral_reserve_key = collateral_reserve.key();

    // Find the borrow position for the repay reserve
    let borrow_index = obligation
        .find_borrow(&repay_reserve_key)
        .ok_or(LiquidateError::NoBorrowFound)?;

    // Find the deposit position for the collateral reserve
    let deposit_index = obligation
        .find_deposit(&collateral_reserve_key)
        .ok_or(LiquidateError::NoCollateralFound)?;

    // Calculate current borrow amount with interest
    let borrow = &obligation.borrows[borrow_index];
    let current_borrow_index = repay_reserve.liquidity.cumulative_borrow_index;
    let current_borrow_amount = if borrow.borrow_index_snapshot > 0 {
        (borrow.borrowed_amount as u128 * current_borrow_index / borrow.borrow_index_snapshot) as u64
    } else {
        borrow.borrowed_amount
    };

    // Calculate maximum repayable (close factor)
    // close_factor = 50% means can only repay half the debt at once
    let max_repay = (current_borrow_amount as u128 * lending_market.close_factor_bps as u128 / 10000) as u64;

    // Determine actual repay amount
    let actual_repay = repay_amount.min(max_repay).min(current_borrow_amount);
    require!(actual_repay > 0, LiquidateError::RepayAmountTooSmall);

    // Calculate collateral to seize
    // In production, this should use oracle prices for proper conversion
    // collateral_value = repay_value * (1 + liquidation_bonus)
    //
    // Simplified calculation (assumes 1:1 price ratio):
    // In production: collateral_amount = (repay_amount * repay_price / collateral_price) * (1 + bonus)
    let bonus_bps = lending_market.liquidation_bonus_bps as u128;
    let collateral_to_seize = (actual_repay as u128 * (10000 + bonus_bps) / 10000) as u64;

    // Verify enough collateral to seize
    let deposit = &obligation.deposits[deposit_index];
    let current_supply_index = collateral_reserve.liquidity.cumulative_supply_index;
    let current_deposit_amount = if deposit.supply_index_snapshot > 0 {
        (deposit.deposited_amount as u128 * current_supply_index / deposit.supply_index_snapshot) as u64
    } else {
        deposit.deposited_amount
    };

    require!(
        collateral_to_seize <= current_deposit_amount,
        LiquidateError::InsufficientCollateral
    );

    // 1. Transfer repayment from liquidator to repay vault
    let transfer_repay_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.liquidator_repay_account.to_account_info(),
            to: ctx.accounts.repay_vault.to_account_info(),
            authority: ctx.accounts.liquidator.to_account_info(),
        },
    );
    token::transfer(transfer_repay_ctx, actual_repay)?;

    // 2. Calculate protocol fee and liquidator reward
    let liquidation_bonus_amount = collateral_to_seize.saturating_sub(actual_repay);
    let protocol_fee = (liquidation_bonus_amount as u128 * lending_market.protocol_fee_bps as u128 / 10000) as u64;
    let liquidator_reward = collateral_to_seize.saturating_sub(protocol_fee);

    // 3. Transfer collateral to liquidator (minus protocol fee) using PDA signer
    let seeds = &[
        Reserve::SEED_PREFIX,
        collateral_reserve.lending_market.as_ref(),
        collateral_reserve.token_mint.as_ref(),
        &[collateral_reserve.bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let transfer_collateral_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.collateral_vault.to_account_info(),
            to: ctx.accounts.liquidator_collateral_account.to_account_info(),
            authority: collateral_reserve.to_account_info(),
        },
        signer_seeds,
    );
    token::transfer(transfer_collateral_ctx, liquidator_reward)?;

    // 4. Transfer protocol fee to fee receiver
    if protocol_fee > 0 {
        let transfer_fee_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.collateral_vault.to_account_info(),
                to: ctx.accounts.collateral_fee_receiver.to_account_info(),
                authority: collateral_reserve.to_account_info(),
            },
            signer_seeds,
        );
        token::transfer(transfer_fee_ctx, protocol_fee)?;
    }

    // Update repay reserve
    repay_reserve.liquidity.total_borrows = repay_reserve.liquidity.total_borrows
        .saturating_sub(actual_repay);

    // Update collateral reserve
    collateral_reserve.liquidity.total_deposits = collateral_reserve.liquidity.total_deposits
        .saturating_sub(collateral_to_seize);

    // Update obligation borrow
    let remaining_borrow = current_borrow_amount.saturating_sub(actual_repay);
    if remaining_borrow == 0 {
        obligation.borrows.remove(borrow_index);
    } else {
        let borrow = &mut obligation.borrows[borrow_index];
        borrow.borrowed_amount = remaining_borrow;
        borrow.borrow_index_snapshot = current_borrow_index;
    }

    // Update obligation deposit (need to recalculate index after borrow removal might have shifted)
    let deposit_index = obligation
        .find_deposit(&collateral_reserve_key)
        .ok_or(LiquidateError::NoCollateralFound)?;

    let remaining_deposit = current_deposit_amount.saturating_sub(collateral_to_seize);
    if remaining_deposit == 0 {
        obligation.deposits.remove(deposit_index);
    } else {
        let deposit = &mut obligation.deposits[deposit_index];
        deposit.deposited_amount = remaining_deposit;
        deposit.supply_index_snapshot = current_supply_index;
    }

    // Update timestamps
    let clock = Clock::get()?;
    repay_reserve.last_update_slot = clock.slot;
    repay_reserve.last_update_timestamp = clock.unix_timestamp;
    collateral_reserve.last_update_slot = clock.slot;
    collateral_reserve.last_update_timestamp = clock.unix_timestamp;
    obligation.last_update_slot = clock.slot;

    // Emit liquidation event
    emit!(LiquidationEvent {
        lending_market: lending_market.key(),
        obligation: obligation.key(),
        liquidator: ctx.accounts.liquidator.key(),
        owner: obligation.owner,
        repay_reserve: repay_reserve_key,
        collateral_reserve: collateral_reserve_key,
        repay_amount: actual_repay,
        collateral_seized: collateral_to_seize,
        liquidation_bonus: liquidation_bonus_amount,
        protocol_fee,
        timestamp: clock.unix_timestamp,
    });

    msg!("Liquidation successful!");
    msg!("Repaid: {} debt tokens", actual_repay);
    msg!("Total collateral seized: {} tokens", collateral_to_seize);
    msg!("Liquidator received: {} tokens", liquidator_reward);
    msg!("Protocol fee collected: {} tokens", protocol_fee);

    Ok(())
}

/// Liquidation errors
#[error_code]
pub enum LiquidateError {
    #[msg("Reserve does not belong to this lending market")]
    InvalidReserve,

    #[msg("Obligation does not belong to this lending market")]
    InvalidObligation,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Invalid fee receiver account")]
    InvalidFeeReceiver,

    #[msg("Token mint mismatch")]
    InvalidTokenMint,

    #[msg("Token account owner mismatch")]
    InvalidTokenOwner,

    #[msg("Obligation is healthy, cannot liquidate")]
    ObligationHealthy,

    #[msg("No borrow found for repay reserve")]
    NoBorrowFound,

    #[msg("No collateral found for collateral reserve")]
    NoCollateralFound,

    #[msg("Repay amount too small")]
    RepayAmountTooSmall,

    #[msg("Insufficient collateral to seize")]
    InsufficientCollateral,

    #[msg("Math overflow")]
    MathOverflow,
}
