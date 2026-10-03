//! Account constraints are enforced exactly as declared, and account header reads return the
//! account's actual fields.
//!
//! Every rule here runs against a symbolic program (see `rules::symbolic`): its constraint records
//! and pubkeys are havoced heap memory constrained through the accessors `validate_account` reads
//! them with. These rules used to parse constant programs written one byte at a time; the parser
//! reads those bytes back as words, which the prover models as unrelated values, so none of them
//! could prove. The rules now also cover more than the constants did: any minimum data length,
//! and any pinned address and owner.

use ballista::error::BallistaError;
use ballista::processor::execute::{
    execute_instruction, validate_runtime_accounts, RunError, RunLayout, RuntimeValue, Scratch,
};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{heap_views, nondet_account_views, AccountSlot};
use pinocchio::error::ProgramError;

use super::symbolic::{self, assume_constraint, empty, InstructionSlot, Shape};
use super::util::{pick, unset_registers, REGISTERS};

/// Records what the validator saw, so a counterexample shows the outcome:
/// 1 = accepted, 2 = missing signature, 3 = constraint or range failure, 4 = other.
fn log_validation(outcome: &Result<RunLayout, RunError>) {
    let outcome_tag: u64 = match outcome {
        Ok(_) => 1,
        Err(RunError::Program(ProgramError::MissingRequiredSignature)) => 2,
        Err(RunError::VmAt(_, _)) => 3,
        Err(_) => 4,
    };
    clog!(outcome_tag);
}

/// A program with one fixed account constrained by exactly these values, and `pubkeys` pubkeys.
fn one_account_program(
    flags: u8,
    address_index: u8,
    owner_index: u8,
    min_data_len: u32,
    pubkeys: usize,
) -> ProgramView<'static> {
    let program = symbolic::program(Shape {
        pubkeys,
        ..Shape::accounts(1)
    });
    assume_constraint(&program.accounts[0], flags, address_index, owner_index, min_data_len);
    program
}

#[rule]
pub fn rule_signer_constraints_require_signers() {
    let program = one_account_program(ACCOUNT_SIGNER, NO_INDEX, NO_INDEX, 0, 0);
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    let outcome = validate_runtime_accounts(&program, &views[..], empty());
    log_validation(&outcome);
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
    let program = one_account_program(flags, NO_INDEX, NO_INDEX, 0, 0);
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    let satisfied = (flags & ACCOUNT_WRITABLE == 0 || account.is_writable())
        && (flags & ACCOUNT_EXECUTABLE == 0 || account.executable());
    match validate_runtime_accounts(&program, &views[..], empty()) {
        Ok(_) => cvlr_assert!(satisfied),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, index)) => {
            cvlr_assert!(!satisfied && index == 0)
        }
        Err(_) => cvlr_assert!(false),
    }
}

/// The account must sit at the pinned address and be owned by the pinned owner, both any pubkey
/// the program declares. The comparison reads both sides as four eight-byte words, as
/// `validate_account` does.
#[rule]
pub fn rule_pinned_address_and_owner_are_enforced() {
    let program = one_account_program(0, 0, 1, 0, 2);
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    let pinned = account.address().as_array() == &program.pubkeys[0].bytes
        && account.owner().as_array() == &program.pubkeys[1].bytes;
    match validate_runtime_accounts(&program, &views[..], empty()) {
        Ok(_) => cvlr_assert!(pinned),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, 0)) => cvlr_assert!(!pinned),
        Err(_) => cvlr_assert!(false),
    }
}

/// Any declared minimum, including ones longer than the account could hold.
#[rule]
pub fn rule_minimum_data_length_is_enforced() {
    let minimum: u32 = nondet();
    let program = one_account_program(0, NO_INDEX, NO_INDEX, minimum, 0);
    let views = nondet_account_views::<1, 128>();
    let account = &views[0];
    clog!(minimum);
    match validate_runtime_accounts(&program, &views[..], empty()) {
        Ok(_) => cvlr_assert!(account.data_len() >= minimum as usize),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, 0)) => {
            cvlr_assert!(account.data_len() < minimum as usize)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_account_count_must_match_the_schema() {
    let program = one_account_program(0, NO_INDEX, NO_INDEX, 0, 0);
    let supplied: usize = nondet();
    cvlr_assume!(supplied <= 3);
    let views = nondet_account_views::<3, 0>();
    let outcome = validate_runtime_accounts(&program, &views[..supplied], empty());
    log_validation(&outcome);
    clog!(supplied);
    match outcome {
        Ok(_) => cvlr_assert!(supplied == 1),
        Err(RunError::VmAt(BallistaError::InvalidAccountRange, count)) => {
            cvlr_assert!(supplied != 1 && count as usize == supplied)
        }
        Err(_) => cvlr_assert!(false),
    }
}

/// Each header read writes the account's field into the destination register. The instruction is
/// a stack record whose opcode the prover knows, so the executor's other opcodes are sliced away.
#[rule]
pub fn rule_account_header_reads_return_the_account_fields() {
    let program = symbolic::program(Shape {
        registers: REGISTERS,
        ..Shape::accounts(1)
    });
    let slot = AccountSlot::<64>::nondet();
    let views = heap_views([slot.view()]);
    let opcode = pick!(
        OP_ACCOUNT_KEY,
        OP_ACCOUNT_OWNER,
        OP_ACCOUNT_LAMPORTS,
        OP_ACCOUNT_DATA_LEN,
        OP_ACCOUNT_IS_EMPTY
    );
    let dst = pick!(0u8, 1, 2, 3);
    let mut record = core::mem::MaybeUninit::uninit();
    let instruction = InstructionSlot::write(&mut record, opcode, dst, 0, NO_INDEX, NO_INDEX, 0, 0);
    let mut registers = unset_registers();
    let mut scratch = Scratch::new(&program);
    let inputs: &[RuntimeValue] = empty();
    let outcome = execute_instruction(
        &program,
        inputs,
        &views[..],
        &mut registers,
        &mut scratch,
        instruction,
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
