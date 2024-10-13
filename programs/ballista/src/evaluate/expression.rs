use std::{any::type_name, borrow::Cow};

use ballista_common::logical_components::{
    AccountInfoType, ArithmeticBehavior, Expression, Value, ValueType,
};
use borsh::BorshDeserialize;
use pinocchio::{
    account_info::AccountInfo,
    msg,
    sysvars::{rent::Rent, Sysvar},
};
// use solana_program::{account_info::AccountInfo, msg, rent::Rent, sysvar::Sysvar};

use crate::{debug_msg, error::BallistaError, task_state::TaskState};

use super::evaluate_task_account;

pub fn evaluate_expression<'info, 'a, 'b>(
    expression: &'b Expression,
    task_state: &'a TaskState<'a>,
) -> Result<Cow<'a, Value>, BallistaError>
where
    'b: 'a,
{
    match expression {
        Expression::Literal(v) => debug_msg!("evaluating literal"),
        Expression::InputValue(index) => debug_msg!("evaluating input value"),
        Expression::StaticValue(index) => debug_msg!("evaluating static value"),
        Expression::CachedValue(index) => debug_msg!("evaluating cached value"),
        Expression::ValueFromAccountData {
            index,
            offset,
            value_type,
        } => debug_msg!("evaluating value from account data"),
        Expression::ValueFromAccountInfo { index, field_name } => {
            debug_msg!("evaluating value from account info");
        }
        Expression::SafeCast(inner_expr, target_type) => {
            debug_msg!("evaluating safe cast");
        }
        Expression::Multiply(left, right, behavior) => {
            debug_msg!("evaluating multiply");
        }
        Expression::Divide(left, right, behavior) => {
            debug_msg!("evaluating divide");
        }
        Expression::Add(left, right, behavior) => {
            debug_msg!("evaluating add");
        }
        Expression::Subtract(left, right, behavior) => {
            debug_msg!("evaluating subtract");
        }
        // Expression::Conditional(condition, true_expr, false_expr) => {
        //     debug_msg!("evaluating conditional");
        // }
        Expression::Rent(space_expr) => {
            debug_msg!("evaluating rent");
        }
    }

    match expression {
        Expression::Literal(v) => Ok(Cow::Borrowed(v)),
        Expression::InputValue(index) => {
            let value = task_state
                .inputs
                .get(*index as usize)
                .ok_or(BallistaError::InputValueNotFound)
                .unwrap();

            Ok(Cow::Borrowed(value))
        }
        Expression::StaticValue(index) => {
            let value = task_state
                .definition
                .shared_values
                .get(*index as usize)
                .ok_or(BallistaError::StaticValueNotFound)
                .unwrap();

            Ok(Cow::Borrowed(value))
        }
        Expression::CachedValue(index) => {
            let value = task_state
                .cached_values
                .get(*index as usize)
                .ok_or(BallistaError::CachedValueNotFound)
                .unwrap();

            Ok(Cow::Borrowed(value))
        }
        Expression::ValueFromAccountData {
            index,
            offset,
            value_type,
        } => {
            // let index_value = evaluate_expression(index, task_state)?;
            let account = evaluate_task_account(index, task_state)?;

            let offset_value = evaluate_expression(offset, task_state)?;

            let result = match value_type {
                ValueType::U8 => Value::U8(account.get_u8(offset_value.as_u128() as usize)?),
                ValueType::U16 => Value::U16(account.get_u16(offset_value.as_u128() as usize)?),
                ValueType::U32 => Value::U32(account.get_u32(offset_value.as_u128() as usize)?),
                ValueType::U64 => Value::U64(account.get_u64(offset_value.as_u128() as usize)?),
                ValueType::U128 => Value::U128(account.get_u128(offset_value.as_u128() as usize)?),
                ValueType::I8 => Value::I8(account.get_i8(offset_value.as_u128() as usize)?),
                ValueType::I16 => Value::I16(account.get_i16(offset_value.as_u128() as usize)?),
                ValueType::I32 => Value::I32(account.get_i32(offset_value.as_u128() as usize)?),
                ValueType::I64 => Value::I64(account.get_i64(offset_value.as_u128() as usize)?),
                ValueType::I128 => Value::I128(account.get_i128(offset_value.as_u128() as usize)?),
                _ => todo!(),
            };

            Ok(Cow::Owned(result))
        }
        Expression::ValueFromAccountInfo { index, field_name } => {
            let account = evaluate_task_account(index, task_state)?;
            let field_value = match field_name {
                // AccountInfoType::Data => account.data,
                AccountInfoType::Key => Value::Pubkey(*account.key()),
                AccountInfoType::Owner => Value::Pubkey(*account.owner()),
                // AccountInfoType::Rent => account.rent,
                _ => todo!(),
            };

            Ok(Cow::Owned(field_value))
        }
        Expression::SafeCast(inner_expr, target_type) => {
            // Default shouldnt matter
            let inner_value = evaluate_expression(inner_expr, task_state)?;

            let value = inner_value
                .safe_cast(target_type.clone())
                .map_err(|_| BallistaError::InvalidCast)?;

            Ok(Cow::Owned(value))
        }
        Expression::Multiply(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;

            let result = match behavior {
                ArithmeticBehavior::Checked => left_value
                    .checked_mul(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Saturating => left_value
                    .saturating_mul(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Wrapping => left_value
                    .wrapping_mul(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
            }?;

            Ok(Cow::Owned(result))
        }
        Expression::Divide(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;

            let result = match behavior {
                ArithmeticBehavior::Checked => left_value
                    .checked_div(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Saturating => left_value
                    .saturating_div(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Wrapping => left_value
                    .wrapping_div(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
            }?;

            Ok(Cow::Owned(result))
        }
        Expression::Add(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;

            let result = match behavior {
                ArithmeticBehavior::Checked => left_value
                    .checked_add(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Saturating => left_value
                    .saturating_add(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Wrapping => left_value
                    .wrapping_add(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
            }?;

            Ok(Cow::Owned(result))
        }
        Expression::Subtract(left, right, behavior) => {
            let left_value = evaluate_expression(left, task_state)?;
            let right_value = evaluate_expression(right, task_state)?;

            let result = match behavior {
                ArithmeticBehavior::Checked => left_value
                    .checked_sub(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Saturating => left_value
                    .saturating_sub(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
                ArithmeticBehavior::Wrapping => left_value
                    .wrapping_sub(&right_value)
                    .map_err(|_| BallistaError::ArithmeticOverflow),
            }?;

            Ok(Cow::Owned(result))
        }
        // Expression::Conditional(condition, true_expr, false_expr) => {
        //     let condition_value = evaluate_condition(condition, task_state)?;
        //     if condition_value {
        //         evaluate_expression(true_expr, task_state, value_output)
        //     } else {
        //         evaluate_expression(false_expr, task_state, value_output)
        //     }
        // }
        Expression::Rent(space_expr) => {
            let space = evaluate_expression(space_expr, task_state)?;

            Ok(Cow::Owned(Value::U64(
                Rent::get()
                    .unwrap()
                    .minimum_balance(space.as_u128() as usize),
            )))
        }
    }
}

trait BorshDeserializeValues {
    fn get_u8(&self, offset: usize) -> Result<u8, BallistaError>;
    fn get_u16(&self, offset: usize) -> Result<u16, BallistaError>;
    fn get_u32(&self, offset: usize) -> Result<u32, BallistaError>;
    fn get_u64(&self, offset: usize) -> Result<u64, BallistaError>;
    fn get_u128(&self, offset: usize) -> Result<u128, BallistaError>;
    fn get_i8(&self, offset: usize) -> Result<i8, BallistaError>;
    fn get_i16(&self, offset: usize) -> Result<i16, BallistaError>;
    fn get_i32(&self, offset: usize) -> Result<i32, BallistaError>;
    fn get_i64(&self, offset: usize) -> Result<i64, BallistaError>;
    fn get_i128(&self, offset: usize) -> Result<i128, BallistaError>;
}

impl BorshDeserializeValues for AccountInfo {
    fn get_u8(&self, offset: usize) -> Result<u8, BallistaError> {
        try_from_slice::<u8>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_u16(&self, offset: usize) -> Result<u16, BallistaError> {
        try_from_slice::<u16>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_u32(&self, offset: usize) -> Result<u32, BallistaError> {
        try_from_slice::<u32>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_u64(&self, offset: usize) -> Result<u64, BallistaError> {
        try_from_slice::<u64>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_u128(&self, offset: usize) -> Result<u128, BallistaError> {
        try_from_slice::<u128>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_i8(&self, offset: usize) -> Result<i8, BallistaError> {
        try_from_slice::<i8>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_i16(&self, offset: usize) -> Result<i16, BallistaError> {
        try_from_slice::<i16>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_i32(&self, offset: usize) -> Result<i32, BallistaError> {
        try_from_slice::<i32>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_i64(&self, offset: usize) -> Result<i64, BallistaError> {
        try_from_slice::<i64>(&self.try_borrow_data().unwrap(), offset)
    }

    fn get_i128(&self, offset: usize) -> Result<i128, BallistaError> {
        try_from_slice::<i128>(&self.try_borrow_data().unwrap(), offset)
    }
}

#[inline(always)]
pub fn try_from_slice<T: BorshDeserialize + Sized>(
    data: &[u8],
    offset: usize,
) -> Result<T, BallistaError> {
    let start = offset;
    let end = offset + std::mem::size_of::<T>();

    let slice = data.get(start..end).ok_or_else(|| {
        msg!(
            "Failed to deserialized {} range {:?} was out of bounds",
            type_name::<T>(),
            start..end
        );

        BallistaError::RangeOutOfBounds
    })?;

    Ok(T::try_from_slice(slice).unwrap())
}
