//! A controllable CPI target for the executor fuzzer. Its instruction data's first byte selects an
//! operation; everything it does is confined to accounts it owns and privileges the transaction
//! granted, so a correctly behaving Ballista run around it stays within the trust model. The op
//! codes match `ballista_fuzz_gen::template::probe`.
#![allow(unexpected_cfgs)]

use pinocchio::{
    account::AccountView,
    address::Address,
    cpi::invoke_with_bounds,
    error::{ProgramError, ProgramResult},
    instruction::{InstructionAccount, InstructionView},
};

pub const NOOP: u8 = 0;
pub const SET_RETURN: u8 = 1;
pub const WRITE_FIRST: u8 = 2;
pub const RESIZE_FIRST: u8 = 3;
pub const FAIL: u8 = 4;
pub const INVOKE: u8 = 5;
pub const TRANSFER: u8 = 6;
pub const FAIL_BASE: u32 = 0x0fee_d000;

#[cfg(not(feature = "no-entrypoint"))]
mod entry {
    use super::process;
    pinocchio::entrypoint!(process, 64);
}

pub fn process(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let (&op, rest) = data.split_first().unwrap_or((&NOOP, &[]));
    match op {
        NOOP | RESIZE_FIRST => Ok(()),
        SET_RETURN => {
            pinocchio::cpi::set_return_data(rest);
            Ok(())
        }
        WRITE_FIRST => {
            let Some(account) = accounts.first_mut() else { return Ok(()) };
            if account.owned_by(program_id) && account.is_writable() {
                if let Ok(mut buffer) = account.try_borrow_mut() {
                    let end = rest.len().min(buffer.len());
                    buffer[..end].copy_from_slice(&rest[..end]);
                }
            }
            Ok(())
        }
        FAIL => {
            let code = FAIL_BASE | u32::from(rest.first().copied().unwrap_or(0));
            Err(ProgramError::Custom(code))
        }
        TRANSFER => {
            let [from, to, ..] = &mut *accounts else { return Ok(()) };
            let amount = u64::from(rest.first().copied().unwrap_or(0));
            if from.owned_by(program_id) && from.is_writable() && to.is_writable() {
                let available = from.lamports();
                if available >= amount {
                    from.set_lamports(available - amount);
                    to.set_lamports(to.lamports().saturating_add(amount));
                }
            }
            Ok(())
        }
        // accounts[0] is the program to call; the rest are forwarded. `rest[0]` is a policy:
        // bit n clears the writable flag of forwarded account n. `rest[1..]` is the data. Kept out
        // of line so its meta array does not weigh on the main frame.
        INVOKE => reinvoke(accounts, rest),
        _ => Err(ProgramError::Custom(FAIL_BASE)),
    }
}

/// How many accounts the probe forwards in a nested invoke. Small, to keep the meta array within
/// the SBF stack frame; enough to forward an open registry entry and a couple of others.
const MAX_FORWARD: usize = 12;

#[inline(never)]
fn reinvoke(accounts: &[AccountView], rest: &[u8]) -> ProgramResult {
    let [target, forwarded @ ..] = accounts else { return Ok(()) };
    let policy = rest.first().copied().unwrap_or(0);
    let inner_data = rest.get(1..).unwrap_or(&[]);
    let count = forwarded.len().min(MAX_FORWARD);
    let mut metas: [core::mem::MaybeUninit<InstructionAccount>; MAX_FORWARD] =
        [const { core::mem::MaybeUninit::uninit() }; MAX_FORWARD];
    for (index, account) in forwarded[..count].iter().enumerate() {
        let demote = index < 8 && policy & (1 << index) != 0;
        metas[index].write(InstructionAccount::new(
            account.address(),
            account.is_writable() && !demote,
            account.is_signer(),
        ));
    }
    // SAFETY: the first `count` slots were just initialized.
    let metas = unsafe { core::slice::from_raw_parts(metas.as_ptr().cast::<InstructionAccount>(), count) };
    let instruction = InstructionView {
        program_id: target.address(),
        accounts: metas,
        data: inner_data,
    };
    invoke_with_bounds::<MAX_FORWARD, _>(&instruction, &forwarded[..count])
}
