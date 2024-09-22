use ballista_common::logical_components::Condition;

use crate::task_state::TaskState;

use super::{evaluate_expression, evaluate_validation};

pub fn evaluate_condition(condition: &Condition, task_state: &TaskState) -> Result<bool, String> {
    match condition {
        Condition::Equal(left, right) => {
            Ok(evaluate_expression(left, task_state)? == evaluate_expression(right, task_state)?)
        }
        Condition::NotEqual(left, right) => {
            Ok(evaluate_expression(left, task_state)? != evaluate_expression(right, task_state)?)
        }
        Condition::GreaterThan(left, right) => {
            Ok(evaluate_expression(left, task_state)? > evaluate_expression(right, task_state)?)
        }
        Condition::GreaterThanOrEqual(left, right) => {
            Ok(evaluate_expression(left, task_state)? >= evaluate_expression(right, task_state)?)
        }
        Condition::LessThan(left, right) => {
            Ok(evaluate_expression(left, task_state)? < evaluate_expression(right, task_state)?)
        }
        Condition::LessThanOrEqual(left, right) => {
            Ok(evaluate_expression(left, task_state)? <= evaluate_expression(right, task_state)?)
        }
        Condition::And(left, right) => {
            Ok(evaluate_condition(left, task_state)? && evaluate_condition(right, task_state)?)
        }
        Condition::Or(left, right) => {
            Ok(evaluate_condition(left, task_state)? || evaluate_condition(right, task_state)?)
        }
        Condition::Validation(validation) => Ok(evaluate_validation(validation, task_state)?),
    }
}
