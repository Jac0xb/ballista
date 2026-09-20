//! Error codes round-trip through their encoding and partition cleanly into runtime, verifier,
//! and foreign codes.

use ballista_common::template::*;
use cvlr::prelude::*;

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
pub fn rule_decoded_errors_partition_by_range() {
    let code: u32 = nondet();
    let (kind, context) = decode_error(code);
    match decode_ballista_error(code) {
        Some(decoded) => {
            cvlr_assert!(decoded.code == code);
            cvlr_assert!(decoded.kind == kind);
            cvlr_assert!(decoded.context == context);
            match decoded.source {
                ErrorSource::Runtime => {
                    cvlr_assert!(kind >= RUNTIME_ERROR_BASE);
                    cvlr_assert!(kind < RUNTIME_ERROR_BASE + RUNTIME_ERROR_NAMES.len() as u32);
                    cvlr_assert!(
                        decoded.name == RUNTIME_ERROR_NAMES[(kind - RUNTIME_ERROR_BASE) as usize]
                    );
                }
                ErrorSource::Verifier => {
                    cvlr_assert!(kind >= VERIFIER_ERROR_BASE);
                    cvlr_assert!(kind < VERIFIER_ERROR_BASE + VERIFIER_ERROR_NAMES.len() as u32);
                    cvlr_assert!(
                        decoded.name == VERIFIER_ERROR_NAMES[(kind - VERIFIER_ERROR_BASE) as usize]
                    );
                }
            }
        }
        None => {
            let runtime = kind >= RUNTIME_ERROR_BASE
                && kind < RUNTIME_ERROR_BASE + RUNTIME_ERROR_NAMES.len() as u32;
            let verifier = kind >= VERIFIER_ERROR_BASE
                && kind < VERIFIER_ERROR_BASE + VERIFIER_ERROR_NAMES.len() as u32;
            cvlr_assert!(!runtime && !verifier);
        }
    }
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
        _ => TemplateError::InvalidMinIterations,
    };
    let (code, _) = error.code();
    cvlr_assert!(code == VERIFIER_ERROR_BASE + index as u32);
    cvlr_assert!(code < VERIFIER_ERROR_BASE + VERIFIER_ERROR_NAMES.len() as u32);
}
