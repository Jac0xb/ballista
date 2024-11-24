use ballista_common::types::logical_components::Condition;
use ballista_common::types::execution_state::ExecutionState;

use crate::error::BallistaError;

use super::{evaluate_expression, evaluate_validation};

pub fn evaluate_condition(
    condition: &Condition,
    execution_state: &ExecutionState,
) -> Result<bool, BallistaError> {
    match condition {
        Condition::Equal(left, right) => {
            Ok(evaluate_expression(left, execution_state)? == evaluate_expression(right, execution_state)?)
        }
        Condition::NotEqual(left, right) => {
            Ok(evaluate_expression(left, execution_state)? != evaluate_expression(right, execution_state)?)
        }
        Condition::GreaterThan(left, right) => {
            Ok(evaluate_expression(left, execution_state)? > evaluate_expression(right, execution_state)?)
        }
        Condition::GreaterThanOrEqual(left, right) => {
            Ok(evaluate_expression(left, execution_state)? >= evaluate_expression(right, execution_state)?)
        }
        Condition::LessThan(left, right) => {
            Ok(evaluate_expression(left, execution_state)? < evaluate_expression(right, execution_state)?)
        }
        Condition::LessThanOrEqual(left, right) => {
            Ok(evaluate_expression(left, execution_state)? <= evaluate_expression(right, execution_state)?)
        }
        Condition::And(left, right) => {
            Ok(evaluate_condition(left, execution_state)? && evaluate_condition(right, execution_state)?)
        }
        Condition::Or(left, right) => {
            Ok(evaluate_condition(left, execution_state)? || evaluate_condition(right, execution_state)?)
        }
        Condition::Validation(validation) => Ok(evaluate_validation(validation, execution_state)?),
    }
}
