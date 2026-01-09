use anchor_lang::prelude::*;

use crate::state::LendingMarket;
use crate::events::EmergencyModeChanged;

/// Accounts for setting emergency mode
#[derive(Accounts)]
pub struct SetEmergencyMode<'info> {
    /// Authority of the lending market (must sign)
    pub authority: Signer<'info>,

    /// The lending market to update
    #[account(
        mut,
        has_one = authority,
        seeds = [LendingMarket::SEED_PREFIX, authority.key().as_ref()],
        bump = lending_market.bump
    )]
    pub lending_market: Account<'info, LendingMarket>,
}

/// Set emergency mode on/off
///
/// When emergency mode is ON:
/// - Deposits are DISABLED
/// - Borrows are DISABLED
/// - Withdrawals are ENABLED (users can exit)
/// - Repayments are ENABLED (borrowers can repay)
/// - Liquidations are ENABLED (protect protocol)
///
/// Use this in case of:
/// - Oracle failure / manipulation
/// - Smart contract vulnerability discovered
/// - Market manipulation detected
/// - Protocol pause for upgrade
///
/// # Arguments
/// * `ctx` - The context containing all accounts
/// * `emergency` - true to enable, false to disable
pub fn handler(
    ctx: Context<SetEmergencyMode>,
    emergency: bool,
) -> Result<()> {
    let lending_market = &mut ctx.accounts.lending_market;
    let clock = Clock::get()?;

    let previous_state = lending_market.emergency_mode;
    lending_market.emergency_mode = emergency;

    // Emit event
    emit!(EmergencyModeChanged {
        lending_market: lending_market.key(),
        emergency_mode: emergency,
        timestamp: clock.unix_timestamp,
    });

    if emergency {
        msg!("EMERGENCY MODE ACTIVATED");
        msg!("Deposits: DISABLED");
        msg!("Borrows: DISABLED");
        msg!("Withdrawals: ENABLED");
        msg!("Repayments: ENABLED");
        msg!("Liquidations: ENABLED");
    } else {
        msg!("Emergency mode deactivated");
        msg!("Normal operations resumed");
    }

    msg!("Previous state: {}", previous_state);
    msg!("New state: {}", lending_market.emergency_mode);

    Ok(())
}
