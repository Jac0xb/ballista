use geppetto::{account, IntoPrimitive, TryFromPrimitive};

use crate::types::logical_components::{Expression, Value};
use crate::types::task::task_action::TaskAction;

use borsh::{BorshDeserialize, BorshSerialize};

//
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

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum BallistaAccount {
    TaskDefinition = 0,
}

account!(BallistaAccount, TaskDefinition);