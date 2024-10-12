pub mod instruction;
pub mod validation;

pub use instruction::*;
pub use validation::*;

use crate::{
    logical_components::{Expression, Value},
    task::task_action::TaskAction,
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TaskDefinition {
    pub execution_settings: ExecutionSettings,
    pub actions: Vec<TaskAction>,
    pub shared_values: Vec<Value>,
    pub account_groups: Vec<AccountGroupDefinition>,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionSettings {
    pub preallocated_instruction_data_cache_size: Option<u16>,
    pub preallocated_account_meta_cache_size: Option<u16>,
    pub preallocated_account_info_cache_size: Option<u16>,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccountGroupDefinition {
    pub account_offset: Expression,
    pub length: u8,
}
