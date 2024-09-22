use borsh::{BorshDeserialize, BorshSerialize};

use crate::logical_components::Expression;

use super::schema_instruction::TaskAccount;

// #[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
// #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
// pub enum TokenProgramVersion {
//     Legacy,
//     Token2022,
// }

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AssociatedTokenProgramInstruction {
    InitializeAccount {
        // program_version: TokenProgramVersion,
        // account: TaskAccount,
        // owner: TaskAccount,
        // mint: TaskAccount,
        payer: TaskAccount,
        token_account: TaskAccount,
        owner: TaskAccount,
        mint: TaskAccount,
        token_program_id: TaskAccount,
        system_program_id: TaskAccount,
    },
}

// build_associated_token_account_instruction
