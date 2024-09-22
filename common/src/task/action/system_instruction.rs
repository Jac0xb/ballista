use borsh::{BorshDeserialize, BorshSerialize};

use crate::logical_components::Expression;

use super::schema_instruction::TaskAccount;

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

// impl SystemInstruction {

// }

// {
//     pub instruction_id: u32,
//     pub program: TaskAccount,
//     pub accounts: Vec<TaskAccount>,
//     pub arguments: Vec<TaskArgument>,
// }
