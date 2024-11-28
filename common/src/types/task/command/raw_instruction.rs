use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::logical_components::Expression;
use crate::types::task::task_account::{TaskAccount, TaskAccounts};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawInstruction {
    pub program: TaskAccount,
    pub data: Expression,
    pub accounts: TaskAccounts,
}
