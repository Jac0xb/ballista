use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{self, evaluate_condition, evaluate_expression},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value,
    schema::TaskDefinition,
    task::{
        action::set_cache::SetCacheType, action_context::ActionContext, task_action::TaskAction,
    },
};
use pinocchio::account_info::AccountInfo;
use solana_program::msg;

pub fn process(
    task_definition: &TaskDefinition,
    input_values: &[Value],
    payer: &AccountInfo,
    remaining_accounts: &[AccountInfo],
) -> Result<(), BallistaError> {
    let mut task_state = TaskState {
        definition: task_definition,
        inputs: input_values,
        all_accounts: remaining_accounts,
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
        // instruction_data_cache: &mut Vec::with_capacity(512),
        payer,
    };

    process_action(
        &mut task_state,
        &mut ActionContext {
            current_index: 0,
            depth: 0,
            actions: &task_definition.actions.as_slice(),
        },
        &mut Vec::with_capacity(
            task_definition
                .execution_settings
                .preallocated_instruction_data_cache_size
                .map(|size| size as usize)
                .unwrap_or(512),
        ),
    )?;

    Ok(())
}

fn process_action(
    task_state: &mut TaskState,
    action_context: &mut ActionContext<'_>,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    let len = action_context.actions_len();

    while action_context.get_index() < len as u8 {
        let action = &action_context.get_current_action();

        debug_msg!("Processing action");

        match action {
            TaskAction::SystemInstruction(program_ix) => {
                evaluate::instructions::system_program::evaluate(
                    program_ix,
                    task_state,
                    instruction_data_cache,
                )
                .unwrap();
                task_state.purge_instruction_cache();
            }
            TaskAction::TokenProgramInstruction(program_ix) => {
                evaluate::instructions::token_program::evaluate(program_ix, task_state).unwrap();
                // task_state.purge_instruction_cache();

                // panic!("Token program not implemented");
            }
            TaskAction::AssociatedTokenProgramInstruction(program_ix) => {
                evaluate::instructions::associated_token_program::evaluate(program_ix, task_state)
                    .unwrap();
                // task_state.purge_instruction_cache();

                // panic!("Associated token program not implemented");
            }
            TaskAction::DefinedInstruction(defined_instruction) => {
                evaluate::instructions::defined::evaluate(
                    defined_instruction,
                    task_state,
                    instruction_data_cache,
                )
                .unwrap();

                task_state.purge_instruction_cache();
                instruction_data_cache.clear();
            }
            TaskAction::Loop { condition, actions } => {
                if evaluate_condition(condition, task_state)? {
                    let action_subcontext = &mut ActionContext {
                        current_index: 0,
                        depth: action_context.depth + 1,
                        actions: &actions.as_slice(),
                    };

                    while evaluate_condition(condition, task_state)? {
                        process_action(task_state, action_subcontext, instruction_data_cache)?;

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
                if evaluate_condition(condition, task_state)? {
                    process_action(
                        task_state,
                        &mut ActionContext {
                            current_index: 0,
                            depth: action_context.depth + 1,
                            actions: &[true_action].as_slice(),
                        },
                        instruction_data_cache,
                    )?;
                }
            }
            TaskAction::RawInstruction(_raw_instruction) => {
                evaluate::instructions::raw::evaluate(_raw_instruction, task_state).unwrap();
                task_state.purge_instruction_cache();
            }
            // TaskAction::Raw(_raw_instruction_group) => {
            //     panic!("Raw instruction groups not implemented");
            // }
            TaskAction::SetCache(set_cache) => match set_cache {
                SetCacheType::AccountData { .. } => {
                    panic!("Account data not implemented");
                }
                SetCacheType::Expression { expression, index } => {
                    let value = evaluate_expression(expression, task_state)?;

                    // TODO: CLONE
                    let owned_value = value.into_owned();

                    if (task_state.cached_values.len() as u8) <= *index {
                        // TODO: The default value needs to be null or something  or this should behave differnet
                        task_state
                            .cached_values
                            .resize(*index as usize + 1, Value::U8(0));
                    }
                    task_state.cached_values[*index as usize] = owned_value;
                }
            },
            // TaskAction::Validate(_validation) => {
            //     panic!("Validation not implemented");
            // }
            TaskAction::Log(expression) => {
                let value = evaluate_expression(expression, task_state)?;
                msg!("LOG: {:?}", value);
            }
        }

        action_context.increment(1);
    }

    Ok(())
}
