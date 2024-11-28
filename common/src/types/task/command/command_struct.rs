use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

use super::associated_token_program_instruction::AssociatedTokenProgramInstruction;
use super::defined_instruction::DefinedInstruction;
use super::raw_instruction::RawInstruction;
use super::set_cache::SetCacheType;
use super::system_instruction::SystemInstruction;
use super::token_program_instruction::TokenProgramInstruction;

use crate::types::logical_components::{Condition, Expression};

#[derive(Clone, Debug, Eq, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Command {
    InvokeRawInstruction(RawInstruction),
    InvokeDefinedInstruction(DefinedInstruction),
    InvokeSystemProgram(SystemInstruction),
    InvokeAssociatedTokenProgram(AssociatedTokenProgramInstruction),
    InvokeTokenProgram(TokenProgramInstruction),
    SetCache(SetCacheType),
    Conditional {
        condition: Condition,
        true_action: Box<Command>,
    },
    Loop {
        condition: Condition,
        actions: Vec<Box<Command>>,
    },
    Log(Expression)
}

impl Command {
    pub fn loop_action(condition: Condition, actions: Vec<Command>) -> Command {
        Command::Loop {
            condition,
            actions: actions.into_iter().map(Box::new).collect(),
        }
    }
}
