use crate::{
    logical_components::{Condition, Expression},
    schema::validation::Validation,
};
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

use super::action::{
    associated_token_program_instruction::AssociatedTokenProgramInstruction,
    raw_instruction::RawInstruction,
    schema_instruction::{SchemaInstruction, TaskAccount},
    set_cache::SetCacheType,
    system_instruction::SystemInstructionAction,
    token_program_instruction::TokenProgramInstruction,
};

#[derive(Clone, Debug, Eq, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaskAction {
    SchemaInstruction(SchemaInstruction),
    SystemInstruction(SystemInstructionAction),
    AssociatedTokenProgramInstruction(AssociatedTokenProgramInstruction),
    TokenProgramInstruction(TokenProgramInstruction),
    RawInstruction(RawInstruction),
    RawInstructionGroup(Vec<RawInstruction>),
    SetCache(SetCacheType),
    Validate(Validation),
    Conditional {
        condition: Condition,
        true_action: Box<TaskAction>,
    },
    Loop {
        condition: Condition,
        actions: Vec<Box<TaskAction>>,
    },
}

impl TaskAction {
    pub fn schema_instruction(instruction: SchemaInstruction) -> TaskAction {
        TaskAction::SchemaInstruction(instruction)
    }

    pub fn system_instruction(instruction: SystemInstructionAction) -> TaskAction {
        TaskAction::SystemInstruction(instruction)
    }

    pub fn system_instruction_transfer(
        from: TaskAccount,
        to: TaskAccount,
        amount: Expression,
    ) -> TaskAction {
        TaskAction::SystemInstruction(SystemInstructionAction::Transfer { from, to, amount })
    }

    pub fn raw_instruction(instruction: RawInstruction) -> TaskAction {
        TaskAction::RawInstruction(instruction)
    }

    pub fn raw_instruction_group(instructions: Vec<RawInstruction>) -> TaskAction {
        TaskAction::RawInstructionGroup(instructions)
    }

    pub fn set_cache(cache: SetCacheType) -> TaskAction {
        TaskAction::SetCache(cache)
    }

    pub fn loop_action(condition: Condition, actions: Vec<TaskAction>) -> TaskAction {
        TaskAction::Loop {
            condition,
            actions: actions.into_iter().map(Box::new).collect(),
        }
    }
}
