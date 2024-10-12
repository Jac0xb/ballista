use borsh::{BorshDeserialize, BorshSerialize};

use crate::{logical_components::Expression, task::shared::TaskAccount};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SystemInstructionAction {
    Transfer {
        from: TaskAccount,
        to: TaskAccount,
        amount: Expression,
    },
    CreateAccount {
        payer: TaskAccount,
        account: TaskAccount,
        program_owner: TaskAccount,
        space: Expression,
        lamports: Expression,
    },
    Allocate,
    AllocateWithSeed,
    AssignWithSeed,
    AdvanceNonceAccount,
    WithdrawNonceAccount,
    InitializeNonceAccount,
    InitializeNonceAccountWithSeed,
}
