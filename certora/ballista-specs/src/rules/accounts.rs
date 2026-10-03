//! Account constraints are enforced exactly as declared, and account header reads return the
//! account's actual fields.
//!
//! Every rule here runs against a symbolic program (see `rules::symbolic`): its constraint records
//! and pubkeys are havoced heap memory constrained through the accessors `validate_account` reads
//! them with. These rules used to parse constant programs written one byte at a time; the parser
//! reads those bytes back as words, which the prover models as unrelated values. They now also
//! cover more than the constants did: any minimum data length, and any pinned address and owner.
//!
//! Blocked all the same. `validate_account` builds each error in a stack temporary from a two-byte
//! tag and a four-byte code and copies it out with an eight-byte move across two bytes it never
//! wrote, which the prover cannot rebuild. To `validate_runtime_accounts`, and so to these rules, a
//! refused account can read as an accepted one. The header-read rule meets the same problem in the
//! executor's register write: it copies a value whose tag is one byte as eight-byte words.
//!
//! Each rule has a reachability rule for each branch that asserts and a twin that must fail.

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

/// What `validate_runtime_accounts` returned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Validation {
    Accepted { iterations: usize },
    MissingSignature,
    ConstraintFailed { index: u16 },
    WrongCount { count: u16 },
    Other,
}

