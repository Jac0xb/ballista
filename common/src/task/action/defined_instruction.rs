use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    logical_components::{Expression, Value},
    task::shared::TaskAccount,
};

#[derive(Copy, Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum SerializationType {
    Borsh,
    Bytemuck,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DefinedInstruction {
    pub serialization_type: SerializationType,
    pub program: TaskAccount,
    pub accounts: Vec<DefinedAccount>,
    pub arguments: Vec<DefinedArgument>,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DefinedAccount {
    pub writable: bool,
    pub signer: bool,
    pub task_account: TaskAccount,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DefinedArgument {
    pub value: Expression,
}
