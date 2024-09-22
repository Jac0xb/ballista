use crate::schema::Validation;

use super::Expression;
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

#[derive(Clone, Debug, Eq, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Condition {
    Equal(Expression, Expression),
    NotEqual(Expression, Expression),
    GreaterThan(Expression, Expression),
    GreaterThanOrEqual(Expression, Expression),
    LessThan(Expression, Expression),
    LessThanOrEqual(Expression, Expression),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Validation(Validation),
}

impl Condition {
    pub fn less_than(left: Expression, right: Expression) -> Condition {
        Condition::LessThan(left, right)
    }

    pub fn less_than_or_equal(left: Expression, right: Expression) -> Condition {
        Condition::LessThanOrEqual(left, right)
    }

    pub fn greater_than(left: Expression, right: Expression) -> Condition {
        Condition::GreaterThan(left, right)
    }

    pub fn greater_than_or_equal(left: Expression, right: Expression) -> Condition {
        Condition::GreaterThanOrEqual(left, right)
    }

    pub fn equal(left: Expression, right: Expression) -> Condition {
        Condition::Equal(left, right)
    }

    pub fn not_equal(left: Expression, right: Expression) -> Condition {
        Condition::NotEqual(left, right)
    }

    pub fn and(left: Condition, right: Condition) -> Condition {
        Condition::And(Box::new(left), Box::new(right))
    }

    pub fn or(left: Condition, right: Condition) -> Condition {
        Condition::Or(Box::new(left), Box::new(right))
    }
}
