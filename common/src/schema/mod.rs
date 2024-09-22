pub mod instruction;
pub mod validation;

pub use instruction::*;
pub use validation::*;

use crate::{logical_components::Value, task::task_action::TaskAction};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TaskDefinition {
    pub actions: Vec<TaskAction>,
    pub static_values: Vec<Value>,
}

// Stuff for instructions
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Schema {
    pub instructions: Vec<InstructionDefinition>,
    pub tasks: Vec<TaskDefinition>,
}
