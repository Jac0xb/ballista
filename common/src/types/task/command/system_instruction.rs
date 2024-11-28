use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::logical_components::Expression;
use crate::types::task::task_account::TaskAccount;

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SystemInstruction {
    CreateAccount {
        payer: TaskAccount,
        account: TaskAccount,
        program_owner: TaskAccount,
        space: Expression,
        lamports: Expression,
    },
    Transfer {
        from: TaskAccount,
        to: TaskAccount,
        amount: Expression,
    },
    Allocate,
    AllocateWithSeed,
    AssignWithSeed,
}
