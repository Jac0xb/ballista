use pinocchio::program_error::ProgramError;
use thiserror::Error;

#[macro_export]
macro_rules! err {
    ($error:expr) => {
        solana_program::program_error::ProgramError::from($error)
    };
}

#[macro_export]
macro_rules! err_msg {
    ($msg:expr, $error:expr) => {
        // Print the message and error
        solana_program::msg!("{}: {:?}", $msg, $error);
    };
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BallistaError {
    // Processor errors
    #[error("Invalid instruction")]
    InvalidInstructionData = 6000,

    #[error("Invalid schema data")]
    InvalidSchemaData = 6001,

    #[error("Failed to get account from all accounts")]
    EvaluationError = 6002,

    #[error("Static value not found")]
    StaticValueNotFound = 6003,

    #[error("Cached value not found")]
    CachedValueNotFound,

    #[error("Input value not found")]
    InputValueNotFound,

    #[error("Invalid cast")]
    InvalidCast,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Invalid token account")]
    InvalidTokenAccount,

    #[error("Account not found")]
    AccountNotFound,

    #[error("Task not found")]
    TaskNotFound,

    #[error("Instruction schema not found")]
    InstructionSchemaNotFound,

    #[error("Account empty")]
    AccountEmpty,

    #[error("Invalid account range")]
    InvalidAccountRange,

    #[error("Range out of bounds")]
    RangeOutOfBounds,

    #[error("TODO")]
    Todo,

    #[error("Invalid PDA")]
    InvalidPDA,
}

impl From<BallistaError> for ProgramError {
    fn from(e: BallistaError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
