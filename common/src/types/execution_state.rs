use pinocchio::{
    account_info::AccountInfo,
    instruction::{Account, AccountMeta},
};

use crate::accounts::task_definition::TaskDefinition;

use super::logical_components::Value;

/// 
/// The state of the execution of a task.
/// Used to reference task definition, 
pub struct ExecutionState<'a> {
    // The definition of the task that is being executed
    pub task_definition: &'a TaskDefinition,
    // The payer of the task.
    pub payer: &'a AccountInfo,
    // The inputs provided via the instruction data.
    pub input_values: &'a [Value],
    // All the remaining accounts passed to the task after the task definition / payer accounts.
    pub input_accounts: &'a [AccountInfo],
    // A register of values that can be used during the execution of the task.
    pub cached_values: Vec<Value>,
    // A cache of the account metas that can be used during the execution of the task.
    // Reused to avoid reallocating account metas to heap for every call.
    pub account_meta_cache: Vec<AccountMeta<'a>>,
    // A cache of the account infos that can be used during the execution of the task.
    // Reused to avoid reallocating account infos to heap for every call.
    pub account_info_cache: Vec<Account<'a>>,
}

impl<'a> ExecutionState<'a> {
    pub fn purge_caches(&mut self) {
        self.account_meta_cache.clear();
        self.account_info_cache.clear();
    }
}