fn classify(outcome: &Result<RunLayout, RunError>) -> Validation {
    let validation = match outcome {
        Ok(layout) => Validation::Accepted { iterations: layout.iterations },
        Err(RunError::Program(ProgramError::MissingRequiredSignature)) => Validation::MissingSignature,
        Err(RunError::VmAt(BallistaError::AccountConstraintFailed, index)) => {
            Validation::ConstraintFailed { index: *index }
        }
        Err(RunError::VmAt(BallistaError::InvalidAccountRange, count)) => {
            Validation::WrongCount { count: *count }
        }
        Err(_) => Validation::Other,
    };
    let tag: u64 = match validation {
        Validation::Accepted { .. } => 1,
        Validation::MissingSignature => 2,
        Validation::ConstraintFailed { .. } => 3,
        Validation::WrongCount { .. } => 4,
        Validation::Other => 5,
    };
    clog!(tag);
    validation
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

// ------------------------------------------------------------------------------------------------
// Signers.

/// One account against a signer constraint: whether it signed, and the outcome.
fn signer_case() -> (bool, Validation) {
    let program = one_account_program(ACCOUNT_SIGNER, NO_INDEX, NO_INDEX, 0, 0);
    let views = nondet_account_views::<1, 0>();
    let signed = views[0].is_signer();
    (signed, classify(&validate_runtime_accounts(&program, &views[..], empty())))
}

#[rule]
pub fn rule_signer_constraints_require_signers() {
    match signer_case() {
        (signed, Validation::Accepted { iterations }) => cvlr_assert!(signed && iterations == 0),
        (signed, Validation::MissingSignature) => cvlr_assert!(!signed),
        _ => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_signer_constraints_reach_acceptance() {
    cvlr_satisfy!(matches!(signer_case().1, Validation::Accepted { .. }));
}

#[rule]
pub fn rule_signer_constraints_reach_a_missing_signature() {
    cvlr_satisfy!(signer_case().1 == Validation::MissingSignature);
}

/// Twin that must fail: it claims an account that did not sign is accepted.
#[rule]
pub fn rule_signer_constraints_twin_accept_non_signers() {
    if let (signed, Validation::Accepted { .. }) = signer_case() {
        cvlr_assert!(!signed);
    }
}

// ------------------------------------------------------------------------------------------------
// Writable and executable.

/// One account against a writable, executable or both constraint: whether it meets it, and the
/// outcome.
fn flags_case() -> (bool, Validation) {
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
    (satisfied, classify(&validate_runtime_accounts(&program, &views[..], empty())))
}

#[rule]
pub fn rule_writable_and_executable_constraints_are_enforced() {
    match flags_case() {
        (satisfied, Validation::Accepted { .. }) => cvlr_assert!(satisfied),
        (satisfied, Validation::ConstraintFailed { index }) => cvlr_assert!(!satisfied && index == 0),
        _ => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_writable_and_executable_constraints_reach_acceptance() {
    cvlr_satisfy!(matches!(flags_case().1, Validation::Accepted { .. }));
}

#[rule]
pub fn rule_writable_and_executable_constraints_reach_a_refusal() {
    cvlr_satisfy!(matches!(flags_case().1, Validation::ConstraintFailed { .. }));
}

/// Twin that must fail: it claims only accounts that miss the constraint are accepted.
#[rule]
pub fn rule_writable_and_executable_constraints_twin_accept_misses() {
    if let (satisfied, Validation::Accepted { .. }) = flags_case() {
        cvlr_assert!(!satisfied);
    }
}

// ------------------------------------------------------------------------------------------------
// Pinned address and owner.

/// One account against a pinned address and owner, both any pubkey the program declares: whether
/// it matches both, and the outcome. The comparison reads both sides as four eight-byte words, as
/// `validate_account` does.
fn pinned_case() -> (bool, Validation) {
    let program = one_account_program(0, 0, 1, 0, 2);
    let views = nondet_account_views::<1, 0>();
    let account = &views[0];
    let pinned = account.address().as_array() == &program.pubkeys[0].bytes
        && account.owner().as_array() == &program.pubkeys[1].bytes;
    (pinned, classify(&validate_runtime_accounts(&program, &views[..], empty())))
}

#[rule]
pub fn rule_pinned_address_and_owner_are_enforced() {
    match pinned_case() {
        (pinned, Validation::Accepted { .. }) => cvlr_assert!(pinned),
        (pinned, Validation::ConstraintFailed { index: 0 }) => cvlr_assert!(!pinned),
        _ => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_pinned_address_and_owner_reach_acceptance() {
    cvlr_satisfy!(matches!(pinned_case().1, Validation::Accepted { .. }));
}

#[rule]
pub fn rule_pinned_address_and_owner_reach_a_refusal() {
    cvlr_satisfy!(pinned_case().1 == Validation::ConstraintFailed { index: 0 });
}

/// Twin that must fail: it claims a matching account is refused.
#[rule]
pub fn rule_pinned_address_and_owner_twin_accept_strangers() {
    if let (pinned, Validation::Accepted { .. }) = pinned_case() {
        cvlr_assert!(!pinned);
    }
}

// ------------------------------------------------------------------------------------------------
// Minimum data length.

/// One account of up to 128 data bytes against any declared minimum: its length, the minimum and
/// the outcome.
fn length_case() -> (usize, usize, Validation) {
    let minimum: u32 = nondet();
    let program = one_account_program(0, NO_INDEX, NO_INDEX, minimum, 0);
    let views = nondet_account_views::<1, 128>();
    let length = views[0].data_len();
    clog!(minimum, length);
    (length, minimum as usize, classify(&validate_runtime_accounts(&program, &views[..], empty())))
}

/// Any declared minimum, including ones longer than the account could hold.
#[rule]
pub fn rule_minimum_data_length_is_enforced() {
    match length_case() {
        (length, minimum, Validation::Accepted { .. }) => cvlr_assert!(length >= minimum),
        (length, minimum, Validation::ConstraintFailed { index: 0 }) => cvlr_assert!(length < minimum),
        _ => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_minimum_data_length_reaches_acceptance() {
    cvlr_satisfy!(matches!(length_case().2, Validation::Accepted { .. }));
}

#[rule]
pub fn rule_minimum_data_length_reaches_a_refusal() {
    cvlr_satisfy!(length_case().2 == Validation::ConstraintFailed { index: 0 });
}

/// Twin that must fail: it claims an account exactly as long as the minimum is refused.
#[rule]
pub fn rule_minimum_data_length_twin_is_strict() {
    if let (length, minimum, Validation::Accepted { .. }) = length_case() {
        cvlr_assert!(length > minimum);
    }
}

// ------------------------------------------------------------------------------------------------
// Account count.

/// Up to three accounts against one declared account: how many, and the outcome.
fn count_case() -> (usize, Validation) {
    let program = one_account_program(0, NO_INDEX, NO_INDEX, 0, 0);
    let supplied: usize = nondet();
    cvlr_assume!(supplied <= 3);
    let views = nondet_account_views::<3, 0>();
    clog!(supplied);
    (supplied, classify(&validate_runtime_accounts(&program, &views[..supplied], empty())))
}

#[rule]
pub fn rule_account_count_must_match_the_schema() {
    match count_case() {
        (supplied, Validation::Accepted { .. }) => cvlr_assert!(supplied == 1),
        (supplied, Validation::WrongCount { count }) => {
            cvlr_assert!(supplied != 1 && count as usize == supplied)
        }
        _ => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_account_count_reaches_acceptance() {
    cvlr_satisfy!(matches!(count_case().1, Validation::Accepted { .. }));
}

#[rule]
pub fn rule_account_count_reaches_a_wrong_count() {
    cvlr_satisfy!(matches!(count_case().1, Validation::WrongCount { .. }));
}

/// Twin that must fail: it claims two accounts are what one declaration accepts.
#[rule]
pub fn rule_account_count_twin_wants_two() {
    if let (supplied, Validation::Accepted { .. }) = count_case() {
        cvlr_assert!(supplied == 2);
    }
}

// ------------------------------------------------------------------------------------------------
// Header reads.

/// One header read of an unconstrained account into a register: whether it succeeded, the register
/// afterwards, and the field it should hold. The instruction is a stack record whose opcode the
/// prover knows, so the executor's other opcodes are sliced away.
fn header_read_case() -> (bool, RuntimeValue<'static>, RuntimeValue<'static>) {
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
    let expected = match opcode {
        OP_ACCOUNT_KEY => RuntimeValue::Pubkey(slot.header.address.to_bytes()),
        OP_ACCOUNT_OWNER => RuntimeValue::Pubkey(slot.header.owner.to_bytes()),
        OP_ACCOUNT_LAMPORTS => RuntimeValue::U64(slot.header.lamports),
        OP_ACCOUNT_DATA_LEN => RuntimeValue::U64(slot.header.data_len),
        _ => RuntimeValue::Bool(slot.header.data_len == 0),
    };
    (outcome.is_ok(), registers[dst as usize], expected)
}

/// Each header read writes the account's field into the destination register.
#[rule]
pub fn rule_account_header_reads_return_the_account_fields() {
    let (ok, register, expected) = header_read_case();
    cvlr_assert!(ok);
    cvlr_assert!(register == expected);
}

/// Twin that must fail: it claims the register ends up holding something else.
#[rule]
pub fn rule_account_header_reads_twin_return_another_value() {
    let (_, register, expected) = header_read_case();
    cvlr_assert!(register != expected);
}
