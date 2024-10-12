use borsh::{BorshDeserialize, BorshSerialize};

use crate::task::shared::TaskAccount;

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AssociatedTokenProgramInstruction {
    InitializeAccount {
        payer: TaskAccount,
        token_account: TaskAccount,
        owner: TaskAccount,
        mint: TaskAccount,
        token_program_id: TaskAccount,
        system_program_id: TaskAccount,
    },
}
