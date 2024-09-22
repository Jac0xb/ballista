use ballista_common::schema::Validation;

use crate::task_state::TaskState;

use super::evaluate_task_account;

pub fn evaluate_validation(
    validation: &Validation,
    task_state: &TaskState,
) -> Result<bool, String> {
    match validation {
        Validation::IsTokenAccount(account) => {
            let account_info = evaluate_task_account(account, task_state)?;

            if account_info.owner != &spl_token::ID && account_info.owner != &spl_token_2022::ID {
                return Err(format!(
                    "Invalid owner for account: {} {}",
                    account_info.key, account_info.owner
                ));
            }

            Ok(true)
        }
        Validation::IsEmpty(account) => {
            // panic!("unimplemented")
            let account_info = evaluate_task_account(account, task_state)?;

            Ok(account_info.try_data_is_empty().unwrap())
        }
    }
}
