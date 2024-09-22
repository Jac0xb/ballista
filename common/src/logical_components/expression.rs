use crate::task::task_action::TaskAction;

use super::{Condition, Value, ValueType};
use borsh::{BorshDeserialize, BorshSerialize};
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ArithmeticBehavior {
    Wrapping,
    Saturating,
    Checked,
}

#[derive(Clone, Debug, Eq, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Expression {
    Literal(Value),
    InputValue(u8),
    StaticValue(u8),
    CachedValue(u8),
    SafeCast(Box<Expression>, ValueType),
    Multiply(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Divide(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Add(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Subtract(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Conditional(Box<Condition>, Box<Expression>, Box<Expression>),
    Rent(Box<Expression>),
}

impl Expression {
    pub fn literal(value: Value) -> Self {
        Expression::Literal(value)
    }

    pub fn input(index: u8) -> Self {
        Expression::InputValue(index)
    }

    pub fn static_value(index: u8) -> Self {
        Expression::StaticValue(index)
    }

    pub fn cached_value(index: u8) -> Self {
        Expression::CachedValue(index)
    }

    pub fn safe_cast(self, target_type: ValueType) -> Self {
        Expression::SafeCast(Box::new(self), target_type)
    }

    pub fn safe_cast_to_u64(self) -> Self {
        Expression::SafeCast(Box::new(self), ValueType::U64)
    }

    pub fn multiply(self, right: Expression, behavior: ArithmeticBehavior) -> Self {
        Expression::Multiply(Box::new(self), Box::new(right), behavior)
    }

    pub fn checked_multiply(self, right: Expression) -> Self {
        Expression::Multiply(Box::new(self), Box::new(right), ArithmeticBehavior::Checked)
    }

    pub fn divide(self, right: Expression, behavior: ArithmeticBehavior) -> Self {
        Expression::Divide(Box::new(self), Box::new(right), behavior)
    }

    pub fn add(self, right: Expression, behavior: ArithmeticBehavior) -> Self {
        Expression::Add(Box::new(self), Box::new(right), behavior)
    }

    pub fn checked_add(self, right: Expression) -> Self {
        Expression::Add(Box::new(self), Box::new(right), ArithmeticBehavior::Checked)
    }

    pub fn subtract(self, right: Expression, behavior: ArithmeticBehavior) -> Self {
        Expression::Subtract(Box::new(self), Box::new(right), behavior)
    }
}

impl From<Value> for Expression {
    fn from(value: Value) -> Self {
        Expression::Literal(value)
    }
}
