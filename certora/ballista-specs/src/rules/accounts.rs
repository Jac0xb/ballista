//! Account constraints are enforced exactly as declared, and account header reads return the
//! account's actual fields.

use ballista::error::BallistaError;
use ballista::processor::execute::{
    execute_instruction, validate_runtime_accounts, RunError, RuntimeValue, Scratch,
};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{nondet_account_view, AccountSlot};
use pinocchio::error::ProgramError;

use super::util::{constrained_program, pick, spec_program, REGISTERS};

const PINNED_ADDRESS: [u8; 32] = [1; 32];
const PINNED_OWNER: [u8; 32] = [2; 32];

#[rule]
pub fn rule_signer_constraints_require_signers() {
    let bytes = constrained_program(ACCOUNT_SIGNER, None, None, 0);
    let program = ProgramView::parse(&bytes).expect("parses");
    let account = nondet_account_view::<0>();
    match validate_runtime_accounts(&program, core::slice::from_ref(&account)) {
        Ok(iterations) => {
            cvlr_assert!(iterations == 0);
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
    let flags = pick(&[ACCOUNT_WRITABLE, ACCOUNT_EXECUTABLE, ACCOUNT_WRITABLE | ACCOUNT_EXECUTABLE]);
    let bytes = constrained_program(flags, None, None, 0);
    let program = ProgramView::parse(&bytes).expect("parses");
    let account = nondet_account_view::<0>();
    let satisfied = (flags & ACCOUNT_WRITABLE == 0 || account.is_writable())
        && (flags & ACCOUNT_EXECUTABLE == 0 || account.executable());
    match validate_runtime_accounts(&program, core::slice::from_ref(&account)) {
        Ok(_) => cvlr_assert!(satisfied),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, index)) => {
            cvlr_assert!(!satisfied && index == 0)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_pinned_address_and_owner_are_enforced() {
    let bytes = constrained_program(0, Some(PINNED_ADDRESS), Some(PINNED_OWNER), 0);
    let program = ProgramView::parse(&bytes).expect("parses");
    let account = nondet_account_view::<0>();
    let pinned = account.address().as_ref() == PINNED_ADDRESS && account.owner().as_ref() == PINNED_OWNER;
    match validate_runtime_accounts(&program, core::slice::from_ref(&account)) {
        Ok(_) => cvlr_assert!(pinned),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, 0)) => cvlr_assert!(!pinned),
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_minimum_data_length_is_enforced() {
    let minimum: u32 = nondet();
    cvlr_assume!(minimum <= 128);
    let bytes = constrained_program(0, None, None, minimum);
    let program = ProgramView::parse(&bytes).expect("parses");
    let account = nondet_account_view::<128>();
    match validate_runtime_accounts(&program, core::slice::from_ref(&account)) {
        Ok(_) => cvlr_assert!(account.data_len() >= minimum as usize),
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, 0)) => {
            cvlr_assert!(account.data_len() < minimum as usize)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_account_count_must_match_the_schema() {
    let bytes = constrained_program(0, None, None, 0);
    let program = ProgramView::parse(&bytes).expect("parses");
    let supplied: usize = nondet();
    cvlr_assume!(supplied <= 3);
    let views = cvlr_pinocchio::nondet_account_views::<3, 0>();
    match validate_runtime_accounts(&program, &views[..supplied]) {
        Ok(_) => cvlr_assert!(supplied == 1),
        Err(RunError::VmAt(BallistaError::InvalidAccountRange, count)) => {
            cvlr_assert!(supplied != 1 && count as usize == supplied)
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_account_header_reads_return_the_account_fields() {
    let bytes = spec_program(true);
    let program = ProgramView::parse(&bytes).expect("parses");
    let slot = AccountSlot::<64>::nondet();
    let account = slot.view();
    let opcode = pick(&[
        OP_ACCOUNT_KEY,
        OP_ACCOUNT_OWNER,
        OP_ACCOUNT_LAMPORTS,
        OP_ACCOUNT_DATA_LEN,
        OP_ACCOUNT_IS_EMPTY,
    ]);
    let dst: u8 = nondet();
    cvlr_assume!((dst as usize) < REGISTERS);
    let instruction = record(opcode, dst, 0, NO_INDEX, NO_INDEX, 0, 0);
    let mut registers = vec![RuntimeValue::Unset; REGISTERS];
    let mut scratch = Scratch::new(&program);
    let inputs: [RuntimeValue; 0] = [];
    let outcome = execute_instruction(
        &program,
        &inputs,
        core::slice::from_ref(&account),
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
