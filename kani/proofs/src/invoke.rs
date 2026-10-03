//! Invocations (`invoke_cpi` in `execute.rs`): what each CPI receives, and the batch cache that
//! skips rebuilding a call on every row.
//!
//! The syscall is stubbed with a recorder, so a harness sees every call exactly as the runtime
//! would: the program id, each account meta with its signer and writable flags, the account views
//! beside them, the data, and the signer seeds. Off-chain the real function is a no-op.

use ballista::processor::execute::{
    execute_instruction, execute_program, RunLayout, RuntimeValue, Scratch,
};
use ballista_common::template::*;
use pinocchio::cpi::{CpiAccount, Signer};
use pinocchio::error::ProgramError;
use pinocchio::instruction::InstructionView;
use pinocchio::{AccountView, Address};

use crate::accounts::{AccountMemory, Fields};
use crate::util::one_of;

const MAX_CALLS: usize = 4;
const MAX_METAS: usize = 3;
const MAX_DATA: usize = 10;

/// One recorded call.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Call {
    program: [u8; 32],
    metas: [([u8; 32], bool, bool); MAX_METAS],
    meta_count: usize,
    data: [u8; MAX_DATA],
    data_len: usize,
    seeds: usize,
    /// Each meta names the same account as the view passed beside it.
    views_match: bool,
}

const NO_CALL: Call = Call {
    program: [0; 32],
    metas: [([0; 32], false, false); MAX_METAS],
    meta_count: 0,
    data: [0; MAX_DATA],
    data_len: 0,
    seeds: 0,
    views_match: true,
};

struct Log {
    calls: [Call; MAX_CALLS],
    count: usize,
}

static mut LOGS: [Log; 2] = [
    Log { calls: [NO_CALL; MAX_CALLS], count: 0 },
    Log { calls: [NO_CALL; MAX_CALLS], count: 0 },
];
static mut CURRENT: usize = 0;

fn log(index: usize) -> &'static mut Log {
    // SAFETY: harnesses are single-threaded, and no reference outlives the statement using it.
    unsafe { &mut (*core::ptr::addr_of_mut!(LOGS))[index] }
}

/// The address a `CpiAccount` points at. Its fields are private, but it is `repr(C)` with the
/// address pointer first.
fn view_address(account: &CpiAccount) -> *const Address {
    // SAFETY: `CpiAccount` is `repr(C)` and its first field is `address: *const Address`.
    unsafe { *(account as *const CpiAccount as *const *const Address) }
}

/// Two recorded calls are the same, compared without a byte loop.
fn same_call(a: &Call, b: &Call) -> bool {
    let mut same = crate::util::eq32(&a.program, &b.program)
        && a.meta_count == b.meta_count
        && a.data_len == b.data_len
        && a.seeds == b.seeds
        && a.views_match == b.views_match;
    for i in 0..MAX_METAS {
        same &= crate::util::eq32(&a.metas[i].0, &b.metas[i].0) && a.metas[i].1 == b.metas[i].1 && a.metas[i].2 == b.metas[i].2;
    }
    for i in 0..MAX_DATA {
        same &= a.data[i] == b.data[i];
    }
    same
}

/// Stub for `pinocchio::cpi::invoke_signed_unchecked`: records the call in the current log.
#[allow(dead_code)] // referenced only from `#[kani::stub]`
pub unsafe fn record_invocation(instruction: &InstructionView, accounts: &[CpiAccount], signers_seeds: &[Signer]) {
    // SAFETY: as `log`.
    let log = log(unsafe { CURRENT });
    assert!(log.count < MAX_CALLS);
    assert!(instruction.accounts.len() <= MAX_METAS && instruction.data.len() <= MAX_DATA);
    let mut call = NO_CALL;
    call.program = instruction.program_id.to_bytes();
    call.meta_count = instruction.accounts.len();
    for (i, meta) in instruction.accounts.iter().enumerate() {
        call.metas[i] = (meta.address.to_bytes(), meta.is_signer, meta.is_writable);
    }
    call.views_match = accounts.len() == instruction.accounts.len();
    for (meta, view) in instruction.accounts.iter().zip(accounts) {
        call.views_match &= core::ptr::eq(meta.address, view_address(view));
    }
    call.data_len = instruction.data.len();
    call.data[..instruction.data.len()].copy_from_slice(instruction.data);
    call.seeds = signers_seeds.len();
    log.calls[log.count] = call;
    log.count += 1;
}

