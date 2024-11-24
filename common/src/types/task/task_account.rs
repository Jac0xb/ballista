use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::logical_components::Expression;

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaskAccount {
    FeePayer,
    FromInput(u8),
    FromInputWithSeed { index: u8, seed: Option<u32> },
    FromGroup { group_index: u8, account_index: u8 },
    Evaluated(Expression),
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaskAccounts {
    FromInput {
        start: u8,
        length: u8,
    },
    Evaluated {
        start: Expression,
        length: Expression,
    },
}

impl TaskAccount {
    pub fn evaluated(expression: &Expression) -> Self {
        Self::Evaluated(expression.clone())
    }
}
