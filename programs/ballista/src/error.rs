use pinocchio::error::ProgramError;
use thiserror::Error;

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
}

impl From<BallistaError> for ProgramError {
    fn from(error: BallistaError) -> Self {
        ProgramError::Custom(error as u32)
    }
}
