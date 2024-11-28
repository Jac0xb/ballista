use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::logical_components::Expression;
use crate::types::task::task_account::TaskAccount;

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TokenProgramVersion {
    Legacy,
    Token2022,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TokenProgramInstruction {
    Transfer {
        program_version: TokenProgramVersion,
        from: TaskAccount,
        from_token_account: TaskAccount,
        to_token_account: TaskAccount,
        multisig: Option<Vec<TaskAccount>>,
        amount: Expression,
    },
    InitializeAccount {
        program_version: TokenProgramVersion,
        account: TaskAccount,
        owner: TaskAccount,
        mint: TaskAccount,
    },
}
