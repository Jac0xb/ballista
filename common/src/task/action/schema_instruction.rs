use borsh::{BorshDeserialize, BorshSerialize};

use crate::logical_components::{Expression, Value};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchemaInstruction {
    pub instruction_id: u32,
    pub program: TaskAccount,
    pub accounts: Vec<TaskAccount>,
    pub arguments: Vec<TaskArgument>,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaskAccount {
    FromInput(u8),
    // EvaluatedInput(Expression),
    Evaluated(Expression),
    MultipleInput {
        start: u8,
        length: u8,
    },
    MultipleEvaluated {
        start: Expression,
        length: Expression,
    },
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaskArgument {
    Expression(Expression),
    Literal(Value),
}
