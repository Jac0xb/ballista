use super::evaluate_expression;
use crate::{error::BallistaError, task_state::TaskState};
use ballista_common::{logical_components::Value, task::shared::TaskAccount};
use pinocchio::account_info::AccountInfo;

pub fn evaluate_program_task_account<'a>(
    program: &TaskAccount,
    task_state: &TaskState<'a>,
) -> Result<&'a AccountInfo, BallistaError> {
    match program {
        TaskAccount::FeePayer => todo!(),
        TaskAccount::FromInput(index) => task_state
            .all_accounts
            .get(*index as usize)
            .ok_or(BallistaError::AccountNotFound),
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, task_state)?;

            let index = match account_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            task_state
                .all_accounts
                .get(index)
                .ok_or(BallistaError::AccountNotFound)
        }
        TaskAccount::FromGroup {
            group_index,
            account_index,
        } => {
            let group = task_state
                .definition
                .account_groups
                .get(*group_index as usize)
                // TODO: Better error handling
                .ok_or(BallistaError::TaskNotFound)?;

            let offset = evaluate_expression(&group.account_offset, task_state)?.as_u128();

            let account = task_state
                .all_accounts
                .get((offset + *account_index as u128) as usize)
                .ok_or(BallistaError::AccountNotFound)?;

            Ok(account)
        }
    }
}

pub fn evaluate_task_account<'info, 'a>(
    account: &TaskAccount,
    task_state: &TaskState<'a>,
) -> Result<&'a AccountInfo, BallistaError> {
    match account {
        TaskAccount::FeePayer => Ok(task_state.payer),
        TaskAccount::FromInput(index) => task_state
            .all_accounts
            .get(*index as usize)
            .ok_or(BallistaError::InputValueNotFound),
        TaskAccount::Evaluated(expression) => {
            let account_value = evaluate_expression(expression, task_state)?;

            let index = match account_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            task_state
                .all_accounts
                .get(index)
                .ok_or(BallistaError::InputValueNotFound)
        }
        TaskAccount::FromGroup {
            group_index,
            account_index,
        } => {
            let group = task_state
                .definition
                .account_groups
                .get(*group_index as usize)
                .ok_or(BallistaError::TaskNotFound)?;

            let offset = evaluate_expression(&group.account_offset, task_state)?.as_u128();

            let account = task_state
                .all_accounts
                .get((offset + *account_index as u128) as usize)
                .ok_or(BallistaError::AccountNotFound)?;

            Ok(account)
        } //
          //
          // TaskAccount::MultipleInput { start, length } => {
          //     let range = *start as usize..(*start + *length) as usize;

          // }
          // TaskAccount::MultipleEvaluated { .. } => {
          //     // Err("Multiple evaluated not implemented for task account".to_string())

          //     panic!("unimplemented")
          // }
    }
}

// pub fn evaluate_task_accounts<'info, 'a>(
//     account: &TaskAccount,
//     task_state: &mut TaskState<'_, '_>,
//     // instruction_accounts: &'_ mut CacheVec<AccountInfo<'info>>,
// ) -> Result<&'a AccountInfo<'info>, BallistaError> {
//     debug_msg!("evaluate task accounts");

//     match account {
//         TaskAccount::FromInput(index) => {
//             // debug_msg!("evaluating from input 1");

//             let account = task_state
//                 .all_accounts
//                 .get(*index as usize)
//                 .ok_or(BallistaError::EvaluationError)?;

//             // debug_msg!("evaluated from input 2");

//             // let account = account.clone();

//             // debug_msg!("evaluated from input 3");

//             // task_state.account_info_cache.push(account.clone());

//             // debug_msg!("evaluated from input 4");

//             Ok(account)
//         }
//         TaskAccount::Evaluated(expression) => {
//             let account_value = evaluate_expression(expression, task_state)?;

//             let index = match account_value {
//                 Value::U8(value) => value as usize,
//                 _ => return Err(BallistaError::InvalidCast),
//             };

//             let account = task_state
//                 .all_accounts
//                 .get(index)
//                 .ok_or(BallistaError::EvaluationError)?;

//             task_state.account_info_cache.push(account.clone());
//         }
//         TaskAccount::MultipleInput { start, length } => {
//             let range = *start as usize..(*start + *length) as usize;

//             for i in range {
//                 let account = task_state
//                     .all_accounts
//                     .get(i)
//                     .ok_or(BallistaError::EvaluationError)?;

//                 task_state.account_info_cache.push(account.clone());
//             }
//         }
//         TaskAccount::MultipleEvaluated { start, length } => {
//             let evaluated_start = evaluate_expression(start, task_state)?;
//             let evaluated_length = evaluate_expression(length, task_state)?;

//             let start = match evaluated_start {
//                 Value::U8(value) => value as usize,
//                 _ => return Err(BallistaError::EvaluationError),
//             };

//             let length = match evaluated_length {
//                 Value::U8(value) => value as usize,
//                 _ => return Err(BallistaError::EvaluationError),
//             };

//             for i in 0..length {
//                 let index = start + i;

//                 let account = task_state
//                     .all_accounts
//                     .get(index)
//                     .ok_or(BallistaError::EvaluationError)?;

//                 task_state.account_info_cache.push(account.clone());
//             }
//         }
//     };

//     Ok(())
// }
