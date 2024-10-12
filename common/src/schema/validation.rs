use borsh::{BorshDeserialize, BorshSerialize};

use crate::task::shared::TaskAccount;

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Validation {
    IsTokenAccount(TaskAccount),
    IsEmpty(TaskAccount),
    // Add other validation types as needed
}
