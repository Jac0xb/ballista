use crate::types::logical_components::{Condition, Expression};
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

use super::{
    task_account::TaskAccount,
    action::{
        associated_token_program_instruction::AssociatedTokenProgramInstruction,
        defined_instruction::DefinedInstruction, raw_instruction::RawInstruction,
        set_cache::SetCacheType, system_instruction::SystemInstructionAction,
        token_program_instruction::TokenProgramInstruction,
    },
};

#[derive(Clone, Debug, Eq, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaskAction {
    DefinedInstruction(DefinedInstruction),
    SystemInstruction(SystemInstructionAction),
    AssociatedTokenProgramInstruction(AssociatedTokenProgramInstruction),
    TokenProgramInstruction(TokenProgramInstruction),
    RawInstruction(RawInstruction),
    SetCache(SetCacheType),
    // Assert(Validation),
    Conditional {
        condition: Condition,
        true_action: Box<TaskAction>,
    },
    Loop {
        condition: Condition,
        actions: Vec<Box<TaskAction>>,
    },
    Log(Expression),
}

impl TaskAction {
    pub fn defined_instruction(instruction: DefinedInstruction) -> TaskAction {
        TaskAction::DefinedInstruction(instruction)
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
