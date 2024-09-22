use ballista_common::logical_components::{ArithmeticBehavior, Expression, Value};
use solana_program::{rent::Rent, sysvar::Sysvar};

use crate::task_state::TaskState;

use super::evaluate_condition;

pub fn evaluate_expression(
    expression: &Expression,
    task_state: &TaskState,
) -> Result<Value, String> {
    let value = match expression {
        Expression::Literal(v) => Ok(v.clone()),
        Expression::InputValue(index) => task_state
            .inputs
            .get(*index as usize)
            .cloned()
            .ok_or_else(|| format!("Input at index {} not found", index)),
        Expression::StaticValue(index) => task_state
            .definition
            .static_values
            .get(*index as usize)
            .cloned()
            .ok_or_else(|| format!("Static value at index {} not found", index)),
        Expression::CachedValue(index) => task_state
            .cached_values
            .get(*index as usize)
            .cloned()
            .flatten()
            .ok_or_else(|| format!("Cached value at index {} not found", index)),
        Expression::SafeCast(inner_expr, target_type) => {
            let inner_value = evaluate_expression(inner_expr, task_state)?;
            inner_value.safe_cast(target_type.clone())
        }
        Expression::Multiply(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;
            match behavior {
                ArithmeticBehavior::Checked => left_value.checked_mul(&right_value),
                ArithmeticBehavior::Saturating => left_value.saturating_mul(&right_value),
                ArithmeticBehavior::Wrapping => left_value.wrapping_mul(&right_value),
            }
        }
        Expression::Divide(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;
            match behavior {
                ArithmeticBehavior::Checked => left_value.checked_div(&right_value),
                ArithmeticBehavior::Saturating => left_value.saturating_div(&right_value),
                ArithmeticBehavior::Wrapping => left_value.wrapping_div(&right_value),
            }
        }
        Expression::Add(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;
            match behavior {
                ArithmeticBehavior::Checked => left_value.checked_add(&right_value),
                ArithmeticBehavior::Saturating => left_value.saturating_add(&right_value),
                ArithmeticBehavior::Wrapping => left_value.wrapping_add(&right_value),
            }
        }
        Expression::Subtract(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;
            match behavior {
                ArithmeticBehavior::Checked => left_value.checked_sub(&right_value),
                ArithmeticBehavior::Saturating => left_value.saturating_sub(&right_value),
                ArithmeticBehavior::Wrapping => left_value.wrapping_sub(&right_value),
            }
        }
        Expression::Conditional(condition, true_expr, false_expr) => {
            let condition_value = evaluate_condition(condition, task_state)?;
            if condition_value {
                evaluate_expression(true_expr, task_state)
            } else {
                evaluate_expression(false_expr, task_state)
            }
        }
        Expression::Rent(space_expr) => {
            let space = evaluate_expression(space_expr, task_state)?;

            Ok(Value::U64(
                Rent::get()
                    .unwrap()
                    .minimum_balance(space.as_u128() as usize),
            ))
        }
    };
    value
}
