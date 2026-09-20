use ballista_common::template::{encode_error, TemplateError};
use pinocchio::error::ProgramError;
use thiserror::Error;

/// Runtime error kinds. The on-chain code is the discriminant, optionally combined with a 16-bit
/// context (program counter, account index, or input index) in the high bits via [`vm_error`].
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BallistaError {
    #[error("invalid instruction data")]
    InvalidInstructionData = 6000,
    #[error("invalid template account")]
    InvalidTemplateAccount,
    #[error("invalid template program")]
    InvalidTemplateProgram,
    #[error("template is not uploading")]
    TemplateNotUploading,
    #[error("template is not finalized")]
    TemplateNotFinalized,
    #[error("invalid template creator")]
    InvalidCreator,
    #[error("invalid chunk offset")]
    InvalidChunkOffset,
    #[error("template hash mismatch")]
    HashMismatch,
    #[error("invalid run inputs")]
    InvalidRunInputs,
    #[error("invalid runtime account")]
    InvalidRuntimeAccount,
    #[error("invalid account range")]
    InvalidAccountRange,
    #[error("invalid register")]
    InvalidRegister,
    #[error("register type mismatch")]
    TypeMismatch,
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    #[error("division by zero")]
    DivisionByZero,
    #[error("template requirement failed")]
    RequirementFailed,
    #[error("CPI data exceeds its declared bound")]
    CpiDataTooLarge,
    #[error("PDA derivation failed")]
    InvalidPdaDerivation,
    #[error("return data is missing")]
    MissingReturnData,
    #[error("return data came from a different program")]
    ReturnDataMismatch,
    /// A runtime account failed its declared constraint before execution; the context is the
    /// account index. `InvalidRuntimeAccount` is reserved for failures during execution.
    #[error("runtime account does not satisfy its constraint")]
    AccountConstraintFailed,
}

/// Runtime error names in code order, shared with the SDK through `fixtures/runtime-error-names.txt`.
pub const RUNTIME_ERROR_NAMES: [&str; 21] = [
    "InvalidInstructionData",
    "InvalidTemplateAccount",
    "InvalidTemplateProgram",
    "TemplateNotUploading",
    "TemplateNotFinalized",
    "InvalidCreator",
    "InvalidChunkOffset",
    "HashMismatch",
    "InvalidRunInputs",
    "InvalidRuntimeAccount",
    "InvalidAccountRange",
    "InvalidRegister",
    "TypeMismatch",
    "ArithmeticOverflow",
    "DivisionByZero",
    "RequirementFailed",
    "CpiDataTooLarge",
    "InvalidPdaDerivation",
    "MissingReturnData",
    "ReturnDataMismatch",
    "AccountConstraintFailed",
];

impl BallistaError {
    /// The bare error code without context bits.
    pub const fn code(self) -> u32 {
        self as u32
    }
}

impl From<BallistaError> for ProgramError {
    fn from(error: BallistaError) -> Self {
        ProgramError::Custom(error.code())
    }
}

/// A runtime failure with its location encoded in the high 16 bits of the custom code.
pub fn vm_error(kind: BallistaError, context: u16) -> ProgramError {
    ProgramError::Custom(encode_error(kind.code(), context))
}

/// Verifier failures keep their own code range so callers can tell them from runtime failures.
pub fn verifier_error(error: TemplateError) -> ProgramError {
    let (code, context) = error.code();
    ProgramError::Custom(encode_error(code, context))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ballista_common::template::{decode_error, VERIFIER_ERROR_BASE};

    #[test]
    fn runtime_error_names_match_the_shared_fixture_in_code_order() {
        let variants = [
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
        ];
        for (index, variant) in variants.iter().enumerate() {
            assert_eq!(variant.code(), 6000 + index as u32, "{variant:?}");
            assert_eq!(format!("{variant:?}"), RUNTIME_ERROR_NAMES[index]);
        }
        let shared: Vec<&str> = include_str!("../../../fixtures/runtime-error-names.txt")
            .lines()
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(shared, RUNTIME_ERROR_NAMES.to_vec());

        let verifier: Vec<&str> = include_str!("../../../fixtures/verifier-error-names.txt")
            .lines()
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(verifier, ballista_common::template::VERIFIER_ERROR_NAMES.to_vec());
    }

    #[test]
    fn runtime_codes_start_at_6000_and_stay_below_the_verifier_range() {
        assert_eq!(BallistaError::InvalidInstructionData.code(), 6000);
        assert!(BallistaError::AccountConstraintFailed.code() < VERIFIER_ERROR_BASE);
        assert_eq!(
            vm_error(BallistaError::RequirementFailed, 7),
            ProgramError::Custom((7 << 16) | 6015)
        );
        assert_eq!(
            decode_error(6015 | (7 << 16)),
            (BallistaError::RequirementFailed.code(), 7)
        );
        assert_eq!(
            verifier_error(TemplateError::InvalidCpi(3)),
            ProgramError::Custom((3 << 16) | (VERIFIER_ERROR_BASE + 15))
        );
    }
}
