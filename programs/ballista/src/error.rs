use solana_program::program_error::ProgramError;
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
}

impl From<BallistaError> for ProgramError {
    fn from(e: BallistaError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