/// The body of the loop: an invoke of descriptor 0, the loop index, or a constant, into register
/// 0 or 1.
fn body_instruction() -> InstructionRecord {
    match one_of(&[OP_INVOKE, OP_LOOP_INDEX, OP_CONST_U64]) {
        OP_INVOKE => record(OP_INVOKE, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, 0),
        opcode => record(opcode, kani::any_where(|r: &u8| *r <= 1), 0, 0, 0, 0, kani::any_where(|n: &u64| *n <= 300)),
    }
}

/// A batch loop whose body invokes one descriptor makes, on every row, exactly the call a fresh
/// build would make: the real run, which builds the account list on the first row and afterwards
/// rebinds only the row accounts (and keeps the data when it judges it loop-invariant), records
/// the same calls as running the same passes one instruction at a time with no cache. Every call
/// also passes each declared account with exactly its record's signer and writable flags, each
/// group member writable only as the transaction marked it and never as a signer, a view beside
/// each meta naming the same account, and no signer seeds. Bound: 2 rows of 1 account after 2
/// fixed accounts, a group of 1 (forwarded or not); a body of an `INVOKE` then a loop index, a
/// constant or a second `INVOKE`; a descriptor with up to 2 account records (fixed or row, any
/// flags) and up to 1 data segment (a literal, or a `u64` from register 0 or 1); any carry mask;
/// all 7 accounts at distinct addresses.
#[kani::proof]
#[kani::unwind(11)]
#[kani::stub(pinocchio::cpi::invoke_signed_unchecked, crate::invoke::record_invocation)]
fn batch_invocations_match_a_fresh_build_every_row() {
    let stride: u8 = 1;
    let rows: usize = 2;
    let header = ProgramHeader::new(2, stride, 2, 0, 0, 2, 3, 1, 2, 2, 0, 0, 4, 0, 1);
    let reference = || {
        let offset: u8 = kani::any_where(|n: &u8| *n <= 1);
        if kani::any() { offset | ITERATION_ACCOUNT_BIT } else { offset }
    };
    let descriptor = CpiDescriptor {
        program_account: reference(),
        account_group: if kani::any() { 0 } else { NO_INDEX },
        account_start_le: 0u16.to_le_bytes(),
        account_len: kani::any_where(|n: &u8| *n <= 2),
        segment_len: kani::any_where(|n: &u8| *n <= 1),
        segment_start_le: 0u16.to_le_bytes(),
        max_data_len_le: (MAX_DATA as u16).to_le_bytes(),
        reserved1: [0; 2],
    };
    let records = [
        CpiAccountRecord { account: reference(), flags: kani::any::<u8>() & (ACCOUNT_SIGNER | ACCOUNT_WRITABLE) },
        CpiAccountRecord { account: reference(), flags: kani::any::<u8>() & (ACCOUNT_SIGNER | ACCOUNT_WRITABLE) },
    ];
    let segment = || {
        let kind = one_of(&[DATA_LITERAL, DATA_REG_U64]);
        if kind == DATA_LITERAL {
            DataSegment { kind, register: NO_INDEX, offset_le: 0u16.to_le_bytes(), len_le: 2u16.to_le_bytes(), reserved: [0; 2] }
        } else {
            DataSegment { kind, register: kani::any_where(|r: &u8| *r <= 1), offset_le: [0; 2], len_le: [0; 2], reserved: [0; 2] }
        }
    };
    let segments = [segment(), segment()];
    let blob: [u8; 4] = kani::any();
    let instructions = [
        record(OP_FOREACH, NO_INDEX, 2, 0, 0, 0, kani::any()),
        record(OP_INVOKE, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, 0),
        body_instruction(),
    ];
    let constraints = [AccountConstraint { flags: 0, address_index: NO_INDEX, owner_index: NO_INDEX, reserved: 0, min_data_len_le: [0; 4] }; 4];
    let program = ProgramView {
        header: &header,
        accounts: &constraints[..2 + stride as usize],
        inputs: &[],
        instructions: &instructions,
        cpis: core::slice::from_ref(&descriptor),
        cpi_accounts: &records,
        data_segments: &segments,
        pubkeys: &[],
        blob: &blob,
    };

    // Two fixed accounts, the rows, then one group member: account i is at address [i + 1; 32].
    let mut memory: [AccountMemory<0>; 7] = core::array::from_fn(|i| {
        let mut fields = Fields::plain([i as u8 + 1; 32], [0; 32]);
        fields.writable = kani::any();
        fields.signer = kani::any();
        AccountMemory::new(fields, [], 0)
    });
    let views: [AccountView; 7] = {
        let base = memory.as_mut_ptr();
        core::array::from_fn(|i| unsafe { (*base.add(i)).view() })
    };
    let declared = 2 + stride as usize * rows;
    let accounts = &views[..declared + 1];
    let mut groups = [(0u8, 0u8); MAX_ACCOUNT_GROUPS];
    groups[0] = (declared as u8, 1);
    let layout = RunLayout { iterations: rows, declared, groups };
    let initial: [RuntimeValue<'_>; 2] = [RuntimeValue::U64(kani::any_where(|n: &u64| *n <= 300)), RuntimeValue::U64(kani::any())];

    // The real run.
    unsafe { CURRENT = 0 };
    let mut registers = initial;
    let mut scratch = Scratch::new(&program);
    scratch.set_groups(&layout);
    let real = execute_program(&program, &[], accounts, rows, &mut registers, &mut scratch);

    // The same passes, one instruction at a time, with no loop and so no cache.
    unsafe { CURRENT = 1 };
    let carry = instructions[0].immediate();
    let carried = |register: usize| carry & (1u64 << register) != 0;
    let mut scratch = Scratch::new(&program);
    scratch.set_groups(&layout);
    let mut state = initial;
    let modelled: Result<(), ProgramError> = (|| {
        for pass in 0..rows {
            let mut file: [RuntimeValue<'_>; 2] =
                core::array::from_fn(|r| if carried(r) { state[r] } else { initial[r] });
            let row_base = 2 + pass * stride as usize;
            for (k, instruction) in instructions[1..].iter().enumerate() {
                execute_instruction(&program, &[], accounts, &mut file, &mut scratch, instruction, Some((pass, row_base)))
                    .map_err(|error| crate::executor::failure_at(error, 1 + k))?;
            }
            state = file;
        }
        Ok(())
    })();

    assert_eq!(real, modelled);
    let (real_log, model_log) = (log(0), log(1));
    if real.is_ok() {
        assert_eq!(real_log.count, model_log.count);
        for i in 0..real_log.count {
            assert!(same_call(&real_log.calls[i], &model_log.calls[i]), "a cached call differs from a fresh build");
        }
        let invokes = instructions[1..].iter().filter(|record| record.opcode == OP_INVOKE).count();
        kani::cover!(rows == 2 && invokes == 1 && real_log.count == 2, "one call per row, the second from the cache");
        kani::cover!(rows == 2 && invokes == 2 && real_log.count == 4, "two calls per row");
        kani::cover!(
            rows == 2 && real_log.count == 2 && descriptor.program_account & ITERATION_ACCOUNT_BIT != 0,
            "a row program account"
        );
        kani::cover!(rows == 2 && real_log.count == 2 && descriptor.account_group == 0, "a forwarded group");
    }
    // Privileges, on every call of the fresh build.
    for i in 0..model_log.count {
        let call = &model_log.calls[i];
        assert_eq!(call.seeds, 0, "a template's call carries no signer seeds");
        assert!(call.views_match);
        let declared_metas = descriptor.account_len as usize;
        for (slot, meta) in call.metas[..call.meta_count].iter().enumerate() {
            if slot < declared_metas {
                let flags = records[slot].flags;
                assert_eq!(meta.1, flags & ACCOUNT_SIGNER != 0);
                assert_eq!(meta.2, flags & ACCOUNT_WRITABLE != 0);
            } else {
                // The group member: never a signer, writable as the transaction marked it.
                assert!(!meta.1);
                assert!(crate::util::eq32(&meta.0, &[declared as u8 + 1; 32]));
                assert_eq!(meta.2, memory[declared].header.is_writable != 0);
            }
        }
    }
}
