use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    logical_components::Expression,
    task::shared::{TaskAccount, TaskAccounts},
};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawInstruction {
    // Should expect this to always be a vec<u8>
    pub program: TaskAccount,
    pub data: Expression,
    pub accounts: TaskAccounts,
    // transforms, allow data to be transformed before being invoked
    // conditional transform
}
