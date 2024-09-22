#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
pub mod schema {
    pub mod expression {
        pub mod condition {
            use crate::schema::task::task::TaskState;
            use borsh::{BorshDeserialize, BorshSerialize};
            use super::Expression;
            pub enum Condition {
                Equal(Expression, Expression),
                NotEqual(Expression, Expression),
                GreaterThan(Expression, Expression),
                GreaterThanOrEqual(Expression, Expression),
                LessThan(Expression, Expression),
                LessThanOrEqual(Expression, Expression),
                And(Box<Condition>, Box<Condition>),
                Or(Box<Condition>, Box<Condition>),
            }
            #[automatically_derived]
            impl ::core::clone::Clone for Condition {
                #[inline]
                fn clone(&self) -> Condition {
                    match self {
                        Condition::Equal(__self_0, __self_1) => {
                            Condition::Equal(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::NotEqual(__self_0, __self_1) => {
                            Condition::NotEqual(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::GreaterThan(__self_0, __self_1) => {
                            Condition::GreaterThan(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::GreaterThanOrEqual(__self_0, __self_1) => {
                            Condition::GreaterThanOrEqual(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::LessThan(__self_0, __self_1) => {
                            Condition::LessThan(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::LessThanOrEqual(__self_0, __self_1) => {
                            Condition::LessThanOrEqual(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::And(__self_0, __self_1) => {
                            Condition::And(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        Condition::Or(__self_0, __self_1) => {
                            Condition::Or(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for Condition {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        Condition::Equal(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "Equal",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::NotEqual(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "NotEqual",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::GreaterThan(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "GreaterThan",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::GreaterThanOrEqual(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "GreaterThanOrEqual",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::LessThan(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "LessThan",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::LessThanOrEqual(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "LessThanOrEqual",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::And(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "And",
                                __self_0,
                                &__self_1,
                            )
                        }
                        Condition::Or(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "Or",
                                __self_0,
                                &__self_1,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for Condition {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<Expression>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Condition>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Condition>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Condition>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Condition>>;
                }
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for Condition {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for Condition {
                #[inline]
                fn eq(&self, other: &Condition) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                Condition::Equal(__self_0, __self_1),
                                Condition::Equal(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::NotEqual(__self_0, __self_1),
                                Condition::NotEqual(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::GreaterThan(__self_0, __self_1),
                                Condition::GreaterThan(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::GreaterThanOrEqual(__self_0, __self_1),
                                Condition::GreaterThanOrEqual(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::LessThan(__self_0, __self_1),
                                Condition::LessThan(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::LessThanOrEqual(__self_0, __self_1),
                                Condition::LessThanOrEqual(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::And(__self_0, __self_1),
                                Condition::And(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                Condition::Or(__self_0, __self_1),
                                Condition::Or(__arg1_0, __arg1_1),
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl Condition {
                pub fn evaluate(&self, task_state: &TaskState) -> Result<bool, String> {
                    match self {
                        Condition::Equal(left, right) => {
                            let left_value = Expression::evaluate(left, task_state)?;
                            let right_value = Expression::evaluate(right, task_state)?;
                            Ok(left_value == right_value)
                        }
                        Condition::NotEqual(left, right) => {
                            let left_value = Expression::evaluate(left, task_state)?;
                            let right_value = Expression::evaluate(right, task_state)?;
                            Ok(left_value != right_value)
                        }
                        Condition::GreaterThan(left, right) => {
                            let left_value = Expression::evaluate(left, task_state)?;
                            let right_value = Expression::evaluate(right, task_state)?;
                            Ok(left_value > right_value)
                        }
                        Condition::GreaterThanOrEqual(left, right) => {
                            let left_value = Expression::evaluate(left, task_state)?;
                            let right_value = Expression::evaluate(right, task_state)?;
                            Ok(left_value >= right_value)
                        }
                        Condition::LessThan(left, right) => {
                            let left_value = Expression::evaluate(left, task_state)?;
                            let right_value = Expression::evaluate(right, task_state)?;
                            Ok(left_value < right_value)
                        }
                        Condition::LessThanOrEqual(left, right) => {
                            let left_value = Expression::evaluate(left, task_state)?;
                            let right_value = Expression::evaluate(right, task_state)?;
                            Ok(left_value <= right_value)
                        }
                        Condition::And(left, right) => {
                            let left_value = left.evaluate(task_state)?;
                            let right_value = right.evaluate(task_state)?;
                            Ok(left_value && right_value)
                        }
                        Condition::Or(left, right) => {
                            let left_value = left.evaluate(task_state)?;
                            let right_value = right.evaluate(task_state)?;
                            Ok(left_value || right_value)
                        }
                    }
                }
            }
            impl BorshDeserialize for Condition {
                fn deserialize_reader<R: std::io::Read>(
                    reader: &mut R,
                ) -> std::io::Result<Self> {
                    let tag = u8::deserialize_reader(reader)?;
                    match tag {
                        0 => {
                            Ok(
                                Condition::Equal(
                                    Expression::deserialize_reader(reader)?,
                                    Expression::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        1 => {
                            Ok(
                                Condition::NotEqual(
                                    Expression::deserialize_reader(reader)?,
                                    Expression::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        2 => {
                            Ok(
                                Condition::GreaterThan(
                                    Expression::deserialize_reader(reader)?,
                                    Expression::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        3 => {
                            Ok(
                                Condition::GreaterThanOrEqual(
                                    Expression::deserialize_reader(reader)?,
                                    Expression::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        4 => {
                            Ok(
                                Condition::LessThan(
                                    Expression::deserialize_reader(reader)?,
                                    Expression::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        5 => {
                            Ok(
                                Condition::LessThanOrEqual(
                                    Expression::deserialize_reader(reader)?,
                                    Expression::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        6 => {
                            Ok(
                                Condition::And(
                                    Box::new(Condition::deserialize_reader(reader)?),
                                    Box::new(Condition::deserialize_reader(reader)?),
                                ),
                            )
                        }
                        7 => {
                            Ok(
                                Condition::Or(
                                    Box::new(Condition::deserialize_reader(reader)?),
                                    Box::new(Condition::deserialize_reader(reader)?),
                                ),
                            )
                        }
                        _ => {
                            Err(
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "Invalid tag",
                                ),
                            )
                        }
                    }
                }
            }
            impl BorshSerialize for Condition {
                fn serialize<W: std::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> std::io::Result<()> {
                    match self {
                        Condition::Equal(left, right) => {
                            0u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::NotEqual(left, right) => {
                            1u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::GreaterThan(left, right) => {
                            2u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::GreaterThanOrEqual(left, right) => {
                            3u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::LessThan(left, right) => {
                            4u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::LessThanOrEqual(left, right) => {
                            5u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::And(left, right) => {
                            6u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                        Condition::Or(left, right) => {
                            7u8.serialize(writer)?;
                            left.serialize(writer)?;
                            right.serialize(writer)
                        }
                    }
                }
            }
        }
        use borsh::{BorshDeserialize, BorshSerialize};
        use condition::Condition;
        use std::{
            io::{Read, Write},
            mem,
        };
        use crate::schema::{task::task::TaskState, value::{Value, ValueType}};
        use borsh_enum_derive::BorshDeserializeBoxed;
        pub enum ArithmeticBehavior {
            Wrapping,
            Saturating,
            Checked,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for ArithmeticBehavior {
            #[inline]
            fn clone(&self) -> ArithmeticBehavior {
                match self {
                    ArithmeticBehavior::Wrapping => ArithmeticBehavior::Wrapping,
                    ArithmeticBehavior::Saturating => ArithmeticBehavior::Saturating,
                    ArithmeticBehavior::Checked => ArithmeticBehavior::Checked,
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ArithmeticBehavior {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        ArithmeticBehavior::Wrapping => "Wrapping",
                        ArithmeticBehavior::Saturating => "Saturating",
                        ArithmeticBehavior::Checked => "Checked",
                    },
                )
            }
        }
        impl borsh::de::BorshDeserialize for ArithmeticBehavior {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                    reader,
                )?;
                <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
            }
        }
        impl borsh::de::EnumExt for ArithmeticBehavior {
            fn deserialize_variant<R: borsh::maybestd::io::Read>(
                reader: &mut R,
                variant_idx: u8,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let mut return_value = match variant_idx {
                    0u8 => ArithmeticBehavior::Wrapping,
                    1u8 => ArithmeticBehavior::Saturating,
                    2u8 => ArithmeticBehavior::Checked,
                    _ => {
                        return Err(
                            borsh::maybestd::io::Error::new(
                                borsh::maybestd::io::ErrorKind::InvalidInput,
                                {
                                    let res = ::alloc::fmt::format(
                                        format_args!("Unexpected variant index: {0:?}", variant_idx),
                                    );
                                    res
                                },
                            ),
                        );
                    }
                };
                Ok(return_value)
            }
        }
        impl borsh::ser::BorshSerialize for ArithmeticBehavior {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                let variant_idx: u8 = match self {
                    ArithmeticBehavior::Wrapping => 0u8,
                    ArithmeticBehavior::Saturating => 1u8,
                    ArithmeticBehavior::Checked => 2u8,
                };
                writer.write_all(&variant_idx.to_le_bytes())?;
                match self {
                    ArithmeticBehavior::Wrapping => {}
                    ArithmeticBehavior::Saturating => {}
                    ArithmeticBehavior::Checked => {}
                }
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for ArithmeticBehavior {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {}
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for ArithmeticBehavior {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for ArithmeticBehavior {
            #[inline]
            fn eq(&self, other: &ArithmeticBehavior) -> bool {
                let __self_tag = ::core::intrinsics::discriminant_value(self);
                let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                __self_tag == __arg1_tag
            }
        }
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
        }
        #[automatically_derived]
        impl ::core::clone::Clone for Expression {
            #[inline]
            fn clone(&self) -> Expression {
                match self {
                    Expression::Literal(__self_0) => {
                        Expression::Literal(::core::clone::Clone::clone(__self_0))
                    }
                    Expression::InputValue(__self_0) => {
                        Expression::InputValue(::core::clone::Clone::clone(__self_0))
                    }
                    Expression::StaticValue(__self_0) => {
                        Expression::StaticValue(::core::clone::Clone::clone(__self_0))
                    }
                    Expression::CachedValue(__self_0) => {
                        Expression::CachedValue(::core::clone::Clone::clone(__self_0))
                    }
                    Expression::SafeCast(__self_0, __self_1) => {
                        Expression::SafeCast(
                            ::core::clone::Clone::clone(__self_0),
                            ::core::clone::Clone::clone(__self_1),
                        )
                    }
                    Expression::Multiply(__self_0, __self_1, __self_2) => {
                        Expression::Multiply(
                            ::core::clone::Clone::clone(__self_0),
                            ::core::clone::Clone::clone(__self_1),
                            ::core::clone::Clone::clone(__self_2),
                        )
                    }
                    Expression::Divide(__self_0, __self_1, __self_2) => {
                        Expression::Divide(
                            ::core::clone::Clone::clone(__self_0),
                            ::core::clone::Clone::clone(__self_1),
                            ::core::clone::Clone::clone(__self_2),
                        )
                    }
                    Expression::Add(__self_0, __self_1, __self_2) => {
                        Expression::Add(
                            ::core::clone::Clone::clone(__self_0),
                            ::core::clone::Clone::clone(__self_1),
                            ::core::clone::Clone::clone(__self_2),
                        )
                    }
                    Expression::Subtract(__self_0, __self_1, __self_2) => {
                        Expression::Subtract(
                            ::core::clone::Clone::clone(__self_0),
                            ::core::clone::Clone::clone(__self_1),
                            ::core::clone::Clone::clone(__self_2),
                        )
                    }
                    Expression::Conditional(__self_0, __self_1, __self_2) => {
                        Expression::Conditional(
                            ::core::clone::Clone::clone(__self_0),
                            ::core::clone::Clone::clone(__self_1),
                            ::core::clone::Clone::clone(__self_2),
                        )
                    }
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for Expression {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match self {
                    Expression::Literal(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Literal",
                            &__self_0,
                        )
                    }
                    Expression::InputValue(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "InputValue",
                            &__self_0,
                        )
                    }
                    Expression::StaticValue(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "StaticValue",
                            &__self_0,
                        )
                    }
                    Expression::CachedValue(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "CachedValue",
                            &__self_0,
                        )
                    }
                    Expression::SafeCast(__self_0, __self_1) => {
                        ::core::fmt::Formatter::debug_tuple_field2_finish(
                            f,
                            "SafeCast",
                            __self_0,
                            &__self_1,
                        )
                    }
                    Expression::Multiply(__self_0, __self_1, __self_2) => {
                        ::core::fmt::Formatter::debug_tuple_field3_finish(
                            f,
                            "Multiply",
                            __self_0,
                            __self_1,
                            &__self_2,
                        )
                    }
                    Expression::Divide(__self_0, __self_1, __self_2) => {
                        ::core::fmt::Formatter::debug_tuple_field3_finish(
                            f,
                            "Divide",
                            __self_0,
                            __self_1,
                            &__self_2,
                        )
                    }
                    Expression::Add(__self_0, __self_1, __self_2) => {
                        ::core::fmt::Formatter::debug_tuple_field3_finish(
                            f,
                            "Add",
                            __self_0,
                            __self_1,
                            &__self_2,
                        )
                    }
                    Expression::Subtract(__self_0, __self_1, __self_2) => {
                        ::core::fmt::Formatter::debug_tuple_field3_finish(
                            f,
                            "Subtract",
                            __self_0,
                            __self_1,
                            &__self_2,
                        )
                    }
                    Expression::Conditional(__self_0, __self_1, __self_2) => {
                        ::core::fmt::Formatter::debug_tuple_field3_finish(
                            f,
                            "Conditional",
                            __self_0,
                            __self_1,
                            &__self_2,
                        )
                    }
                }
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for Expression {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<Value>;
                let _: ::core::cmp::AssertParamIsEq<u8>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<ValueType>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<ArithmeticBehavior>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Condition>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for Expression {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for Expression {
            #[inline]
            fn eq(&self, other: &Expression) -> bool {
                let __self_tag = ::core::intrinsics::discriminant_value(self);
                let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                __self_tag == __arg1_tag
                    && match (self, other) {
                        (
                            Expression::Literal(__self_0),
                            Expression::Literal(__arg1_0),
                        ) => *__self_0 == *__arg1_0,
                        (
                            Expression::InputValue(__self_0),
                            Expression::InputValue(__arg1_0),
                        ) => *__self_0 == *__arg1_0,
                        (
                            Expression::StaticValue(__self_0),
                            Expression::StaticValue(__arg1_0),
                        ) => *__self_0 == *__arg1_0,
                        (
                            Expression::CachedValue(__self_0),
                            Expression::CachedValue(__arg1_0),
                        ) => *__self_0 == *__arg1_0,
                        (
                            Expression::SafeCast(__self_0, __self_1),
                            Expression::SafeCast(__arg1_0, __arg1_1),
                        ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                        (
                            Expression::Multiply(__self_0, __self_1, __self_2),
                            Expression::Multiply(__arg1_0, __arg1_1, __arg1_2),
                        ) => {
                            *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                && *__self_2 == *__arg1_2
                        }
                        (
                            Expression::Divide(__self_0, __self_1, __self_2),
                            Expression::Divide(__arg1_0, __arg1_1, __arg1_2),
                        ) => {
                            *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                && *__self_2 == *__arg1_2
                        }
                        (
                            Expression::Add(__self_0, __self_1, __self_2),
                            Expression::Add(__arg1_0, __arg1_1, __arg1_2),
                        ) => {
                            *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                && *__self_2 == *__arg1_2
                        }
                        (
                            Expression::Subtract(__self_0, __self_1, __self_2),
                            Expression::Subtract(__arg1_0, __arg1_1, __arg1_2),
                        ) => {
                            *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                && *__self_2 == *__arg1_2
                        }
                        (
                            Expression::Conditional(__self_0, __self_1, __self_2),
                            Expression::Conditional(__arg1_0, __arg1_1, __arg1_2),
                        ) => {
                            *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                && *__self_2 == *__arg1_2
                        }
                        _ => unsafe { ::core::intrinsics::unreachable() }
                    }
            }
        }
        impl borsh::BorshDeserialize for Expression {
            fn deserialize_reader<R: std::io::Read>(
                reader: &mut R,
            ) -> std::io::Result<Self> {
                let tag = u8::deserialize_reader(reader)?;
                match tag {
                    0u8 => {
                        Ok(
                            Expression::Literal(
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    1u8 => {
                        Ok(
                            Expression::InputValue(
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    2u8 => {
                        Ok(
                            Expression::StaticValue(
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    3u8 => {
                        Ok(
                            Expression::CachedValue(
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    4u8 => {
                        Ok(
                            Expression::SafeCast(
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    5u8 => {
                        Ok(
                            Expression::Multiply(
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    6u8 => {
                        Ok(
                            Expression::Divide(
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    7u8 => {
                        Ok(
                            Expression::Add(
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    8u8 => {
                        Ok(
                            Expression::Subtract(
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            ),
                        )
                    }
                    9u8 => {
                        Ok(
                            Expression::Conditional(
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            ),
                        )
                    }
                    _ => {
                        Err(
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid tag",
                            ),
                        )
                    }
                }
            }
        }
        impl Expression {
            pub fn evaluate(&self, task_state: &TaskState) -> Result<Value, String> {
                match self {
                    Expression::Literal(v) => Ok(v.clone()),
                    Expression::InputValue(index) => {
                        task_state
                            .inputs
                            .get(*index as usize)
                            .cloned()
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!("Input at index {0} not found", index),
                                );
                                res
                            })
                    }
                    Expression::StaticValue(index) => {
                        task_state
                            .definition
                            .static_values
                            .get(*index as usize)
                            .cloned()
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!("Static value at index {0} not found", index),
                                );
                                res
                            })
                    }
                    Expression::CachedValue(index) => {
                        task_state
                            .cached_values
                            .get(*index as usize)
                            .cloned()
                            .flatten()
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!("Cached value at index {0} not found", index),
                                );
                                res
                            })
                    }
                    Expression::SafeCast(inner_expr, target_type) => {
                        let inner_value = Expression::evaluate(inner_expr, task_state)?;
                        inner_value.safe_cast(target_type.clone())
                    }
                    Expression::Multiply(left, right, behavior) => {
                        let left_value = Expression::evaluate(left, task_state)?;
                        let right_value = Expression::evaluate(right, task_state)?;
                        match behavior {
                            ArithmeticBehavior::Checked => {
                                left_value.checked_mul(&right_value)
                            }
                            ArithmeticBehavior::Saturating => {
                                left_value.saturating_mul(&right_value)
                            }
                            ArithmeticBehavior::Wrapping => {
                                left_value.wrapping_mul(&right_value)
                            }
                        }
                    }
                    Expression::Divide(left, right, behavior) => {
                        let left_value = Expression::evaluate(left, task_state)?;
                        let right_value = Expression::evaluate(right, task_state)?;
                        match behavior {
                            ArithmeticBehavior::Checked => {
                                left_value.checked_div(&right_value)
                            }
                            ArithmeticBehavior::Saturating => {
                                left_value.saturating_div(&right_value)
                            }
                            ArithmeticBehavior::Wrapping => {
                                left_value.wrapping_div(&right_value)
                            }
                        }
                    }
                    Expression::Add(left, right, behavior) => {
                        let left_value = Expression::evaluate(left, task_state)?;
                        let right_value = Expression::evaluate(right, task_state)?;
                        match behavior {
                            ArithmeticBehavior::Checked => {
                                left_value.checked_add(&right_value)
                            }
                            ArithmeticBehavior::Saturating => {
                                left_value.saturating_add(&right_value)
                            }
                            ArithmeticBehavior::Wrapping => {
                                left_value.wrapping_add(&right_value)
                            }
                        }
                    }
                    Expression::Subtract(left, right, behavior) => {
                        let left_value = Expression::evaluate(left, task_state)?;
                        let right_value = Expression::evaluate(right, task_state)?;
                        match behavior {
                            ArithmeticBehavior::Checked => {
                                left_value.checked_sub(&right_value)
                            }
                            ArithmeticBehavior::Saturating => {
                                left_value.saturating_sub(&right_value)
                            }
                            ArithmeticBehavior::Wrapping => {
                                left_value.wrapping_sub(&right_value)
                            }
                        }
                    }
                    Expression::Conditional(condition, true_expr, false_expr) => {
                        let condition_value = Condition::evaluate(
                            condition,
                            task_state,
                        )?;
                        if condition_value {
                            Expression::evaluate(true_expr, task_state)
                        } else {
                            Expression::evaluate(false_expr, task_state)
                        }
                    }
                }
            }
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
            pub fn multiply(
                self,
                right: Expression,
                behavior: ArithmeticBehavior,
            ) -> Self {
                Expression::Multiply(Box::new(self), Box::new(right), behavior)
            }
            pub fn checked_multiply(self, right: Expression) -> Self {
                Expression::Multiply(
                    Box::new(self),
                    Box::new(right),
                    ArithmeticBehavior::Checked,
                )
            }
            pub fn divide(
                self,
                right: Expression,
                behavior: ArithmeticBehavior,
            ) -> Self {
                Expression::Divide(Box::new(self), Box::new(right), behavior)
            }
            pub fn add(self, right: Expression, behavior: ArithmeticBehavior) -> Self {
                Expression::Add(Box::new(self), Box::new(right), behavior)
            }
            pub fn checked_add(self, right: Expression) -> Self {
                Expression::Add(
                    Box::new(self),
                    Box::new(right),
                    ArithmeticBehavior::Checked,
                )
            }
            pub fn subtract(
                self,
                right: Expression,
                behavior: ArithmeticBehavior,
            ) -> Self {
                Expression::Subtract(Box::new(self), Box::new(right), behavior)
            }
        }
    }
    pub mod instruction {
        use borsh::{BorshDeserialize, BorshSerialize};
        use super::{validation::Validation, value::{Value, ValueType}};
        pub enum SerializationType {
            Borsh,
            Bytemuck,
            C,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for SerializationType {
            #[inline]
            fn clone(&self) -> SerializationType {
                match self {
                    SerializationType::Borsh => SerializationType::Borsh,
                    SerializationType::Bytemuck => SerializationType::Bytemuck,
                    SerializationType::C => SerializationType::C,
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for SerializationType {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        SerializationType::Borsh => "Borsh",
                        SerializationType::Bytemuck => "Bytemuck",
                        SerializationType::C => "C",
                    },
                )
            }
        }
        impl borsh::de::BorshDeserialize for SerializationType {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                    reader,
                )?;
                <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
            }
        }
        impl borsh::de::EnumExt for SerializationType {
            fn deserialize_variant<R: borsh::maybestd::io::Read>(
                reader: &mut R,
                variant_idx: u8,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let mut return_value = match variant_idx {
                    0u8 => SerializationType::Borsh,
                    1u8 => SerializationType::Bytemuck,
                    2u8 => SerializationType::C,
                    _ => {
                        return Err(
                            borsh::maybestd::io::Error::new(
                                borsh::maybestd::io::ErrorKind::InvalidInput,
                                {
                                    let res = ::alloc::fmt::format(
                                        format_args!("Unexpected variant index: {0:?}", variant_idx),
                                    );
                                    res
                                },
                            ),
                        );
                    }
                };
                Ok(return_value)
            }
        }
        impl borsh::ser::BorshSerialize for SerializationType {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                let variant_idx: u8 = match self {
                    SerializationType::Borsh => 0u8,
                    SerializationType::Bytemuck => 1u8,
                    SerializationType::C => 2u8,
                };
                writer.write_all(&variant_idx.to_le_bytes())?;
                match self {
                    SerializationType::Borsh => {}
                    SerializationType::Bytemuck => {}
                    SerializationType::C => {}
                }
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for SerializationType {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {}
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for SerializationType {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for SerializationType {
            #[inline]
            fn eq(&self, other: &SerializationType) -> bool {
                let __self_tag = ::core::intrinsics::discriminant_value(self);
                let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                __self_tag == __arg1_tag
            }
        }
        pub struct AccountDefinition {
            pub name: String,
            pub writable: bool,
            pub signer: bool,
            pub validate: Option<Vec<Validation>>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for AccountDefinition {
            #[inline]
            fn clone(&self) -> AccountDefinition {
                AccountDefinition {
                    name: ::core::clone::Clone::clone(&self.name),
                    writable: ::core::clone::Clone::clone(&self.writable),
                    signer: ::core::clone::Clone::clone(&self.signer),
                    validate: ::core::clone::Clone::clone(&self.validate),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for AccountDefinition {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field4_finish(
                    f,
                    "AccountDefinition",
                    "name",
                    &self.name,
                    "writable",
                    &self.writable,
                    "signer",
                    &self.signer,
                    "validate",
                    &&self.validate,
                )
            }
        }
        impl borsh::de::BorshDeserialize for AccountDefinition
        where
            String: borsh::BorshDeserialize,
            bool: borsh::BorshDeserialize,
            bool: borsh::BorshDeserialize,
            Option<Vec<Validation>>: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                Ok(Self {
                    name: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    writable: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    signer: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    validate: borsh::BorshDeserialize::deserialize_reader(reader)?,
                })
            }
        }
        impl borsh::ser::BorshSerialize for AccountDefinition
        where
            String: borsh::ser::BorshSerialize,
            bool: borsh::ser::BorshSerialize,
            bool: borsh::ser::BorshSerialize,
            Option<Vec<Validation>>: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                borsh::BorshSerialize::serialize(&self.name, writer)?;
                borsh::BorshSerialize::serialize(&self.writable, writer)?;
                borsh::BorshSerialize::serialize(&self.signer, writer)?;
                borsh::BorshSerialize::serialize(&self.validate, writer)?;
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for AccountDefinition {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<String>;
                let _: ::core::cmp::AssertParamIsEq<bool>;
                let _: ::core::cmp::AssertParamIsEq<Option<Vec<Validation>>>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for AccountDefinition {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for AccountDefinition {
            #[inline]
            fn eq(&self, other: &AccountDefinition) -> bool {
                self.name == other.name && self.writable == other.writable
                    && self.signer == other.signer && self.validate == other.validate
            }
        }
        pub struct InstructionDefinition {
            pub serialization: SerializationType,
            pub arguments: Vec<ArgumentDefinition>,
            pub accounts: Vec<AccountDefinition>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for InstructionDefinition {
            #[inline]
            fn clone(&self) -> InstructionDefinition {
                InstructionDefinition {
                    serialization: ::core::clone::Clone::clone(&self.serialization),
                    arguments: ::core::clone::Clone::clone(&self.arguments),
                    accounts: ::core::clone::Clone::clone(&self.accounts),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for InstructionDefinition {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field3_finish(
                    f,
                    "InstructionDefinition",
                    "serialization",
                    &self.serialization,
                    "arguments",
                    &self.arguments,
                    "accounts",
                    &&self.accounts,
                )
            }
        }
        impl borsh::de::BorshDeserialize for InstructionDefinition
        where
            SerializationType: borsh::BorshDeserialize,
            Vec<ArgumentDefinition>: borsh::BorshDeserialize,
            Vec<AccountDefinition>: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                Ok(Self {
                    serialization: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    arguments: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    accounts: borsh::BorshDeserialize::deserialize_reader(reader)?,
                })
            }
        }
        impl borsh::ser::BorshSerialize for InstructionDefinition
        where
            SerializationType: borsh::ser::BorshSerialize,
            Vec<ArgumentDefinition>: borsh::ser::BorshSerialize,
            Vec<AccountDefinition>: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                borsh::BorshSerialize::serialize(&self.serialization, writer)?;
                borsh::BorshSerialize::serialize(&self.arguments, writer)?;
                borsh::BorshSerialize::serialize(&self.accounts, writer)?;
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for InstructionDefinition {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<SerializationType>;
                let _: ::core::cmp::AssertParamIsEq<Vec<ArgumentDefinition>>;
                let _: ::core::cmp::AssertParamIsEq<Vec<AccountDefinition>>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for InstructionDefinition {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for InstructionDefinition {
            #[inline]
            fn eq(&self, other: &InstructionDefinition) -> bool {
                self.serialization == other.serialization
                    && self.arguments == other.arguments
                    && self.accounts == other.accounts
            }
        }
        pub enum ArgumentDefinition {
            Constant { value: Value },
            Input { value_type: ValueType },
        }
        #[automatically_derived]
        impl ::core::clone::Clone for ArgumentDefinition {
            #[inline]
            fn clone(&self) -> ArgumentDefinition {
                match self {
                    ArgumentDefinition::Constant { value: __self_0 } => {
                        ArgumentDefinition::Constant {
                            value: ::core::clone::Clone::clone(__self_0),
                        }
                    }
                    ArgumentDefinition::Input { value_type: __self_0 } => {
                        ArgumentDefinition::Input {
                            value_type: ::core::clone::Clone::clone(__self_0),
                        }
                    }
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ArgumentDefinition {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match self {
                    ArgumentDefinition::Constant { value: __self_0 } => {
                        ::core::fmt::Formatter::debug_struct_field1_finish(
                            f,
                            "Constant",
                            "value",
                            &__self_0,
                        )
                    }
                    ArgumentDefinition::Input { value_type: __self_0 } => {
                        ::core::fmt::Formatter::debug_struct_field1_finish(
                            f,
                            "Input",
                            "value_type",
                            &__self_0,
                        )
                    }
                }
            }
        }
        impl borsh::de::BorshDeserialize for ArgumentDefinition
        where
            Value: borsh::BorshDeserialize,
            ValueType: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                    reader,
                )?;
                <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
            }
        }
        impl borsh::de::EnumExt for ArgumentDefinition
        where
            Value: borsh::BorshDeserialize,
            ValueType: borsh::BorshDeserialize,
        {
            fn deserialize_variant<R: borsh::maybestd::io::Read>(
                reader: &mut R,
                variant_idx: u8,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let mut return_value = match variant_idx {
                    0u8 => {
                        ArgumentDefinition::Constant {
                            value: borsh::BorshDeserialize::deserialize_reader(reader)?,
                        }
                    }
                    1u8 => {
                        ArgumentDefinition::Input {
                            value_type: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                        }
                    }
                    _ => {
                        return Err(
                            borsh::maybestd::io::Error::new(
                                borsh::maybestd::io::ErrorKind::InvalidInput,
                                {
                                    let res = ::alloc::fmt::format(
                                        format_args!("Unexpected variant index: {0:?}", variant_idx),
                                    );
                                    res
                                },
                            ),
                        );
                    }
                };
                Ok(return_value)
            }
        }
        impl borsh::ser::BorshSerialize for ArgumentDefinition
        where
            Value: borsh::ser::BorshSerialize,
            ValueType: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                let variant_idx: u8 = match self {
                    ArgumentDefinition::Constant { .. } => 0u8,
                    ArgumentDefinition::Input { .. } => 1u8,
                };
                writer.write_all(&variant_idx.to_le_bytes())?;
                match self {
                    ArgumentDefinition::Constant { value } => {
                        borsh::BorshSerialize::serialize(value, writer)?;
                    }
                    ArgumentDefinition::Input { value_type } => {
                        borsh::BorshSerialize::serialize(value_type, writer)?;
                    }
                }
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for ArgumentDefinition {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<Value>;
                let _: ::core::cmp::AssertParamIsEq<ValueType>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for ArgumentDefinition {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for ArgumentDefinition {
            #[inline]
            fn eq(&self, other: &ArgumentDefinition) -> bool {
                let __self_tag = ::core::intrinsics::discriminant_value(self);
                let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                __self_tag == __arg1_tag
                    && match (self, other) {
                        (
                            ArgumentDefinition::Constant { value: __self_0 },
                            ArgumentDefinition::Constant { value: __arg1_0 },
                        ) => *__self_0 == *__arg1_0,
                        (
                            ArgumentDefinition::Input { value_type: __self_0 },
                            ArgumentDefinition::Input { value_type: __arg1_0 },
                        ) => *__self_0 == *__arg1_0,
                        _ => unsafe { ::core::intrinsics::unreachable() }
                    }
            }
        }
    }
    pub mod schema {
        use borsh::{BorshDeserialize, BorshSerialize};
        use super::{instruction::InstructionDefinition, task::task::TaskDefinition};
        pub struct GlobalSchema {
            pub instructions: Vec<InstructionDefinition>,
            pub tasks: Vec<TaskDefinition>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for GlobalSchema {
            #[inline]
            fn clone(&self) -> GlobalSchema {
                GlobalSchema {
                    instructions: ::core::clone::Clone::clone(&self.instructions),
                    tasks: ::core::clone::Clone::clone(&self.tasks),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for GlobalSchema {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field2_finish(
                    f,
                    "GlobalSchema",
                    "instructions",
                    &self.instructions,
                    "tasks",
                    &&self.tasks,
                )
            }
        }
        impl borsh::de::BorshDeserialize for GlobalSchema
        where
            Vec<InstructionDefinition>: borsh::BorshDeserialize,
            Vec<TaskDefinition>: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                Ok(Self {
                    instructions: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    tasks: borsh::BorshDeserialize::deserialize_reader(reader)?,
                })
            }
        }
        impl borsh::ser::BorshSerialize for GlobalSchema
        where
            Vec<InstructionDefinition>: borsh::ser::BorshSerialize,
            Vec<TaskDefinition>: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                borsh::BorshSerialize::serialize(&self.instructions, writer)?;
                borsh::BorshSerialize::serialize(&self.tasks, writer)?;
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for GlobalSchema {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<Vec<InstructionDefinition>>;
                let _: ::core::cmp::AssertParamIsEq<Vec<TaskDefinition>>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for GlobalSchema {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for GlobalSchema {
            #[inline]
            fn eq(&self, other: &GlobalSchema) -> bool {
                self.instructions == other.instructions && self.tasks == other.tasks
            }
        }
    }
    pub mod task {
        pub mod action {
            pub mod raw_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                pub struct RawInstruction {
                    pub data: Vec<u8>,
                    pub num_accounts: u8,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for RawInstruction {
                    #[inline]
                    fn clone(&self) -> RawInstruction {
                        RawInstruction {
                            data: ::core::clone::Clone::clone(&self.data),
                            num_accounts: ::core::clone::Clone::clone(&self.num_accounts),
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for RawInstruction {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::debug_struct_field2_finish(
                            f,
                            "RawInstruction",
                            "data",
                            &self.data,
                            "num_accounts",
                            &&self.num_accounts,
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for RawInstruction
                where
                    Vec<u8>: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        Ok(Self {
                            data: borsh::BorshDeserialize::deserialize_reader(reader)?,
                            num_accounts: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                        })
                    }
                }
                impl borsh::ser::BorshSerialize for RawInstruction
                where
                    Vec<u8>: borsh::ser::BorshSerialize,
                    u8: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        borsh::BorshSerialize::serialize(&self.data, writer)?;
                        borsh::BorshSerialize::serialize(&self.num_accounts, writer)?;
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for RawInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<Vec<u8>>;
                        let _: ::core::cmp::AssertParamIsEq<u8>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for RawInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for RawInstruction {
                    #[inline]
                    fn eq(&self, other: &RawInstruction) -> bool {
                        self.data == other.data
                            && self.num_accounts == other.num_accounts
                    }
                }
            }
            pub mod record_value {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::schema::{expression::Expression, value::ValueType};
                pub enum SetCache {
                    AccountData { account: u8, offset: u8, value_type: ValueType },
                    Expression { index: u8, expression: Expression },
                }
                #[automatically_derived]
                impl ::core::clone::Clone for SetCache {
                    #[inline]
                    fn clone(&self) -> SetCache {
                        match self {
                            SetCache::AccountData {
                                account: __self_0,
                                offset: __self_1,
                                value_type: __self_2,
                            } => {
                                SetCache::AccountData {
                                    account: ::core::clone::Clone::clone(__self_0),
                                    offset: ::core::clone::Clone::clone(__self_1),
                                    value_type: ::core::clone::Clone::clone(__self_2),
                                }
                            }
                            SetCache::Expression {
                                index: __self_0,
                                expression: __self_1,
                            } => {
                                SetCache::Expression {
                                    index: ::core::clone::Clone::clone(__self_0),
                                    expression: ::core::clone::Clone::clone(__self_1),
                                }
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for SetCache {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            SetCache::AccountData {
                                account: __self_0,
                                offset: __self_1,
                                value_type: __self_2,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field3_finish(
                                    f,
                                    "AccountData",
                                    "account",
                                    __self_0,
                                    "offset",
                                    __self_1,
                                    "value_type",
                                    &__self_2,
                                )
                            }
                            SetCache::Expression {
                                index: __self_0,
                                expression: __self_1,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field2_finish(
                                    f,
                                    "Expression",
                                    "index",
                                    __self_0,
                                    "expression",
                                    &__self_1,
                                )
                            }
                        }
                    }
                }
                impl borsh::de::BorshDeserialize for SetCache
                where
                    u8: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    ValueType: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                            reader,
                        )?;
                        <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
                    }
                }
                impl borsh::de::EnumExt for SetCache
                where
                    u8: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    ValueType: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => {
                                SetCache::AccountData {
                                    account: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    offset: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    value_type: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                }
                            }
                            1u8 => {
                                SetCache::Expression {
                                    index: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    expression: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                }
                            }
                            _ => {
                                return Err(
                                    borsh::maybestd::io::Error::new(
                                        borsh::maybestd::io::ErrorKind::InvalidInput,
                                        {
                                            let res = ::alloc::fmt::format(
                                                format_args!("Unexpected variant index: {0:?}", variant_idx),
                                            );
                                            res
                                        },
                                    ),
                                );
                            }
                        };
                        Ok(return_value)
                    }
                }
                impl borsh::ser::BorshSerialize for SetCache
                where
                    u8: borsh::ser::BorshSerialize,
                    u8: borsh::ser::BorshSerialize,
                    ValueType: borsh::ser::BorshSerialize,
                    u8: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            SetCache::AccountData { .. } => 0u8,
                            SetCache::Expression { .. } => 1u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            SetCache::AccountData { account, offset, value_type } => {
                                borsh::BorshSerialize::serialize(account, writer)?;
                                borsh::BorshSerialize::serialize(offset, writer)?;
                                borsh::BorshSerialize::serialize(value_type, writer)?;
                            }
                            SetCache::Expression { index, expression } => {
                                borsh::BorshSerialize::serialize(index, writer)?;
                                borsh::BorshSerialize::serialize(expression, writer)?;
                            }
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for SetCache {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<u8>;
                        let _: ::core::cmp::AssertParamIsEq<ValueType>;
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for SetCache {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for SetCache {
                    #[inline]
                    fn eq(&self, other: &SetCache) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    SetCache::AccountData {
                                        account: __self_0,
                                        offset: __self_1,
                                        value_type: __self_2,
                                    },
                                    SetCache::AccountData {
                                        account: __arg1_0,
                                        offset: __arg1_1,
                                        value_type: __arg1_2,
                                    },
                                ) => {
                                    *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                        && *__self_2 == *__arg1_2
                                }
                                (
                                    SetCache::Expression {
                                        index: __self_0,
                                        expression: __self_1,
                                    },
                                    SetCache::Expression {
                                        index: __arg1_0,
                                        expression: __arg1_1,
                                    },
                                ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                                _ => unsafe { ::core::intrinsics::unreachable() }
                            }
                    }
                }
            }
            pub mod schema_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::schema::{expression::Expression, value::Value};
                pub struct SchemaInstruction {
                    pub instruction_id: u32,
                    pub program: TaskAccount,
                    pub accounts: Vec<TaskAccount>,
                    pub arguments: Vec<TaskArgument>,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for SchemaInstruction {
                    #[inline]
                    fn clone(&self) -> SchemaInstruction {
                        SchemaInstruction {
                            instruction_id: ::core::clone::Clone::clone(
                                &self.instruction_id,
                            ),
                            program: ::core::clone::Clone::clone(&self.program),
                            accounts: ::core::clone::Clone::clone(&self.accounts),
                            arguments: ::core::clone::Clone::clone(&self.arguments),
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for SchemaInstruction {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::debug_struct_field4_finish(
                            f,
                            "SchemaInstruction",
                            "instruction_id",
                            &self.instruction_id,
                            "program",
                            &self.program,
                            "accounts",
                            &self.accounts,
                            "arguments",
                            &&self.arguments,
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for SchemaInstruction
                where
                    u32: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Vec<TaskAccount>: borsh::BorshDeserialize,
                    Vec<TaskArgument>: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        Ok(Self {
                            instruction_id: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                            program: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                            accounts: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                            arguments: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                        })
                    }
                }
                impl borsh::ser::BorshSerialize for SchemaInstruction
                where
                    u32: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    Vec<TaskAccount>: borsh::ser::BorshSerialize,
                    Vec<TaskArgument>: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        borsh::BorshSerialize::serialize(&self.instruction_id, writer)?;
                        borsh::BorshSerialize::serialize(&self.program, writer)?;
                        borsh::BorshSerialize::serialize(&self.accounts, writer)?;
                        borsh::BorshSerialize::serialize(&self.arguments, writer)?;
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for SchemaInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<u32>;
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                        let _: ::core::cmp::AssertParamIsEq<Vec<TaskAccount>>;
                        let _: ::core::cmp::AssertParamIsEq<Vec<TaskArgument>>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for SchemaInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for SchemaInstruction {
                    #[inline]
                    fn eq(&self, other: &SchemaInstruction) -> bool {
                        self.instruction_id == other.instruction_id
                            && self.program == other.program
                            && self.accounts == other.accounts
                            && self.arguments == other.arguments
                    }
                }
                pub enum TaskAccount {
                    Input(u8),
                    Evaluated(Expression),
                    MultipleInput { start: u8, length: u8 },
                    MultipleEvaluated { start: Expression, length: Expression },
                }
                #[automatically_derived]
                impl ::core::clone::Clone for TaskAccount {
                    #[inline]
                    fn clone(&self) -> TaskAccount {
                        match self {
                            TaskAccount::Input(__self_0) => {
                                TaskAccount::Input(::core::clone::Clone::clone(__self_0))
                            }
                            TaskAccount::Evaluated(__self_0) => {
                                TaskAccount::Evaluated(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            TaskAccount::MultipleInput {
                                start: __self_0,
                                length: __self_1,
                            } => {
                                TaskAccount::MultipleInput {
                                    start: ::core::clone::Clone::clone(__self_0),
                                    length: ::core::clone::Clone::clone(__self_1),
                                }
                            }
                            TaskAccount::MultipleEvaluated {
                                start: __self_0,
                                length: __self_1,
                            } => {
                                TaskAccount::MultipleEvaluated {
                                    start: ::core::clone::Clone::clone(__self_0),
                                    length: ::core::clone::Clone::clone(__self_1),
                                }
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for TaskAccount {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            TaskAccount::Input(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "Input",
                                    &__self_0,
                                )
                            }
                            TaskAccount::Evaluated(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "Evaluated",
                                    &__self_0,
                                )
                            }
                            TaskAccount::MultipleInput {
                                start: __self_0,
                                length: __self_1,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field2_finish(
                                    f,
                                    "MultipleInput",
                                    "start",
                                    __self_0,
                                    "length",
                                    &__self_1,
                                )
                            }
                            TaskAccount::MultipleEvaluated {
                                start: __self_0,
                                length: __self_1,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field2_finish(
                                    f,
                                    "MultipleEvaluated",
                                    "start",
                                    __self_0,
                                    "length",
                                    &__self_1,
                                )
                            }
                        }
                    }
                }
                impl borsh::de::BorshDeserialize for TaskAccount
                where
                    u8: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                            reader,
                        )?;
                        <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
                    }
                }
                impl borsh::de::EnumExt for TaskAccount
                where
                    u8: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    u8: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => {
                                TaskAccount::Input(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                )
                            }
                            1u8 => {
                                TaskAccount::Evaluated(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                )
                            }
                            2u8 => {
                                TaskAccount::MultipleInput {
                                    start: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    length: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                }
                            }
                            3u8 => {
                                TaskAccount::MultipleEvaluated {
                                    start: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    length: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                }
                            }
                            _ => {
                                return Err(
                                    borsh::maybestd::io::Error::new(
                                        borsh::maybestd::io::ErrorKind::InvalidInput,
                                        {
                                            let res = ::alloc::fmt::format(
                                                format_args!("Unexpected variant index: {0:?}", variant_idx),
                                            );
                                            res
                                        },
                                    ),
                                );
                            }
                        };
                        Ok(return_value)
                    }
                }
                impl borsh::ser::BorshSerialize for TaskAccount
                where
                    u8: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                    u8: borsh::ser::BorshSerialize,
                    u8: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            TaskAccount::Input(..) => 0u8,
                            TaskAccount::Evaluated(..) => 1u8,
                            TaskAccount::MultipleInput { .. } => 2u8,
                            TaskAccount::MultipleEvaluated { .. } => 3u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            TaskAccount::Input(id0) => {
                                borsh::BorshSerialize::serialize(id0, writer)?;
                            }
                            TaskAccount::Evaluated(id0) => {
                                borsh::BorshSerialize::serialize(id0, writer)?;
                            }
                            TaskAccount::MultipleInput { start, length } => {
                                borsh::BorshSerialize::serialize(start, writer)?;
                                borsh::BorshSerialize::serialize(length, writer)?;
                            }
                            TaskAccount::MultipleEvaluated { start, length } => {
                                borsh::BorshSerialize::serialize(start, writer)?;
                                borsh::BorshSerialize::serialize(length, writer)?;
                            }
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for TaskAccount {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<u8>;
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for TaskAccount {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for TaskAccount {
                    #[inline]
                    fn eq(&self, other: &TaskAccount) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    TaskAccount::Input(__self_0),
                                    TaskAccount::Input(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    TaskAccount::Evaluated(__self_0),
                                    TaskAccount::Evaluated(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    TaskAccount::MultipleInput {
                                        start: __self_0,
                                        length: __self_1,
                                    },
                                    TaskAccount::MultipleInput {
                                        start: __arg1_0,
                                        length: __arg1_1,
                                    },
                                ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                                (
                                    TaskAccount::MultipleEvaluated {
                                        start: __self_0,
                                        length: __self_1,
                                    },
                                    TaskAccount::MultipleEvaluated {
                                        start: __arg1_0,
                                        length: __arg1_1,
                                    },
                                ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                                _ => unsafe { ::core::intrinsics::unreachable() }
                            }
                    }
                }
                pub enum TaskArgument {
                    Expression(Expression),
                    Literal(Value),
                }
                #[automatically_derived]
                impl ::core::clone::Clone for TaskArgument {
                    #[inline]
                    fn clone(&self) -> TaskArgument {
                        match self {
                            TaskArgument::Expression(__self_0) => {
                                TaskArgument::Expression(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            TaskArgument::Literal(__self_0) => {
                                TaskArgument::Literal(::core::clone::Clone::clone(__self_0))
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for TaskArgument {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            TaskArgument::Expression(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "Expression",
                                    &__self_0,
                                )
                            }
                            TaskArgument::Literal(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "Literal",
                                    &__self_0,
                                )
                            }
                        }
                    }
                }
                impl borsh::de::BorshDeserialize for TaskArgument
                where
                    Expression: borsh::BorshDeserialize,
                    Value: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                            reader,
                        )?;
                        <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
                    }
                }
                impl borsh::de::EnumExt for TaskArgument
                where
                    Expression: borsh::BorshDeserialize,
                    Value: borsh::BorshDeserialize,
                {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => {
                                TaskArgument::Expression(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                )
                            }
                            1u8 => {
                                TaskArgument::Literal(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                )
                            }
                            _ => {
                                return Err(
                                    borsh::maybestd::io::Error::new(
                                        borsh::maybestd::io::ErrorKind::InvalidInput,
                                        {
                                            let res = ::alloc::fmt::format(
                                                format_args!("Unexpected variant index: {0:?}", variant_idx),
                                            );
                                            res
                                        },
                                    ),
                                );
                            }
                        };
                        Ok(return_value)
                    }
                }
                impl borsh::ser::BorshSerialize for TaskArgument
                where
                    Expression: borsh::ser::BorshSerialize,
                    Value: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            TaskArgument::Expression(..) => 0u8,
                            TaskArgument::Literal(..) => 1u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            TaskArgument::Expression(id0) => {
                                borsh::BorshSerialize::serialize(id0, writer)?;
                            }
                            TaskArgument::Literal(id0) => {
                                borsh::BorshSerialize::serialize(id0, writer)?;
                            }
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for TaskArgument {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                        let _: ::core::cmp::AssertParamIsEq<Value>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for TaskArgument {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for TaskArgument {
                    #[inline]
                    fn eq(&self, other: &TaskArgument) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    TaskArgument::Expression(__self_0),
                                    TaskArgument::Expression(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    TaskArgument::Literal(__self_0),
                                    TaskArgument::Literal(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                _ => unsafe { ::core::intrinsics::unreachable() }
                            }
                    }
                }
            }
        }
        pub mod task {
            use std::io::{Read, Repeat};
            use borsh::{BorshDeserialize, BorshSerialize};
            use crate::schema::{
                expression::{condition::Condition, Expression},
                validation::Validation, value::Value,
            };
            use super::action::{
                raw_instruction::RawInstruction, record_value::SetCache,
                schema_instruction::SchemaInstruction,
            };
            pub struct ActionContext<'a> {
                pub current_index: u8,
                pub actions: &'a [TaskAction],
            }
            pub struct TaskState<'a> {
                pub definition: &'a TaskDefinition,
                pub inputs: &'a [Value],
                pub cached_values: Vec<Option<Value>>,
            }
            pub struct TaskDefinition {
                pub actions: Vec<TaskAction>,
                pub static_values: Vec<Value>,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for TaskDefinition {
                #[inline]
                fn clone(&self) -> TaskDefinition {
                    TaskDefinition {
                        actions: ::core::clone::Clone::clone(&self.actions),
                        static_values: ::core::clone::Clone::clone(&self.static_values),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TaskDefinition {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "TaskDefinition",
                        "actions",
                        &self.actions,
                        "static_values",
                        &&self.static_values,
                    )
                }
            }
            impl borsh::de::BorshDeserialize for TaskDefinition
            where
                Vec<TaskAction>: borsh::BorshDeserialize,
                Vec<Value>: borsh::BorshDeserialize,
            {
                fn deserialize_reader<R: borsh::maybestd::io::Read>(
                    reader: &mut R,
                ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                    Ok(Self {
                        actions: borsh::BorshDeserialize::deserialize_reader(reader)?,
                        static_values: borsh::BorshDeserialize::deserialize_reader(
                            reader,
                        )?,
                    })
                }
            }
            impl borsh::ser::BorshSerialize for TaskDefinition
            where
                Vec<TaskAction>: borsh::ser::BorshSerialize,
                Vec<Value>: borsh::ser::BorshSerialize,
            {
                fn serialize<W: borsh::maybestd::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                    borsh::BorshSerialize::serialize(&self.actions, writer)?;
                    borsh::BorshSerialize::serialize(&self.static_values, writer)?;
                    Ok(())
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for TaskDefinition {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<Vec<TaskAction>>;
                    let _: ::core::cmp::AssertParamIsEq<Vec<Value>>;
                }
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for TaskDefinition {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for TaskDefinition {
                #[inline]
                fn eq(&self, other: &TaskDefinition) -> bool {
                    self.actions == other.actions
                        && self.static_values == other.static_values
                }
            }
            pub enum TaskAction {
                SchemaInstruction(SchemaInstruction),
                RawInstruction(RawInstruction),
                RawInstructionGroup(Vec<RawInstruction>),
                SetCache(SetCache),
                Validate(Validation),
                Repeat,
                Branch {
                    condition: Condition,
                    true_action: Expression,
                    false_action: Expression,
                },
                Loop { condition: Condition, actions: Vec<Box<TaskAction>> },
            }
            #[automatically_derived]
            impl ::core::clone::Clone for TaskAction {
                #[inline]
                fn clone(&self) -> TaskAction {
                    match self {
                        TaskAction::SchemaInstruction(__self_0) => {
                            TaskAction::SchemaInstruction(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        TaskAction::RawInstruction(__self_0) => {
                            TaskAction::RawInstruction(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        TaskAction::RawInstructionGroup(__self_0) => {
                            TaskAction::RawInstructionGroup(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        TaskAction::SetCache(__self_0) => {
                            TaskAction::SetCache(::core::clone::Clone::clone(__self_0))
                        }
                        TaskAction::Validate(__self_0) => {
                            TaskAction::Validate(::core::clone::Clone::clone(__self_0))
                        }
                        TaskAction::Repeat => TaskAction::Repeat,
                        TaskAction::Branch {
                            condition: __self_0,
                            true_action: __self_1,
                            false_action: __self_2,
                        } => {
                            TaskAction::Branch {
                                condition: ::core::clone::Clone::clone(__self_0),
                                true_action: ::core::clone::Clone::clone(__self_1),
                                false_action: ::core::clone::Clone::clone(__self_2),
                            }
                        }
                        TaskAction::Loop { condition: __self_0, actions: __self_1 } => {
                            TaskAction::Loop {
                                condition: ::core::clone::Clone::clone(__self_0),
                                actions: ::core::clone::Clone::clone(__self_1),
                            }
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TaskAction {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        TaskAction::SchemaInstruction(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SchemaInstruction",
                                &__self_0,
                            )
                        }
                        TaskAction::RawInstruction(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "RawInstruction",
                                &__self_0,
                            )
                        }
                        TaskAction::RawInstructionGroup(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "RawInstructionGroup",
                                &__self_0,
                            )
                        }
                        TaskAction::SetCache(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SetCache",
                                &__self_0,
                            )
                        }
                        TaskAction::Validate(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Validate",
                                &__self_0,
                            )
                        }
                        TaskAction::Repeat => {
                            ::core::fmt::Formatter::write_str(f, "Repeat")
                        }
                        TaskAction::Branch {
                            condition: __self_0,
                            true_action: __self_1,
                            false_action: __self_2,
                        } => {
                            ::core::fmt::Formatter::debug_struct_field3_finish(
                                f,
                                "Branch",
                                "condition",
                                __self_0,
                                "true_action",
                                __self_1,
                                "false_action",
                                &__self_2,
                            )
                        }
                        TaskAction::Loop { condition: __self_0, actions: __self_1 } => {
                            ::core::fmt::Formatter::debug_struct_field2_finish(
                                f,
                                "Loop",
                                "condition",
                                __self_0,
                                "actions",
                                &__self_1,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for TaskAction {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<SchemaInstruction>;
                    let _: ::core::cmp::AssertParamIsEq<RawInstruction>;
                    let _: ::core::cmp::AssertParamIsEq<Vec<RawInstruction>>;
                    let _: ::core::cmp::AssertParamIsEq<SetCache>;
                    let _: ::core::cmp::AssertParamIsEq<Validation>;
                    let _: ::core::cmp::AssertParamIsEq<Condition>;
                    let _: ::core::cmp::AssertParamIsEq<Expression>;
                    let _: ::core::cmp::AssertParamIsEq<Vec<Box<TaskAction>>>;
                }
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for TaskAction {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for TaskAction {
                #[inline]
                fn eq(&self, other: &TaskAction) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                TaskAction::SchemaInstruction(__self_0),
                                TaskAction::SchemaInstruction(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                TaskAction::RawInstruction(__self_0),
                                TaskAction::RawInstruction(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                TaskAction::RawInstructionGroup(__self_0),
                                TaskAction::RawInstructionGroup(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                TaskAction::SetCache(__self_0),
                                TaskAction::SetCache(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                TaskAction::Validate(__self_0),
                                TaskAction::Validate(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                TaskAction::Branch {
                                    condition: __self_0,
                                    true_action: __self_1,
                                    false_action: __self_2,
                                },
                                TaskAction::Branch {
                                    condition: __arg1_0,
                                    true_action: __arg1_1,
                                    false_action: __arg1_2,
                                },
                            ) => {
                                *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                    && *__self_2 == *__arg1_2
                            }
                            (
                                TaskAction::Loop { condition: __self_0, actions: __self_1 },
                                TaskAction::Loop { condition: __arg1_0, actions: __arg1_1 },
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            _ => true,
                        }
                }
            }
            impl BorshDeserialize for TaskAction {
                fn deserialize_reader<R: Read>(
                    reader: &mut R,
                ) -> borsh::maybestd::io::Result<Self> {
                    let tag: u8 = u8::deserialize_reader(reader)?;
                    match tag {
                        0 => {
                            Ok(
                                TaskAction::SchemaInstruction(
                                    SchemaInstruction::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        1 => {
                            Ok(
                                TaskAction::RawInstruction(
                                    RawInstruction::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        2 => {
                            Ok(
                                TaskAction::RawInstructionGroup(
                                    Vec::<RawInstruction>::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        3 => {
                            Ok(
                                TaskAction::SetCache(SetCache::deserialize_reader(reader)?),
                            )
                        }
                        4 => {
                            Ok(
                                TaskAction::Validate(
                                    Validation::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        5 => Ok(TaskAction::Repeat),
                        6 => {
                            Ok(TaskAction::Branch {
                                condition: Condition::deserialize_reader(reader)?,
                                true_action: Expression::deserialize_reader(reader)?,
                                false_action: Expression::deserialize_reader(reader)?,
                            })
                        }
                        7 => {
                            Ok(TaskAction::Loop {
                                condition: Condition::deserialize_reader(reader)?,
                                actions: Vec::<Box<TaskAction>>::deserialize_reader(reader)?,
                            })
                        }
                        _ => {
                            Err(
                                borsh::maybestd::io::Error::new(
                                    borsh::maybestd::io::ErrorKind::InvalidData,
                                    "invalid tag",
                                ),
                            )
                        }
                    }
                }
            }
            impl BorshSerialize for TaskAction {
                fn serialize<W: std::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> borsh::maybestd::io::Result<()> {
                    let tag: u8 = match self {
                        TaskAction::SchemaInstruction(_) => 0,
                        TaskAction::RawInstruction(_) => 1,
                        TaskAction::RawInstructionGroup(_) => 2,
                        TaskAction::SetCache(_) => 3,
                        TaskAction::Validate(_) => 4,
                        TaskAction::Repeat => 5,
                        TaskAction::Branch { .. } => 6,
                        TaskAction::Loop { .. } => 7,
                    };
                    tag.serialize(writer)?;
                    match self {
                        TaskAction::SchemaInstruction(schema_instruction) => {
                            schema_instruction.serialize(writer)?;
                        }
                        TaskAction::RawInstruction(raw_instruction) => {
                            raw_instruction.serialize(writer)?;
                        }
                        TaskAction::RawInstructionGroup(raw_instruction_group) => {
                            raw_instruction_group.serialize(writer)?;
                        }
                        TaskAction::SetCache(record_value) => {
                            record_value.serialize(writer)?;
                        }
                        TaskAction::Validate(validation) => {
                            validation.serialize(writer)?;
                        }
                        TaskAction::Branch { condition, true_action, false_action } => {
                            condition.serialize(writer)?;
                            true_action.serialize(writer)?;
                            false_action.serialize(writer)?;
                        }
                        TaskAction::Loop { condition, actions } => {
                            condition.serialize(writer)?;
                            actions.serialize(writer)?;
                        }
                        TaskAction::Repeat => {}
                    }
                    Ok(())
                }
            }
        }
    }
    pub mod validation {
        use borsh::{BorshDeserialize, BorshSerialize};
        pub enum Validation {
            IsTokenAccount,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for Validation {
            #[inline]
            fn clone(&self) -> Validation {
                Validation::IsTokenAccount
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for Validation {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(f, "IsTokenAccount")
            }
        }
        impl borsh::de::BorshDeserialize for Validation {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                    reader,
                )?;
                <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
            }
        }
        impl borsh::de::EnumExt for Validation {
            fn deserialize_variant<R: borsh::maybestd::io::Read>(
                reader: &mut R,
                variant_idx: u8,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let mut return_value = match variant_idx {
                    0u8 => Validation::IsTokenAccount,
                    _ => {
                        return Err(
                            borsh::maybestd::io::Error::new(
                                borsh::maybestd::io::ErrorKind::InvalidInput,
                                {
                                    let res = ::alloc::fmt::format(
                                        format_args!("Unexpected variant index: {0:?}", variant_idx),
                                    );
                                    res
                                },
                            ),
                        );
                    }
                };
                Ok(return_value)
            }
        }
        impl borsh::ser::BorshSerialize for Validation {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                let variant_idx: u8 = match self {
                    Validation::IsTokenAccount => 0u8,
                };
                writer.write_all(&variant_idx.to_le_bytes())?;
                match self {
                    Validation::IsTokenAccount => {}
                }
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for Validation {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {}
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for Validation {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for Validation {
            #[inline]
            fn eq(&self, other: &Validation) -> bool {
                true
            }
        }
    }
    pub mod value {
        use std::io::{Read, Write};
        use borsh::{BorshDeserialize, BorshSerialize};
        use super::expression::Expression;
        pub enum ValueType {
            U8,
            I8,
            U16,
            I16,
            U32,
            I32,
            U64,
            I64,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for ValueType {
            #[inline]
            fn clone(&self) -> ValueType {
                match self {
                    ValueType::U8 => ValueType::U8,
                    ValueType::I8 => ValueType::I8,
                    ValueType::U16 => ValueType::U16,
                    ValueType::I16 => ValueType::I16,
                    ValueType::U32 => ValueType::U32,
                    ValueType::I32 => ValueType::I32,
                    ValueType::U64 => ValueType::U64,
                    ValueType::I64 => ValueType::I64,
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ValueType {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        ValueType::U8 => "U8",
                        ValueType::I8 => "I8",
                        ValueType::U16 => "U16",
                        ValueType::I16 => "I16",
                        ValueType::U32 => "U32",
                        ValueType::I32 => "I32",
                        ValueType::U64 => "U64",
                        ValueType::I64 => "I64",
                    },
                )
            }
        }
        impl borsh::de::BorshDeserialize for ValueType {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                    reader,
                )?;
                <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
            }
        }
        impl borsh::de::EnumExt for ValueType {
            fn deserialize_variant<R: borsh::maybestd::io::Read>(
                reader: &mut R,
                variant_idx: u8,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                let mut return_value = match variant_idx {
                    0u8 => ValueType::U8,
                    1u8 => ValueType::I8,
                    2u8 => ValueType::U16,
                    3u8 => ValueType::I16,
                    4u8 => ValueType::U32,
                    5u8 => ValueType::I32,
                    6u8 => ValueType::U64,
                    7u8 => ValueType::I64,
                    _ => {
                        return Err(
                            borsh::maybestd::io::Error::new(
                                borsh::maybestd::io::ErrorKind::InvalidInput,
                                {
                                    let res = ::alloc::fmt::format(
                                        format_args!("Unexpected variant index: {0:?}", variant_idx),
                                    );
                                    res
                                },
                            ),
                        );
                    }
                };
                Ok(return_value)
            }
        }
        impl borsh::ser::BorshSerialize for ValueType {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                let variant_idx: u8 = match self {
                    ValueType::U8 => 0u8,
                    ValueType::I8 => 1u8,
                    ValueType::U16 => 2u8,
                    ValueType::I16 => 3u8,
                    ValueType::U32 => 4u8,
                    ValueType::I32 => 5u8,
                    ValueType::U64 => 6u8,
                    ValueType::I64 => 7u8,
                };
                writer.write_all(&variant_idx.to_le_bytes())?;
                match self {
                    ValueType::U8 => {}
                    ValueType::I8 => {}
                    ValueType::U16 => {}
                    ValueType::I16 => {}
                    ValueType::U32 => {}
                    ValueType::I32 => {}
                    ValueType::U64 => {}
                    ValueType::I64 => {}
                }
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for ValueType {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {}
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for ValueType {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for ValueType {
            #[inline]
            fn eq(&self, other: &ValueType) -> bool {
                let __self_tag = ::core::intrinsics::discriminant_value(self);
                let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                __self_tag == __arg1_tag
            }
        }
        pub enum Value {
            U8(u8),
            I8(i8),
            U16(u16),
            I16(i16),
            U32(u32),
            I32(i32),
            U64(u64),
            I64(i64),
            I128(i128),
            U128(u128),
            Bytes(Vec<u8>),
            Option(Option<Box<Value>>),
        }
        #[automatically_derived]
        impl ::core::clone::Clone for Value {
            #[inline]
            fn clone(&self) -> Value {
                match self {
                    Value::U8(__self_0) => {
                        Value::U8(::core::clone::Clone::clone(__self_0))
                    }
                    Value::I8(__self_0) => {
                        Value::I8(::core::clone::Clone::clone(__self_0))
                    }
                    Value::U16(__self_0) => {
                        Value::U16(::core::clone::Clone::clone(__self_0))
                    }
                    Value::I16(__self_0) => {
                        Value::I16(::core::clone::Clone::clone(__self_0))
                    }
                    Value::U32(__self_0) => {
                        Value::U32(::core::clone::Clone::clone(__self_0))
                    }
                    Value::I32(__self_0) => {
                        Value::I32(::core::clone::Clone::clone(__self_0))
                    }
                    Value::U64(__self_0) => {
                        Value::U64(::core::clone::Clone::clone(__self_0))
                    }
                    Value::I64(__self_0) => {
                        Value::I64(::core::clone::Clone::clone(__self_0))
                    }
                    Value::I128(__self_0) => {
                        Value::I128(::core::clone::Clone::clone(__self_0))
                    }
                    Value::U128(__self_0) => {
                        Value::U128(::core::clone::Clone::clone(__self_0))
                    }
                    Value::Bytes(__self_0) => {
                        Value::Bytes(::core::clone::Clone::clone(__self_0))
                    }
                    Value::Option(__self_0) => {
                        Value::Option(::core::clone::Clone::clone(__self_0))
                    }
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for Value {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match self {
                    Value::U8(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "U8",
                            &__self_0,
                        )
                    }
                    Value::I8(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "I8",
                            &__self_0,
                        )
                    }
                    Value::U16(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "U16",
                            &__self_0,
                        )
                    }
                    Value::I16(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "I16",
                            &__self_0,
                        )
                    }
                    Value::U32(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "U32",
                            &__self_0,
                        )
                    }
                    Value::I32(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "I32",
                            &__self_0,
                        )
                    }
                    Value::U64(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "U64",
                            &__self_0,
                        )
                    }
                    Value::I64(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "I64",
                            &__self_0,
                        )
                    }
                    Value::I128(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "I128",
                            &__self_0,
                        )
                    }
                    Value::U128(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "U128",
                            &__self_0,
                        )
                    }
                    Value::Bytes(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Bytes",
                            &__self_0,
                        )
                    }
                    Value::Option(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Option",
                            &__self_0,
                        )
                    }
                }
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for Value {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<u8>;
                let _: ::core::cmp::AssertParamIsEq<i8>;
                let _: ::core::cmp::AssertParamIsEq<u16>;
                let _: ::core::cmp::AssertParamIsEq<i16>;
                let _: ::core::cmp::AssertParamIsEq<u32>;
                let _: ::core::cmp::AssertParamIsEq<i32>;
                let _: ::core::cmp::AssertParamIsEq<u64>;
                let _: ::core::cmp::AssertParamIsEq<i64>;
                let _: ::core::cmp::AssertParamIsEq<i128>;
                let _: ::core::cmp::AssertParamIsEq<u128>;
                let _: ::core::cmp::AssertParamIsEq<Vec<u8>>;
                let _: ::core::cmp::AssertParamIsEq<Option<Box<Value>>>;
            }
        }
        impl BorshDeserialize for Value {
            fn deserialize_reader<R: Read>(
                reader: &mut R,
            ) -> borsh::maybestd::io::Result<Self> {
                let tag: u8 = u8::deserialize_reader(reader)?;
                match tag {
                    0 => Ok(Value::U8(u8::deserialize_reader(reader)?)),
                    1 => Ok(Value::I8(i8::deserialize_reader(reader)?)),
                    2 => Ok(Value::U16(u16::deserialize_reader(reader)?)),
                    3 => Ok(Value::I16(i16::deserialize_reader(reader)?)),
                    4 => Ok(Value::U32(u32::deserialize_reader(reader)?)),
                    5 => Ok(Value::I32(i32::deserialize_reader(reader)?)),
                    6 => Ok(Value::U64(u64::deserialize_reader(reader)?)),
                    7 => Ok(Value::I64(i64::deserialize_reader(reader)?)),
                    8 => Ok(Value::I128(i128::deserialize_reader(reader)?)),
                    9 => Ok(Value::U128(u128::deserialize_reader(reader)?)),
                    10 => Ok(Value::Bytes(Vec::<u8>::deserialize_reader(reader)?)),
                    11 => {
                        let flag = bool::deserialize_reader(reader)?;
                        if flag {
                            Ok(
                                Value::Option(
                                    Some(Box::new(Value::deserialize_reader(reader)?)),
                                ),
                            )
                        } else {
                            Ok(Value::Option(None))
                        }
                    }
                    _ => {
                        Err(
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid tag",
                            ),
                        )
                    }
                }
            }
        }
        impl BorshSerialize for Value {
            fn serialize<W: Write>(
                &self,
                writer: &mut W,
            ) -> borsh::maybestd::io::Result<()> {
                let tag: u8 = match self {
                    Value::U8(_) => 0,
                    Value::I8(_) => 1,
                    Value::U16(_) => 2,
                    Value::I16(_) => 3,
                    Value::U32(_) => 4,
                    Value::I32(_) => 5,
                    Value::U64(_) => 6,
                    Value::I64(_) => 7,
                    Value::I128(_) => 8,
                    Value::U128(_) => 9,
                    Value::Bytes(_) => 10,
                    Value::Option(_) => 11,
                };
                tag.serialize(writer)?;
                match self {
                    Value::U8(v) => v.serialize(writer),
                    Value::I8(v) => v.serialize(writer),
                    Value::U16(v) => v.serialize(writer),
                    Value::I16(v) => v.serialize(writer),
                    Value::U32(v) => v.serialize(writer),
                    Value::I32(v) => v.serialize(writer),
                    Value::U64(v) => v.serialize(writer),
                    Value::I64(v) => v.serialize(writer),
                    Value::I128(v) => v.serialize(writer),
                    Value::U128(v) => v.serialize(writer),
                    Value::Bytes(v) => v.serialize(writer),
                    Value::Option(v) => {
                        v.is_some().serialize(writer)?;
                        if let Some(v) = v { v.serialize(writer) } else { Ok(()) }
                    }
                }
            }
        }
        impl Value {
            pub fn as_u128(&self) -> u128 {
                match self {
                    Value::U8(v) => *v as u128,
                    Value::U16(v) => *v as u128,
                    Value::U32(v) => *v as u128,
                    Value::U64(v) => *v as u128,
                    Value::I8(v) => *v as u128,
                    Value::I16(v) => *v as u128,
                    Value::I32(v) => *v as u128,
                    Value::I64(v) => *v as u128,
                    Value::I128(v) => *v as u128,
                    Value::U128(v) => *v,
                    Value::Bytes(_) => {
                        ::core::panicking::panic_fmt(
                            format_args!("Cannot convert bytes to u128"),
                        );
                    }
                    Value::Option(v) => {
                        if let Some(v) = v {
                            v.as_u128()
                        } else {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Cannot convert None to u128"),
                                );
                            }
                        }
                    }
                }
            }
            pub fn as_i128(&self) -> i128 {
                match self {
                    Value::U8(v) => *v as i128,
                    Value::I8(v) => *v as i128,
                    Value::U16(v) => *v as i128,
                    Value::I16(v) => *v as i128,
                    Value::U32(v) => *v as i128,
                    Value::I32(v) => *v as i128,
                    Value::U64(v) => *v as i128,
                    Value::I64(v) => *v as i128,
                    Value::I128(v) => *v,
                    Value::U128(v) => *v as i128,
                    Value::Bytes(_) => {
                        ::core::panicking::panic_fmt(
                            format_args!("Cannot convert bytes to i128"),
                        );
                    }
                    Value::Option(v) => {
                        if let Some(v) = v {
                            v.as_i128()
                        } else {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Cannot convert None to i128"),
                                );
                            }
                        }
                    }
                }
            }
            pub fn as_bytes(&self) -> Vec<u8> {
                match self {
                    Value::U8(v) => v.to_le_bytes().to_vec(),
                    Value::I8(v) => v.to_le_bytes().to_vec(),
                    Value::U16(v) => v.to_le_bytes().to_vec(),
                    Value::I16(v) => v.to_le_bytes().to_vec(),
                    Value::U32(v) => v.to_le_bytes().to_vec(),
                    Value::I32(v) => v.to_le_bytes().to_vec(),
                    Value::U64(v) => v.to_le_bytes().to_vec(),
                    Value::I64(v) => v.to_le_bytes().to_vec(),
                    Value::I128(v) => v.to_le_bytes().to_vec(),
                    Value::U128(v) => v.to_le_bytes().to_vec(),
                    Value::Bytes(v) => v.clone(),
                    Value::Option(v) => {
                        if let Some(v) = v {
                            v.as_bytes()
                        } else {
                            ::alloc::vec::Vec::new()
                        }
                    }
                }
            }
            pub fn safe_cast(&self, target: ValueType) -> Result<Value, String> {
                let v = if self.is_unsigned() {
                    self.as_u128() as i128
                } else {
                    self.as_i128()
                };
                match target {
                    ValueType::U8 => {
                        if (0..=u8::MAX as i128).contains(&v) {
                            Ok(Value::U8(v as u8))
                        } else {
                            Err({
                                let res = ::alloc::fmt::format(
                                    format_args!("Out of range for U8: {0}", v),
                                );
                                res
                            })
                        }
                    }
                    ValueType::I8 => {
                        if (i8::MIN as i128..=i8::MAX as i128).contains(&v) {
                            Ok(Value::I8(v as i8))
                        } else {
                            Err("Out of range for I8".into())
                        }
                    }
                    ValueType::U16 => {
                        if (0..=u16::MAX as i128).contains(&v) {
                            Ok(Value::U16(v as u16))
                        } else {
                            Err("Out of range for U16".into())
                        }
                    }
                    ValueType::I16 => {
                        if (i16::MIN as i128..=i16::MAX as i128).contains(&v) {
                            Ok(Value::I16(v as i16))
                        } else {
                            Err("Out of range for I16".into())
                        }
                    }
                    ValueType::U32 => {
                        if (0..=u32::MAX as i128).contains(&v) {
                            Ok(Value::U32(v as u32))
                        } else {
                            Err("Out of range for U32".into())
                        }
                    }
                    ValueType::I32 => {
                        if (i32::MIN as i128..=i32::MAX as i128).contains(&v) {
                            Ok(Value::I32(v as i32))
                        } else {
                            Err("Out of range for I32".into())
                        }
                    }
                    ValueType::U64 => {
                        if (0..=u64::MAX as i128).contains(&v) {
                            Ok(Value::U64(v as u64))
                        } else {
                            Err("Out of range for U64".into())
                        }
                    }
                    ValueType::I64 => {
                        if (i64::MIN as i128..=i64::MAX as i128).contains(&v) {
                            Ok(Value::I64(v as i64))
                        } else {
                            Err("Out of range for I64".into())
                        }
                    }
                }
            }
            pub fn is_unsigned(&self) -> bool {
                match self {
                    Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) => true,
                    _ => false,
                }
            }
            pub fn is_signed(&self) -> bool {
                match self {
                    Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) => true,
                    _ => false,
                }
            }
            pub fn is_integer(&self) -> bool {
                self.is_unsigned() || self.is_signed()
            }
        }
        impl PartialEq for Value {
            fn eq(&self, right: &Self) -> bool {
                if self.is_integer() && right.is_integer() {
                    let left_signed = self.is_signed();
                    let right_signed = right.is_signed();
                    if left_signed && right_signed {
                        self.as_i128() == right.as_i128()
                    } else if left_signed && !right_signed {
                        self.as_i128() == right.as_u128() as i128
                    } else if !left_signed && right_signed {
                        self.as_u128() as i128 == right.as_i128()
                    } else {
                        self.as_u128() == right.as_u128()
                    }
                } else {
                    match (self, right) {
                        (Value::Bytes(a), Value::Bytes(b)) => a.eq(b),
                        (Value::Option(a), Value::Option(b)) => a.eq(b),
                        _ => false,
                    }
                }
            }
        }
        impl PartialOrd for Value {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                match (self, other) {
                    (Value::U8(a), Value::U8(b)) => a.partial_cmp(b),
                    (Value::I8(a), Value::I8(b)) => a.partial_cmp(b),
                    (Value::U16(a), Value::U16(b)) => a.partial_cmp(b),
                    (Value::I16(a), Value::I16(b)) => a.partial_cmp(b),
                    (Value::U32(a), Value::U32(b)) => a.partial_cmp(b),
                    (Value::I32(a), Value::I32(b)) => a.partial_cmp(b),
                    (Value::U64(a), Value::U64(b)) => a.partial_cmp(b),
                    (Value::I64(a), Value::I64(b)) => a.partial_cmp(b),
                    (Value::Bytes(a), Value::Bytes(b)) => a.partial_cmp(b),
                    (Value::Option(a), Value::Option(b)) => a.partial_cmp(b),
                    _ => {
                        if self.is_integer() && other.is_integer() {
                            self.as_u128().partial_cmp(&other.as_u128())
                        } else {
                            None
                        }
                    }
                }
            }
        }
        impl Value {
            pub fn checked_mul(&self, rhs: &Self) -> Result<Value, String> {
                match (self, rhs) {
                    (Value::U8(a), Value::U8(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::U8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I8(a), Value::I8(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::I8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U16(a), Value::U16(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::U16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I16(a), Value::I16(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::I16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U32(a), Value::U32(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::U32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I32(a), Value::I32(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::I32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U64(a), Value::U64(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::U64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I64(a), Value::I64(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::I64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U128(a), Value::U128(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::U128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I128(a), Value::I128(b)) => {
                        (*a)
                            .checked_mul(*b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    _ => {
                        let (a, b) = match (self, rhs) {
                            (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U128(b)) => (self.as_i128(), (*b as u128) as i128),
                            _ => (self.as_i128(), rhs.as_i128()),
                        };
                        a.checked_mul(b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked multiplication",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                }
            }
        }
        impl Value {
            pub fn checked_sub(&self, rhs: &Self) -> Result<Value, String> {
                match (self, rhs) {
                    (Value::U8(a), Value::U8(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::U8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I8(a), Value::I8(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::I8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U16(a), Value::U16(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::U16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I16(a), Value::I16(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::I16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U32(a), Value::U32(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::U32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I32(a), Value::I32(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::I32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U64(a), Value::U64(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::U64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I64(a), Value::I64(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::I64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U128(a), Value::U128(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::U128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I128(a), Value::I128(b)) => {
                        (*a)
                            .checked_sub(*b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    _ => {
                        let (a, b) = match (self, rhs) {
                            (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U128(b)) => (self.as_i128(), (*b as u128) as i128),
                            _ => (self.as_i128(), rhs.as_i128()),
                        };
                        a.checked_sub(b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked subtraction",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                }
            }
        }
        impl Value {
            pub fn checked_add(&self, rhs: &Self) -> Result<Value, String> {
                match (self, rhs) {
                    (Value::U8(a), Value::U8(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::U8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I8(a), Value::I8(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::I8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U16(a), Value::U16(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::U16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I16(a), Value::I16(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::I16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U32(a), Value::U32(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::U32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I32(a), Value::I32(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::I32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U64(a), Value::U64(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::U64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I64(a), Value::I64(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::I64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U128(a), Value::U128(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::U128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I128(a), Value::I128(b)) => {
                        (*a)
                            .checked_add(*b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    _ => {
                        let (a, b) = match (self, rhs) {
                            (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U128(b)) => (self.as_i128(), (*b as u128) as i128),
                            _ => (self.as_i128(), rhs.as_i128()),
                        };
                        a.checked_add(b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Overflow in checked addition",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                }
            }
        }
        impl Value {
            pub fn checked_div(&self, rhs: &Self) -> Result<Value, String> {
                match (self, rhs) {
                    (Value::U8(a), Value::U8(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::U8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I8(a), Value::I8(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::I8)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U16(a), Value::U16(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::U16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I16(a), Value::I16(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::I16)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U32(a), Value::U32(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::U32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I32(a), Value::I32(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::I32)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U64(a), Value::U64(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::U64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I64(a), Value::I64(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::I64)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::U128(a), Value::U128(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::U128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    (Value::I128(a), Value::I128(b)) => {
                        (*a)
                            .checked_div(*b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                    _ => {
                        let (a, b) = match (self, rhs) {
                            (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U128(b)) => (self.as_i128(), (*b as u128) as i128),
                            _ => (self.as_i128(), rhs.as_i128()),
                        };
                        a.checked_div(b)
                            .map(Value::I128)
                            .ok_or_else(|| {
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "{0} (left: {1:?}, right: {2:?})",
                                        "Division by zero or overflow in checked division",
                                        a,
                                        b,
                                    ),
                                );
                                res
                            })
                    }
                }
            }
        }
        impl Value {
            pub fn saturating_mul(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => {
                            Value::U8((*a).saturating_mul(*b))
                        }
                        (Value::I8(a), Value::I8(b)) => {
                            Value::I8((*a).saturating_mul(*b))
                        }
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).saturating_mul(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).saturating_mul(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).saturating_mul(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).saturating_mul(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).saturating_mul(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).saturating_mul(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).saturating_mul(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).saturating_mul(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.saturating_mul(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn saturating_add(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => {
                            Value::U8((*a).saturating_add(*b))
                        }
                        (Value::I8(a), Value::I8(b)) => {
                            Value::I8((*a).saturating_add(*b))
                        }
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).saturating_add(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).saturating_add(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).saturating_add(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).saturating_add(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).saturating_add(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).saturating_add(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).saturating_add(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).saturating_add(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.saturating_add(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn saturating_sub(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => {
                            Value::U8((*a).saturating_sub(*b))
                        }
                        (Value::I8(a), Value::I8(b)) => {
                            Value::I8((*a).saturating_sub(*b))
                        }
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).saturating_sub(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).saturating_sub(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).saturating_sub(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).saturating_sub(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).saturating_sub(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).saturating_sub(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).saturating_sub(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).saturating_sub(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.saturating_sub(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn saturating_div(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => {
                            Value::U8((*a).saturating_div(*b))
                        }
                        (Value::I8(a), Value::I8(b)) => {
                            Value::I8((*a).saturating_div(*b))
                        }
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).saturating_div(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).saturating_div(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).saturating_div(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).saturating_div(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).saturating_div(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).saturating_div(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).saturating_div(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).saturating_div(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.saturating_div(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn wrapping_mul(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => Value::U8((*a).wrapping_mul(*b)),
                        (Value::I8(a), Value::I8(b)) => Value::I8((*a).wrapping_mul(*b)),
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).wrapping_mul(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).wrapping_mul(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).wrapping_mul(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).wrapping_mul(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).wrapping_mul(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).wrapping_mul(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).wrapping_mul(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).wrapping_mul(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.wrapping_mul(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn wrapping_add(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => Value::U8((*a).wrapping_add(*b)),
                        (Value::I8(a), Value::I8(b)) => Value::I8((*a).wrapping_add(*b)),
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).wrapping_add(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).wrapping_add(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).wrapping_add(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).wrapping_add(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).wrapping_add(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).wrapping_add(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).wrapping_add(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).wrapping_add(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.wrapping_add(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn wrapping_sub(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => Value::U8((*a).wrapping_sub(*b)),
                        (Value::I8(a), Value::I8(b)) => Value::I8((*a).wrapping_sub(*b)),
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).wrapping_sub(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).wrapping_sub(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).wrapping_sub(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).wrapping_sub(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).wrapping_sub(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).wrapping_sub(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).wrapping_sub(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).wrapping_sub(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.wrapping_sub(b))
                        }
                    },
                )
            }
        }
        impl Value {
            pub fn wrapping_div(&self, rhs: &Self) -> Result<Value, String> {
                Ok(
                    match (self, rhs) {
                        (Value::U8(a), Value::U8(b)) => Value::U8((*a).wrapping_div(*b)),
                        (Value::I8(a), Value::I8(b)) => Value::I8((*a).wrapping_div(*b)),
                        (Value::U16(a), Value::U16(b)) => {
                            Value::U16((*a).wrapping_div(*b))
                        }
                        (Value::I16(a), Value::I16(b)) => {
                            Value::I16((*a).wrapping_div(*b))
                        }
                        (Value::U32(a), Value::U32(b)) => {
                            Value::U32((*a).wrapping_div(*b))
                        }
                        (Value::I32(a), Value::I32(b)) => {
                            Value::I32((*a).wrapping_div(*b))
                        }
                        (Value::U64(a), Value::U64(b)) => {
                            Value::U64((*a).wrapping_div(*b))
                        }
                        (Value::I64(a), Value::I64(b)) => {
                            Value::I64((*a).wrapping_div(*b))
                        }
                        (Value::U128(a), Value::U128(b)) => {
                            Value::U128((*a).wrapping_div(*b))
                        }
                        (Value::I128(a), Value::I128(b)) => {
                            Value::I128((*a).wrapping_div(*b))
                        }
                        _ => {
                            let (a, b) = match (self, rhs) {
                                (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                                (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
                                _ => (self.as_i128(), rhs.as_i128()),
                            };
                            Value::I128(a.wrapping_div(b))
                        }
                    },
                )
            }
        }
        impl From<Value> for Expression {
            fn from(value: Value) -> Self {
                Expression::Literal(value)
            }
        }
    }
}
