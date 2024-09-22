use crate::{
    evaluate::{self, evaluate_condition, evaluate_expression},
    task_state::{CacheVec, TaskState},
};
use ballista_common::{
    logical_components::Value,
    schema::Schema,
    task::{
        action::set_cache::SetCacheType, action_context::ActionContext, task_action::TaskAction,
    },
};
use solana_program::account_info::AccountInfo;

pub fn process(
    schema: &Schema,
    task_id: u8,
    input_values: &[Value],
    remaining_accounts: &[AccountInfo<'_>],
) -> Result<(), String> {
    let task_definition = schema
        .tasks
        .get(task_id as usize)
        .ok_or_else(|| format!("Task at index {} not found", task_id))?;

    let mut task_state = TaskState {
        definition: task_definition,
        inputs: input_values,
        cached_values: Vec::with_capacity(8),
        all_accounts: remaining_accounts,
        account_meta_cache: CacheVec::new(remaining_accounts.len() * 2),
        account_info_cache: CacheVec::new(remaining_accounts.len() * 2),
        instruction_data_cache: Vec::with_capacity(256),
    };

    process_action(
        schema,
        &mut task_state,
        &mut ActionContext {
            current_index: 0,
            actions: &task_definition.actions.as_slice(),
        },
    )?;

    Ok(())
}

fn process_action(
    schema: &Schema,
    task_state: &mut TaskState,
    action_context: &mut ActionContext<'_>,
) -> Result<(), String> {
    let len = action_context.actions_len();

    while action_context.get_index() < len as u8 {
        let action = &action_context.get_current_action();

        match action {
            TaskAction::SystemInstruction(program_ix) => {
                evaluate::instructions::system_program::evaluate(program_ix, task_state)?;
                task_state.purge_instruction_cache();
            }
            TaskAction::TokenProgramInstruction(program_ix) => {
                evaluate::instructions::token_program::evaluate(program_ix, task_state)?;
                task_state.purge_instruction_cache();
            }
            TaskAction::AssociatedTokenProgramInstruction(program_ix) => {
                evaluate::instructions::associated_token_program::evaluate(program_ix, task_state)?;
                task_state.purge_instruction_cache();
            }
            TaskAction::SchemaInstruction(schema_instruction) => {
                evaluate::instructions::schema::evaluate(schema, schema_instruction, task_state)?;
                task_state.purge_instruction_cache();
            }
            TaskAction::Loop { condition, actions } => {
                if evaluate_condition(condition, task_state)? {
                    let action_subcontext = &mut ActionContext {
                        current_index: 0,
                        actions: &actions.as_slice(),
                    };

                    while evaluate_condition(condition, task_state)? {
                        process_action(schema, task_state, action_subcontext)?;

                        // Reset the action subcontext
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
                        schema,
                        task_state,
                        &mut ActionContext {
                            current_index: 0,
                            actions: &[true_action].as_slice(),
                        },
                    )?;
                }
            }
            TaskAction::RawInstruction(_raw_instruction) => {
                panic!("Raw instructions not implemented");
            }
            TaskAction::RawInstructionGroup(_raw_instruction_group) => {
                panic!("Raw instruction groups not implemented");
            }
            TaskAction::SetCache(set_cache) => match set_cache {
                SetCacheType::AccountData { .. } => {
                    panic!("Account data not implemented");
                }
                SetCacheType::Expression { expression, index } => {
                    let value = evaluate_expression(expression, task_state)?;

                    if (task_state.cached_values.len() as u8) <= *index {
                        task_state.cached_values.resize(*index as usize + 1, None);
                    }
                    task_state.cached_values[*index as usize] = Some(value);
                }
            },
            TaskAction::Validate(_validation) => {
                panic!("Validation not implemented");
            }
        }

        action_context.increment(1);
    }

    Ok(())
}
