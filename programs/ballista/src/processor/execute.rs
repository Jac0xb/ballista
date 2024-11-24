use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{self, evaluate_condition, evaluate_expression},
};
use ballista_common::types::execution_state::ExecutionState;
use ballista_common::types::{
    logical_components::Value,
    task::{action::set_cache::SetCacheType, task_action::TaskAction},
};
use ballista_common::{
    accounts::task_definition::TaskDefinition, types::execution_frame::ExecutionFrame,
};
use pinocchio::{account_info::AccountInfo, msg};

/// 
/// Creates an execution state and creates an execution frame for the task
/// using the actions defined at the top level of the task definition.
/// 
pub fn execute(
    task_definition: &TaskDefinition,
    input_values: &[Value],
    payer: &AccountInfo,
    input_accounts: &[AccountInfo],
) -> Result<(), BallistaError> {
    let mut execution_state = ExecutionState {
        task_definition,
        payer,
        input_values,
        input_accounts,
        cached_values: Vec::with_capacity(8),
        account_meta_cache: Vec::with_capacity(
            task_definition
                .execution_settings
                .preallocated_account_meta_cache_size
                .map(|size| size as usize)
                .unwrap_or(remaining_accounts.len() * 2),
        ),
        account_info_cache: Vec::with_capacity(
            task_definition
                .execution_settings
                .preallocated_account_info_cache_size
                .map(|size| size as usize)
                .unwrap_or(remaining_accounts.len() * 2),
        ),
    };

    create_execution_frame(
        &mut execution_state,
        &mut ExecutionFrame {
            current_index: 0,
            depth: 0,
            actions: &task_definition.actions.as_slice(),
        },
        &mut Vec::with_capacity(
            task_definition
                .execution_settings
                .preallocated_instruction_data_cache_size
                .map(|size| size as usize)
                .unwrap_or(0),
        ),
    )?;

    Ok(())
}

fn create_execution_frame(
    execution_state: &mut ExecutionState,
    execution_frame: &mut ExecutionFrame<'_>,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    let len = execution_frame.actions_len();

    while execution_frame.get_index() < len as u8 {
        let action = &execution_frame.get_current_action();

        debug_msg!("Processing action");

        match action {
            TaskAction::SystemInstruction(program_ix) => {
                evaluate::instructions::system_program::evaluate(program_ix, execution_state)
                    .unwrap();
                execution_state.purge_caches();
            }
            TaskAction::TokenProgramInstruction(program_ix) => {
                evaluate::instructions::token_program::evaluate(program_ix, execution_state)
                    .unwrap();
                execution_state.purge_caches();
            }
            TaskAction::AssociatedTokenProgramInstruction(program_ix) => {
                evaluate::instructions::associated_token_program::evaluate(
                    program_ix,
                    execution_state,
                )
                .unwrap();
                execution_state.purge_caches();
            }
            TaskAction::DefinedInstruction(defined_instruction) => {
                evaluate::instructions::defined::evaluate(
                    defined_instruction,
                    execution_state,
                    instruction_data_cache,
                )
                .unwrap();

                execution_state.purge_caches();
                instruction_data_cache.clear();
            }
            TaskAction::Loop { condition, actions } => {
                if evaluate_condition(condition, execution_state)? {
                    let action_subcontext = &mut ExecutionFrame {
                        current_index: 0,
                        depth: execution_frame.depth + 1,
                        actions: &actions.as_slice(),
                    };

                    while evaluate_condition(condition, execution_state)? {
                        create_execution_frame(
                            execution_state,
                            action_subcontext,
                            instruction_data_cache,
                        )?;

                        // Reset the action subcontext (WHY THO?)
                        // Oh because it loops in the inner subcontext
                        action_subcontext.set_index(0);
                    }
                }
            }
            TaskAction::Conditional {
                condition,
                true_action,
            } => {
                if evaluate_condition(condition, execution_state)? {
                    create_execution_frame(
                        execution_state,
                        &mut ExecutionFrame {
                            current_index: 0,
                            depth: execution_frame.depth + 1,
                            actions: true_action,
                        },
                        instruction_data_cache,
                    )?;
                }
            }
            TaskAction::RawInstruction(_raw_instruction) => {
                evaluate::instructions::raw::evaluate(_raw_instruction, execution_state).unwrap();
                execution_state.purge_caches();
            }
            TaskAction::SetCache(set_cache) => match set_cache {
                SetCacheType::AccountData { .. } => {
                    panic!("Account data not implemented");
                }
                SetCacheType::Expression { expression, index } => {
                    let value = evaluate_expression(expression, execution_state)?;

                    // TODO: CLONE
                    let owned_value = value.into_owned();

                    if (execution_state.cached_values.len() as u8) <= *index {
                        // TODO: The default value needs to be null or something  or this should behave differnet
                        execution_state
                            .cached_values
                            .resize(*index as usize + 1, Value::U8(0));
                    }
                    execution_state.cached_values[*index as usize] = owned_value;
                }
            },
            // TaskAction::Validate(_validation) => {
            //     panic!("Validation not implemented");
            // }
            TaskAction::Log(expression) => {
                let value = evaluate_expression(expression, execution_state)?;
                msg!("LOG: {:?}", value);
            }
        }

        execution_frame.increment(1);
    }

    Ok(())
}
