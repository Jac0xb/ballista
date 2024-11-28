use ballista_common::types::execution_state::ExecutionState;
use ballista_common::types::logical_components::Validation;

use crate::error::BallistaError;

use super::evaluate_task_account;

pub fn evaluate_validation(
    validation: &Validation,
    execution_state: &ExecutionState,
) -> Result<bool, BallistaError> {
    match validation {
        Validation::IsTokenAccount(account) => {
            let (account_info, _) = evaluate_task_account(account, execution_state)?;

            if account_info.owner() != &pinocchio_token::ID {
                return Err(BallistaError::InvalidTokenAccount);
            }

            Ok(true)
        }
        Validation::IsEmpty(account) => {
            // panic!("unimplemented")
            let (account_info, _) = evaluate_task_account(account, execution_state)?;

            Ok(account_info.data_is_empty())
        }
    }
}
