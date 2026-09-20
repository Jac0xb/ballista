//! Account constraints are enforced exactly as declared, and account header reads return the
//! account's actual fields.

use ballista::error::BallistaError;
use ballista::processor::execute::{
    execute_instruction, validate_runtime_accounts, RunError, RunLayout, RuntimeValue, Scratch,
};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{heap_views, nondet_account_views, AccountSlot};
use pinocchio::error::ProgramError;

use super::util::{
    constrained_program, pick, pinned_program, spec_program, unset_registers, REGISTERS,
};

/// Records what the validator saw, so a counterexample shows the parsed schema next to the
/// outcome: 1 = accepted, 2 = missing signature, 3 = constraint or range failure, 4 = other.
fn log_validation(program: &ProgramView<'_>, outcome: &Result<RunLayout, RunError>) {
    let declared_accounts = program.header.fixed_account_count() as u64;
    let first_flags = program.accounts.first().map_or(u64::MAX, |record| record.flags as u64);
    let first_min_len = program.accounts.first().map_or(u64::MAX, |record| record.min_data_len() as u64);
    let outcome_tag: u64 = match outcome {
        Ok(_) => 1,
        Err(RunError::Program(ProgramError::MissingRequiredSignature)) => 2,
        Err(RunError::VmAt(_, _)) => 3,
        Err(_) => 4,
    };
    clog!(declared_accounts, first_flags, first_min_len, outcome_tag);
}

#[rule]
pub fn rule_signer_constraints_require_signers() {
    let program = ProgramView::parse(constrained_program(ACCOUNT_SIGNER, 0)).expect("parses");
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    let outcome = validate_runtime_accounts(&program, &views[..], &[]);
    log_validation(&program, &outcome);
    match outcome {
        Ok(layout) => {
            cvlr_assert!(layout.iterations == 0);
            cvlr_assert!(account.is_signer());
        }
        Err(RunError::Program(ProgramError::MissingRequiredSignature)) => {
            cvlr_assert!(!account.is_signer())
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_writable_and_executable_constraints_are_enforced() {
    let flags = pick!(
        ACCOUNT_WRITABLE,
        ACCOUNT_EXECUTABLE,
        ACCOUNT_WRITABLE | ACCOUNT_EXECUTABLE
    );
    let program = ProgramView::parse(constrained_program(flags, 0)).expect("parses");
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    let satisfied = (flags & ACCOUNT_WRITABLE == 0 || account.is_writable())
        && (flags & ACCOUNT_EXECUTABLE == 0 || account.executable());
    match validate_runtime_accounts(&program, &views[..], &[]) {
        Ok(_) => cvlr_assert!(satisfied),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, index)) => {
            cvlr_assert!(!satisfied && index == 0)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_pinned_address_and_owner_are_enforced() {
    let program = ProgramView::parse(pinned_program()).expect("parses");
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    // Compare against the program's own pubkey table (on the heap) rather than the constants,
    // which would be read from the binary's data section.
    let pinned = account.address().as_ref() == program.pubkeys[0].bytes
        && account.owner().as_ref() == program.pubkeys[1].bytes;
    match validate_runtime_accounts(&program, &views[..], &[]) {
        Ok(_) => cvlr_assert!(pinned),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, 0)) => cvlr_assert!(!pinned),
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_minimum_data_length_is_enforced() {
    let minimum: u32 = nondet();
    cvlr_assume!(minimum <= 128);
    let program = ProgramView::parse(constrained_program(0, minimum)).expect("parses");
    let views = nondet_account_views::<1, 128>();
    let account = &views[0];
    match validate_runtime_accounts(&program, &views[..], &[]) {
        Ok(_) => cvlr_assert!(account.data_len() >= minimum as usize),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, 0)) => {
            cvlr_assert!(account.data_len() < minimum as usize)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_account_count_must_match_the_schema() {
    let program = ProgramView::parse(constrained_program(0, 0)).expect("parses");
    let supplied: usize = nondet();
    cvlr_assume!(supplied <= 3);
    let views = nondet_account_views::<3, 0>();
    let outcome = validate_runtime_accounts(&program, &views[..supplied], &[]);
    log_validation(&program, &outcome);
    clog!(supplied);
    match outcome {
        Ok(_) => cvlr_assert!(supplied == 1),
        Err(RunError::VmAt(BallistaError::InvalidAccountRange, count)) => {
            cvlr_assert!(supplied != 1 && count as usize == supplied)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_account_header_reads_return_the_account_fields() {
    let program = ProgramView::parse(spec_program(true)).expect("parses");
    let slot = AccountSlot::<64>::nondet();
    let views = heap_views([slot.view()]);
    let opcode = pick!(
        OP_ACCOUNT_KEY,
        OP_ACCOUNT_OWNER,
        OP_ACCOUNT_LAMPORTS,
        OP_ACCOUNT_DATA_LEN,
        OP_ACCOUNT_IS_EMPTY
    );
    let dst: u8 = nondet();
    cvlr_assume!((dst as usize) < REGISTERS);
    let instruction = record(opcode, dst, 0, NO_INDEX, NO_INDEX, 0, 0);
    let mut registers = unset_registers();
    let mut scratch = Scratch::new(&program);
    let inputs: [RuntimeValue; 0] = [];
    let outcome = execute_instruction(
        &program,
        &inputs,
        &views[..],
        &mut registers,
        &mut scratch,
        &instruction,
        None,
    );
    cvlr_assert!(outcome.is_ok());
    let expected = match opcode {
        OP_ACCOUNT_KEY => RuntimeValue::Pubkey(slot.header.address.to_bytes()),
        OP_ACCOUNT_OWNER => RuntimeValue::Pubkey(slot.header.owner.to_bytes()),
        OP_ACCOUNT_LAMPORTS => RuntimeValue::U64(slot.header.lamports),
        OP_ACCOUNT_DATA_LEN => RuntimeValue::U64(slot.header.data_len),
        _ => RuntimeValue::Bool(slot.header.data_len == 0),
    };
    cvlr_assert!(registers[dst as usize] == expected);
}
