use crate::logical_components::{Value, ValueType};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Copy, Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum SerializationType {
    Borsh,
    Bytemuck,
    C,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccountDefinition {
    pub name: String,
    pub writable: bool,
    pub signer: bool,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InstructionDefinition {
    pub serialization: SerializationType,
    pub arguments: Vec<ArgumentDefinition>,
    pub accounts: Vec<AccountDefinition>,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ArgumentDefinition {
    Constant { value: Value },
    Input { value_type: Option<ValueType> },
}
