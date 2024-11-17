use ballista_common::{logical_components::Value, schema::TaskDefinition};
use pinocchio::{
    account_info::AccountInfo,
    instruction::{Account, AccountMeta},
};

use crate::debug_msg;

pub struct TaskState<'a> {
    pub definition: &'a TaskDefinition,
    pub inputs: &'a [Value],
    pub cached_values: Vec<Value>,
    pub all_accounts: &'a [AccountInfo],
    pub account_meta_cache: Vec<AccountMeta<'a>>,
    pub account_info_cache: Vec<Account<'a>>,
    pub payer: &'a AccountInfo,
}

impl<'a> TaskState<'a> {
    pub fn purge_instruction_cache(&mut self) {
        debug_msg!("purging");
        self.account_meta_cache.clear();
        self.account_info_cache.clear();
        debug_msg!("purged");
    }
}
