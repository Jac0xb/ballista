use borsh::{BorshDeserialize, BorshSerialize};

use crate::logical_components::{Expression, ValueType};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SetCacheType {
    AccountData {
        account: u8,
        offset: u8,
        value_type: ValueType,
    },
    Expression {
        index: u8,
        expression: Expression,
    },
}
