use super::evaluate_expression;
use crate::error::BallistaError;
use ballista_common::types::execution_state::ExecutionState;
use ballista_common::types::{logical_components::Value, task::task_account::TaskAccount};
use pinocchio::{account_info::AccountInfo, msg};

pub fn evaluate_program_task_account<'a>(
    program: &TaskAccount,
    execution_state: &ExecutionState<'a>,
) -> Result<&'a AccountInfo, BallistaError> {
    match program {
        TaskAccount::FeePayer => todo!(),
        TaskAccount::FromInput(index) => execution_state
            .input_accounts
            .get(*index as usize)
            .ok_or(BallistaError::AccountNotFound),
        TaskAccount::FromInputWithSeed { index, seed: _seed } => execution_state
            .input_accounts
            .get(*index as usize)
            .ok_or(BallistaError::AccountNotFound),
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, execution_state)?;

            let index = match account_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            execution_state
                .input_accounts
                .get(index)
                .ok_or(BallistaError::AccountNotFound)
        }
        TaskAccount::FromGroup {
            group_index,
            account_index,
        } => {
            let group = execution_state
                .definition
                .account_groups
                .get(*group_index as usize)
                // TODO: Better error handling
                .ok_or(BallistaError::TaskNotFound)?;

            let offset = evaluate_expression(&group.account_offset, execution_state)?.as_u128();

            let account = execution_state
                .input_accounts
                .get((offset + *account_index as u128) as usize)
                .ok_or(BallistaError::AccountNotFound)?;

            Ok(account)
        }
    }
}

pub fn evaluate_task_account<'info, 'a>(
    account: &TaskAccount,
    execution_state: &ExecutionState<'a>,
) -> Result<(&'a AccountInfo, Option<u32>), BallistaError> {
    match account {
        TaskAccount::FeePayer => Ok((execution_state.payer, None)),
        TaskAccount::FromInput(index) => {
            let account = execution_state
                .input_accounts
                .get(*index as usize)
                .ok_or_else(|| {
                    msg!("Account not found at index {}", index);

                    BallistaError::AccountNotFound
                })?;

            Ok((account, None))
        }
        TaskAccount::FromInputWithSeed { index, seed } => {
            let account = execution_state
                .input_accounts
                .get(*index as usize)
                .ok_or_else(|| {
                    msg!("Account not found at index {}", index);

                    BallistaError::AccountNotFound
                })?;

            Ok((account, *seed))
        }
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, execution_state)?;

            let index = match account_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            let account = execution_state.input_accounts.get(index).ok_or_else(|| {
                msg!("Account not found at index {}", index);

                BallistaError::AccountNotFound
            })?;

            Ok((account, None))
        }
        TaskAccount::FromGroup {
            group_index,
            account_index,
        } => {
            let group = execution_state
                .definition
                .account_groups
                .get(*group_index as usize)
                .ok_or(BallistaError::TaskNotFound)?;

            let offset = evaluate_expression(&group.account_offset, execution_state)?.as_u128();

            let account = execution_state
                .input_accounts
                .get((offset + *account_index as u128) as usize)
                .ok_or(BallistaError::AccountNotFound)?;

            Ok((account, None))
        }
    }
}
