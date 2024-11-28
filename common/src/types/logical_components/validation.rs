use borsh::{BorshDeserialize, BorshSerialize};
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

use crate::types::task::task_account::TaskAccount;

#[derive(Clone, Debug, BorshDeserializeBoxed, BorshSerializeBoxed, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Validation {
    IsTokenAccount(TaskAccount),
    IsEmpty(TaskAccount),
}
