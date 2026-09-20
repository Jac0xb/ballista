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
}

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
    fn runtime_codes_start_at_6000_and_stay_below_the_verifier_range() {
        assert_eq!(BallistaError::InvalidInstructionData.code(), 6000);
        assert!(BallistaError::ReturnDataMismatch.code() < VERIFIER_ERROR_BASE);
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
