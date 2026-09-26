//! Error codes round-trip through their encoding, and runtime and verifier codes occupy disjoint
//! ranges with their context in the high half.
//!
//! `decode_ballista_error` itself has no rule: it reads the name table in the binary's data
//! section at a symbolic index, which the prover's pointer analysis does not follow. Its behaviour
//! is covered by the host tests in `ballista-common`; the range facts it relies on are proved here.

use ballista::error::{vm_error, BallistaError};
use ballista_common::template::*;
use cvlr::prelude::*;
use pinocchio::error::ProgramError;

use super::util::pick;

#[rule]
pub fn rule_error_codes_round_trip() {
    let kind: u32 = nondet();
    cvlr_assume!(kind <= 0xffff);
    let context: u16 = nondet();
    let code = encode_error(kind, context);
    clog!(kind, context, code);
    cvlr_assert!(decode_error(code) == (kind, context));
}

#[rule]
pub fn rule_runtime_error_codes_carry_context_and_stay_in_range() {
    let kind = pick!(
        BallistaError::InvalidInstructionData,
        BallistaError::InvalidTemplateAccount,
        BallistaError::InvalidTemplateProgram,
        BallistaError::TemplateNotUploading,
        BallistaError::TemplateNotFinalized,
        BallistaError::InvalidCreator,
        BallistaError::InvalidChunkOffset,
        BallistaError::HashMismatch,
        BallistaError::InvalidRunInputs,
        BallistaError::InvalidRuntimeAccount,
        BallistaError::InvalidAccountRange,
        BallistaError::InvalidRegister,
        BallistaError::TypeMismatch,
        BallistaError::ArithmeticOverflow,
        BallistaError::DivisionByZero,
        BallistaError::RequirementFailed,
        BallistaError::CpiDataTooLarge,
        BallistaError::InvalidPdaDerivation,
        BallistaError::MissingReturnData,
        BallistaError::ReturnDataMismatch,
        BallistaError::AccountConstraintFailed,
    );
    let context: u16 = nondet();
    let ProgramError::Custom(code) = vm_error(kind, context) else {
        cvlr_assert!(false);
        return;
    };
    let (decoded_kind, decoded_context) = decode_error(code);
    clog!(code, decoded_kind, decoded_context);
    cvlr_assert!(decoded_kind == kind.code());
    cvlr_assert!(decoded_context == context);
    cvlr_assert!(decoded_kind >= RUNTIME_ERROR_BASE);
    cvlr_assert!(decoded_kind < RUNTIME_ERROR_BASE + RUNTIME_ERROR_NAMES.len() as u32);
    // Runtime codes never reach the verifier range, so a decoder can tell them apart.
    cvlr_assert!(decoded_kind < VERIFIER_ERROR_BASE);
}

#[rule]
pub fn rule_verifier_error_codes_are_distinct_and_in_range() {
    let index: usize = nondet();
    cvlr_assume!(index < VERIFIER_ERROR_NAMES.len());
    let error = match index {
        0 => TemplateError::Truncated,
        1 => TemplateError::PayloadTooLarge(nondet()),
        2 => TemplateError::InvalidMagic,
        3 => TemplateError::UnsupportedVersion(nondet()),
        4 => TemplateError::InvalidReservedBytes,
        5 => TemplateError::SectionLengthMismatch,
        6 => TemplateError::CountOverflow,
        7 => TemplateError::TooManyAccounts,
        8 => TemplateError::TooManyInputs,
        9 => TemplateError::TooManyRegisters,
        10 => TemplateError::TooManyInstructions,
        11 => TemplateError::InvalidBatch,
        12 => TemplateError::InvalidAccountConstraint(nondet()),
        13 => TemplateError::InvalidInput(nondet()),
        14 => TemplateError::InvalidInstruction(nondet()),
        15 => TemplateError::InvalidCpi(nondet()),
        16 => TemplateError::InvalidDataSegment(nondet()),
        17 => TemplateError::InvalidRegister(nondet()),
        18 => TemplateError::RegisterNotInitialized(nondet()),
        19 => TemplateError::TypeMismatch,
        20 => TemplateError::InvalidBlobRange,
        21 => TemplateError::ExcessiveCpiExpansion,
        22 => TemplateError::InvalidFlags(nondet()),
        23 => TemplateError::InvalidCarry(nondet()),
        24 => TemplateError::ReadOutOfBounds(nondet()),
        25 => TemplateError::TooManyCpiAccounts(nondet()),
        26 => TemplateError::InvalidReturnData(nondet()),
        27 => TemplateError::InvalidMinIterations,
        _ => TemplateError::TooManyAccountGroups,
    };
    let (code, _) = error.code();
    cvlr_assert!(code == VERIFIER_ERROR_BASE + index as u32);
    cvlr_assert!(code < VERIFIER_ERROR_BASE + VERIFIER_ERROR_NAMES.len() as u32);
    // Verifier codes never reach down into the runtime range.
    cvlr_assert!(code >= RUNTIME_ERROR_BASE + RUNTIME_ERROR_NAMES.len() as u32);
}
