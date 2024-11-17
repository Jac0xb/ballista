use ballista_common::schema::Validation;

use crate::{error::BallistaError, task_state::TaskState};

use super::evaluate_task_account;

pub fn evaluate_validation(
    validation: &Validation,
    task_state: &TaskState,
) -> Result<bool, BallistaError> {
    match validation {
        Validation::IsTokenAccount(account) => {
            let (account_info, _) = evaluate_task_account(account, task_state)?;

            if account_info.owner() != &spl_token::ID.to_bytes()
                && account_info.owner() != &spl_token_2022::ID.to_bytes()
            {
                return Err(BallistaError::InvalidTokenAccount);
            }

            Ok(true)
        }
        Validation::IsEmpty(account) => {
            // panic!("unimplemented")
            let (account_info, _) = evaluate_task_account(account, task_state)?;

            Ok(account_info.data_is_empty())
        }
    }
}
