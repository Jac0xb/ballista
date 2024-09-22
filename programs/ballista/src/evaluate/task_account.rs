use super::evaluate_expression;
use crate::task_state::TaskState;
use ballista_common::{logical_components::Value, task::action::schema_instruction::TaskAccount};
use solana_program::account_info::AccountInfo;

pub fn evaluate_program_task_account<'info, 'a>(
    program: &TaskAccount,
    task_state: &TaskState<'info, 'a>,
) -> Result<&'a AccountInfo<'info>, String> {
    match program {
        TaskAccount::FromInput(index) => task_state
            .all_accounts
            .get(*index as usize)
            .ok_or_else(|| format!("Input account at index {} not found", index)),
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, task_state)?;

            let index = match account_value {
                Value::U8(value) => value as usize,
                _ => return Err("Program account index must be a u8".to_string()),
            };

            task_state
                .all_accounts
                .get(index)
                .ok_or_else(|| format!("Input account at index {} not found", index))
        }
        TaskAccount::MultipleInput { .. } => {
            Err("Multiple input not implemented for program account".to_string())
        }
        TaskAccount::MultipleEvaluated { .. } => {
            Err("Multiple evaluated not implemented for program account".to_string())
        }
    }
}

pub fn evaluate_task_account<'info, 'a>(
    account: &TaskAccount,
    task_state: &TaskState<'info, 'a>,
) -> Result<&'a AccountInfo<'info>, String> {
    match account {
        TaskAccount::FromInput(index) => task_state
            .all_accounts
            .get(*index as usize)
            .ok_or_else(|| format!("Input account at index {} not found", index)),
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, task_state)?;

            let index = match account_value {
                Value::U8(value) => value as usize,
                _ => return Err("Program account index must be a u8".to_string()),
            };

            task_state
                .all_accounts
                .get(index)
                .ok_or_else(|| format!("Input account at index {} not found", index))
        }
        TaskAccount::MultipleInput { .. } => {
            Err("Multiple input not implemented for task account".to_string())
        }
        TaskAccount::MultipleEvaluated { .. } => {
            Err("Multiple evaluated not implemented for task account".to_string())
        }
    }
}

pub fn evaluate_task_accounts<'info, 'a, 'b>(
    account: &TaskAccount,
    task_state: &TaskState<'info, 'a>,
    instruction_accounts: &'b mut Vec<&'a AccountInfo<'info>>,
) -> Result<(), String>
where
    'a: 'b,
{
    match account {
        TaskAccount::FromInput(index) => {
            let account = task_state
                .all_accounts
                .get(*index as usize)
                .ok_or_else(|| format!("Input account at index {} not found", index))?;

            instruction_accounts.push(account);
        }
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, task_state)?;

            let index = match account_value {
                Value::U8(value) => value as usize,
                _ => return Err("Program account index must be a u8".to_string()),
            };

            let account = task_state
                .all_accounts
                .get(index)
                .ok_or_else(|| format!("Input account at index {} not found", index))?;

            instruction_accounts.push(account);
        }
        TaskAccount::MultipleInput { start, length } => {
            let range = *start as usize..(*start + *length) as usize;

            for i in range {
                let account = task_state
                    .all_accounts
                    .get(i)
                    .ok_or_else(|| format!("Input account at index {} not found", i))?;

                instruction_accounts.push(account);
            }
        }
        TaskAccount::MultipleEvaluated { start, length } => {
            let evaluated_start = evaluate_expression(start, task_state)?;
            let evaluated_length = evaluate_expression(length, task_state)?;

            let start = match evaluated_start {
                Value::U8(value) => value as usize,
                _ => return Err("Start index must be a u8".to_string()),
            };

            let length = match evaluated_length {
                Value::U8(value) => value as usize,
                _ => return Err("Length must be a u8".to_string()),
            };

            for i in 0..length {
                let index = start + i;

                let account = task_state
                    .all_accounts
                    .get(index)
                    .ok_or_else(|| format!("Input account at index {} not found", index))?;

                instruction_accounts.push(account);
            }
        }
    };

    Ok(())
}
