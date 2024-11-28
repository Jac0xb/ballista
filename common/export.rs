#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
pub mod accounts {
    use num_enum::{IntoPrimitive, TryFromPrimitive};
    pub mod task_definition {
        use geppetto::account;
        use crate::types::logical_components::{Expression, Value};
        use crate::types::task::command::Command;
        use borsh::{BorshDeserialize, BorshSerialize};
        use super::BallistaAccount;
        pub struct TaskDefinition {
            pub execution_settings: ExecutionSettings,
            pub actions: Vec<Command>,
            pub shared_values: Vec<Value>,
            pub account_groups: Vec<AccountGroupDefinition>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for TaskDefinition {
            #[inline]
            fn clone(&self) -> TaskDefinition {
                TaskDefinition {
                    execution_settings: ::core::clone::Clone::clone(
                        &self.execution_settings,
                    ),
                    actions: ::core::clone::Clone::clone(&self.actions),
                    shared_values: ::core::clone::Clone::clone(&self.shared_values),
                    account_groups: ::core::clone::Clone::clone(&self.account_groups),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for TaskDefinition {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field4_finish(
                    f,
                    "TaskDefinition",
                    "execution_settings",
                    &self.execution_settings,
                    "actions",
                    &self.actions,
                    "shared_values",
                    &self.shared_values,
                    "account_groups",
                    &&self.account_groups,
                )
            }
        }
        impl borsh::de::BorshDeserialize for TaskDefinition
        where
            ExecutionSettings: borsh::BorshDeserialize,
            Vec<Command>: borsh::BorshDeserialize,
            Vec<Value>: borsh::BorshDeserialize,
            Vec<AccountGroupDefinition>: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                Ok(Self {
                    execution_settings: borsh::BorshDeserialize::deserialize_reader(
                        reader,
                    )?,
                    actions: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    shared_values: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    account_groups: borsh::BorshDeserialize::deserialize_reader(reader)?,
                })
            }
        }
        impl borsh::ser::BorshSerialize for TaskDefinition
        where
            ExecutionSettings: borsh::ser::BorshSerialize,
            Vec<Command>: borsh::ser::BorshSerialize,
            Vec<Value>: borsh::ser::BorshSerialize,
            Vec<AccountGroupDefinition>: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                borsh::BorshSerialize::serialize(&self.execution_settings, writer)?;
                borsh::BorshSerialize::serialize(&self.actions, writer)?;
                borsh::BorshSerialize::serialize(&self.shared_values, writer)?;
                borsh::BorshSerialize::serialize(&self.account_groups, writer)?;
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for TaskDefinition {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<ExecutionSettings>;
                let _: ::core::cmp::AssertParamIsEq<Vec<Command>>;
                let _: ::core::cmp::AssertParamIsEq<Vec<Value>>;
                let _: ::core::cmp::AssertParamIsEq<Vec<AccountGroupDefinition>>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for TaskDefinition {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for TaskDefinition {
            #[inline]
            fn eq(&self, other: &TaskDefinition) -> bool {
                self.execution_settings == other.execution_settings
                    && self.actions == other.actions
                    && self.shared_values == other.shared_values
                    && self.account_groups == other.account_groups
            }
        }
        pub struct ExecutionSettings {
            pub preallocated_instruction_data_cache_size: Option<u16>,
            pub preallocated_account_meta_cache_size: Option<u16>,
            pub preallocated_account_info_cache_size: Option<u16>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for ExecutionSettings {
            #[inline]
            fn clone(&self) -> ExecutionSettings {
                ExecutionSettings {
                    preallocated_instruction_data_cache_size: ::core::clone::Clone::clone(
                        &self.preallocated_instruction_data_cache_size,
                    ),
                    preallocated_account_meta_cache_size: ::core::clone::Clone::clone(
                        &self.preallocated_account_meta_cache_size,
                    ),
                    preallocated_account_info_cache_size: ::core::clone::Clone::clone(
                        &self.preallocated_account_info_cache_size,
                    ),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ExecutionSettings {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field3_finish(
                    f,
                    "ExecutionSettings",
                    "preallocated_instruction_data_cache_size",
                    &self.preallocated_instruction_data_cache_size,
                    "preallocated_account_meta_cache_size",
                    &self.preallocated_account_meta_cache_size,
                    "preallocated_account_info_cache_size",
                    &&self.preallocated_account_info_cache_size,
                )
            }
        }
        impl borsh::de::BorshDeserialize for ExecutionSettings
        where
            Option<u16>: borsh::BorshDeserialize,
            Option<u16>: borsh::BorshDeserialize,
            Option<u16>: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                Ok(Self {
                    preallocated_instruction_data_cache_size: borsh::BorshDeserialize::deserialize_reader(
                        reader,
                    )?,
                    preallocated_account_meta_cache_size: borsh::BorshDeserialize::deserialize_reader(
                        reader,
                    )?,
                    preallocated_account_info_cache_size: borsh::BorshDeserialize::deserialize_reader(
                        reader,
                    )?,
                })
            }
        }
        impl borsh::ser::BorshSerialize for ExecutionSettings
        where
            Option<u16>: borsh::ser::BorshSerialize,
            Option<u16>: borsh::ser::BorshSerialize,
            Option<u16>: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                borsh::BorshSerialize::serialize(
                    &self.preallocated_instruction_data_cache_size,
                    writer,
                )?;
                borsh::BorshSerialize::serialize(
                    &self.preallocated_account_meta_cache_size,
                    writer,
                )?;
                borsh::BorshSerialize::serialize(
                    &self.preallocated_account_info_cache_size,
                    writer,
                )?;
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for ExecutionSettings {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<Option<u16>>;
                let _: ::core::cmp::AssertParamIsEq<Option<u16>>;
                let _: ::core::cmp::AssertParamIsEq<Option<u16>>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for ExecutionSettings {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for ExecutionSettings {
            #[inline]
            fn eq(&self, other: &ExecutionSettings) -> bool {
                self.preallocated_instruction_data_cache_size
                    == other.preallocated_instruction_data_cache_size
                    && self.preallocated_account_meta_cache_size
                        == other.preallocated_account_meta_cache_size
                    && self.preallocated_account_info_cache_size
                        == other.preallocated_account_info_cache_size
            }
        }
        #[automatically_derived]
        impl ::core::default::Default for ExecutionSettings {
            #[inline]
            fn default() -> ExecutionSettings {
                ExecutionSettings {
                    preallocated_instruction_data_cache_size: ::core::default::Default::default(),
                    preallocated_account_meta_cache_size: ::core::default::Default::default(),
                    preallocated_account_info_cache_size: ::core::default::Default::default(),
                }
            }
        }
        pub struct AccountGroupDefinition {
            pub account_offset: Expression,
            pub length: u8,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for AccountGroupDefinition {
            #[inline]
            fn clone(&self) -> AccountGroupDefinition {
                AccountGroupDefinition {
                    account_offset: ::core::clone::Clone::clone(&self.account_offset),
                    length: ::core::clone::Clone::clone(&self.length),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for AccountGroupDefinition {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field2_finish(
                    f,
                    "AccountGroupDefinition",
                    "account_offset",
                    &self.account_offset,
                    "length",
                    &&self.length,
                )
            }
        }
        impl borsh::de::BorshDeserialize for AccountGroupDefinition
        where
            Expression: borsh::BorshDeserialize,
            u8: borsh::BorshDeserialize,
        {
            fn deserialize_reader<R: borsh::maybestd::io::Read>(
                reader: &mut R,
            ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                Ok(Self {
                    account_offset: borsh::BorshDeserialize::deserialize_reader(reader)?,
                    length: borsh::BorshDeserialize::deserialize_reader(reader)?,
                })
            }
        }
        impl borsh::ser::BorshSerialize for AccountGroupDefinition
        where
            Expression: borsh::ser::BorshSerialize,
            u8: borsh::ser::BorshSerialize,
        {
            fn serialize<W: borsh::maybestd::io::Write>(
                &self,
                writer: &mut W,
            ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                borsh::BorshSerialize::serialize(&self.account_offset, writer)?;
                borsh::BorshSerialize::serialize(&self.length, writer)?;
                Ok(())
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for AccountGroupDefinition {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<Expression>;
                let _: ::core::cmp::AssertParamIsEq<u8>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for AccountGroupDefinition {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for AccountGroupDefinition {
            #[inline]
            fn eq(&self, other: &AccountGroupDefinition) -> bool {
                self.account_offset == other.account_offset
                    && self.length == other.length
            }
        }
        impl TaskDefinition
        where
            Self: borsh::BorshSerialize,
        {
            pub fn to_bytes(&self) -> Vec<u8> {
                borsh::to_vec(self).unwrap()
            }
        }
        impl ::geppetto::Discriminator for TaskDefinition {
            fn discriminator() -> u8 {
                BallistaAccount::TaskDefinition.into()
            }
        }
        impl ::geppetto::AccountValidation for TaskDefinition {
            fn assert<F>(
                &self,
                condition: F,
            ) -> Result<&Self, pinocchio::program_error::ProgramError>
            where
                F: Fn(&Self) -> bool,
            {
                if !condition(self) {
                    return Err(
                        pinocchio::program_error::ProgramError::InvalidAccountData,
                    );
                }
                Ok(self)
            }
            fn assert_err<F>(
                &self,
                condition: F,
                err: pinocchio::program_error::ProgramError,
            ) -> Result<&Self, pinocchio::program_error::ProgramError>
            where
                F: Fn(&Self) -> bool,
            {
                if !condition(self) {
                    return Err(err);
                }
                Ok(self)
            }
            fn assert_msg<F>(
                &self,
                condition: F,
                msg: &str,
            ) -> Result<&Self, pinocchio::program_error::ProgramError>
            where
                F: Fn(&Self) -> bool,
            {
                match ::geppetto::assert(
                    condition(self),
                    pinocchio::program_error::ProgramError::InvalidAccountData,
                    msg,
                ) {
                    Err(err) => Err(err.into()),
                    Ok(()) => Ok(self),
                }
            }
            fn assert_mut<F>(
                &mut self,
                condition: F,
            ) -> Result<&mut Self, pinocchio::program_error::ProgramError>
            where
                F: Fn(&Self) -> bool,
            {
                if !condition(self) {
                    return Err(
                        pinocchio::program_error::ProgramError::InvalidAccountData,
                    );
                }
                Ok(self)
            }
            fn assert_mut_err<F>(
                &mut self,
                condition: F,
                err: pinocchio::program_error::ProgramError,
            ) -> Result<&mut Self, pinocchio::program_error::ProgramError>
            where
                F: Fn(&Self) -> bool,
            {
                if !condition(self) {
                    return Err(err);
                }
                Ok(self)
            }
            fn assert_mut_msg<F>(
                &mut self,
                condition: F,
                msg: &str,
            ) -> Result<&mut Self, pinocchio::program_error::ProgramError>
            where
                F: Fn(&Self) -> bool,
            {
                match ::geppetto::assert(
                    condition(self),
                    pinocchio::program_error::ProgramError::InvalidAccountData,
                    msg,
                ) {
                    Err(err) => Err(err.into()),
                    Ok(()) => Ok(self),
                }
            }
        }
    }
    #[repr(u8)]
    pub enum BallistaAccount {
        TaskDefinition = 0,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for BallistaAccount {
        #[inline]
        fn clone(&self) -> BallistaAccount {
            *self
        }
    }
    #[automatically_derived]
    impl ::core::marker::Copy for BallistaAccount {}
    #[automatically_derived]
    impl ::core::fmt::Debug for BallistaAccount {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::write_str(f, "TaskDefinition")
        }
    }
    #[automatically_derived]
    impl ::core::cmp::Eq for BallistaAccount {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {}
    }
    #[automatically_derived]
    impl ::core::marker::StructuralPartialEq for BallistaAccount {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for BallistaAccount {
        #[inline]
        fn eq(&self, other: &BallistaAccount) -> bool {
            true
        }
    }
    impl From<BallistaAccount> for u8 {
        #[inline]
        fn from(enum_value: BallistaAccount) -> Self {
            enum_value as Self
        }
    }
    impl ::num_enum::TryFromPrimitive for BallistaAccount {
        type Primitive = u8;
        type Error = ::num_enum::TryFromPrimitiveError<Self>;
        const NAME: &'static str = "BallistaAccount";
        fn try_from_primitive(
            number: Self::Primitive,
        ) -> ::core::result::Result<Self, ::num_enum::TryFromPrimitiveError<Self>> {
            #![allow(non_upper_case_globals)]
            const TaskDefinition__num_enum_0__: u8 = 0;
            #[deny(unreachable_patterns)]
            match number {
                TaskDefinition__num_enum_0__ => {
                    ::core::result::Result::Ok(Self::TaskDefinition)
                }
                #[allow(unreachable_patterns)]
                _ => {
                    ::core::result::Result::Err(
                        ::num_enum::TryFromPrimitiveError::<Self>::new(number),
                    )
                }
            }
        }
    }
    impl ::core::convert::TryFrom<u8> for BallistaAccount {
        type Error = ::num_enum::TryFromPrimitiveError<Self>;
        #[inline]
        fn try_from(
            number: u8,
        ) -> ::core::result::Result<Self, ::num_enum::TryFromPrimitiveError<Self>> {
            ::num_enum::TryFromPrimitive::try_from_primitive(number)
        }
    }
    #[doc(hidden)]
    impl ::num_enum::CannotDeriveBothFromPrimitiveAndTryFromPrimitive
    for BallistaAccount {}
}
pub mod types {
    pub mod execution_frame {
        use super::task::command::Command;
        pub struct ExecutionFrame<'a> {
            pub current_index: u8,
            pub actions: &'a dyn ActionCollection,
        }
        impl<'a> ExecutionFrame<'a> {
            #[inline(always)]
            pub fn increment(&mut self, increment: u8) {
                self.current_index += increment;
            }
            #[inline(always)]
            pub fn set_index(&mut self, index: u8) {
                self.current_index = index;
            }
            #[inline(always)]
            pub fn get_index(&self) -> u8 {
                self.current_index
            }
            #[inline(always)]
            pub fn get_current_action(&self) -> &Command {
                self.actions.get(self.current_index as usize)
            }
            #[inline(always)]
            pub fn get_action(&self, index: u8) -> &Command {
                self.actions.get(index as usize)
            }
            #[inline(always)]
            pub fn actions_len(&self) -> usize {
                self.actions.len()
            }
        }
        pub trait ActionCollection {
            fn len(&self) -> usize;
            fn is_empty(&self) -> bool;
            fn get(&self, index: usize) -> &Command;
        }
        impl ActionCollection for [Command] {
            fn len(&self) -> usize {
                self.len()
            }
            fn is_empty(&self) -> bool {
                self.is_empty()
            }
            fn get(&self, index: usize) -> &Command {
                &self[index]
            }
        }
        impl ActionCollection for &[Command] {
            fn len(&self) -> usize {
                (*self).len()
            }
            fn is_empty(&self) -> bool {
                (*self).is_empty()
            }
            fn get(&self, index: usize) -> &Command {
                &self[index]
            }
        }
        impl ActionCollection for &[&Command] {
            fn len(&self) -> usize {
                (*self).len()
            }
            fn is_empty(&self) -> bool {
                (*self).is_empty()
            }
            fn get(&self, index: usize) -> &Command {
                self[index]
            }
        }
        impl ActionCollection for &[Box<Command>] {
            fn len(&self) -> usize {
                (*self).len()
            }
            fn is_empty(&self) -> bool {
                (*self).is_empty()
            }
            fn get(&self, index: usize) -> &Command {
                &(*self)[index]
            }
        }
        impl ActionCollection for Box<Command> {
            fn len(&self) -> usize {
                1
            }
            fn is_empty(&self) -> bool {
                false
            }
            fn get(&self, _index: usize) -> &Command {
                self
            }
        }
    }
    pub mod logical_components {
        pub mod condition {
            use super::{Expression, Validation};
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
                        Condition::Validation(__self_0) => {
                            Condition::Validation(::core::clone::Clone::clone(__self_0))
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
                        Condition::Validation(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Validation",
                                &__self_0,
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
                    let _: ::core::cmp::AssertParamIsEq<Validation>;
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
                            (
                                Condition::Validation(__self_0),
                                Condition::Validation(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl Condition {
                pub fn less_than(left: Expression, right: Expression) -> Condition {
                    Condition::LessThan(left, right)
                }
                pub fn less_than_or_equal(
                    left: Expression,
                    right: Expression,
                ) -> Condition {
                    Condition::LessThanOrEqual(left, right)
                }
                pub fn greater_than(left: Expression, right: Expression) -> Condition {
                    Condition::GreaterThan(left, right)
                }
                pub fn greater_than_or_equal(
                    left: Expression,
                    right: Expression,
                ) -> Condition {
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
        }
        pub mod expression {
            use crate::types::task::task_account::TaskAccount;
            use super::{Value, ValueType};
            use borsh::{BorshDeserialize, BorshSerialize};
            use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};
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
            pub enum AccountInfoType {
                Key,
                Lamports,
                DataLength,
                Owner,
                KnownOwner,
                RentEpoch,
                IsSigner,
                IsWritable,
                Executable,
            }
            impl borsh::de::BorshDeserialize for AccountInfoType {
                fn deserialize_reader<R: borsh::maybestd::io::Read>(
                    reader: &mut R,
                ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                    let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                        reader,
                    )?;
                    <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
                }
            }
            impl borsh::de::EnumExt for AccountInfoType {
                fn deserialize_variant<R: borsh::maybestd::io::Read>(
                    reader: &mut R,
                    variant_idx: u8,
                ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                    let mut return_value = match variant_idx {
                        0u8 => AccountInfoType::Key,
                        1u8 => AccountInfoType::Lamports,
                        2u8 => AccountInfoType::DataLength,
                        3u8 => AccountInfoType::Owner,
                        4u8 => AccountInfoType::KnownOwner,
                        5u8 => AccountInfoType::RentEpoch,
                        6u8 => AccountInfoType::IsSigner,
                        7u8 => AccountInfoType::IsWritable,
                        8u8 => AccountInfoType::Executable,
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
            impl borsh::ser::BorshSerialize for AccountInfoType {
                fn serialize<W: borsh::maybestd::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                    let variant_idx: u8 = match self {
                        AccountInfoType::Key => 0u8,
                        AccountInfoType::Lamports => 1u8,
                        AccountInfoType::DataLength => 2u8,
                        AccountInfoType::Owner => 3u8,
                        AccountInfoType::KnownOwner => 4u8,
                        AccountInfoType::RentEpoch => 5u8,
                        AccountInfoType::IsSigner => 6u8,
                        AccountInfoType::IsWritable => 7u8,
                        AccountInfoType::Executable => 8u8,
                    };
                    writer.write_all(&variant_idx.to_le_bytes())?;
                    match self {
                        AccountInfoType::Key => {}
                        AccountInfoType::Lamports => {}
                        AccountInfoType::DataLength => {}
                        AccountInfoType::Owner => {}
                        AccountInfoType::KnownOwner => {}
                        AccountInfoType::RentEpoch => {}
                        AccountInfoType::IsSigner => {}
                        AccountInfoType::IsWritable => {}
                        AccountInfoType::Executable => {}
                    }
                    Ok(())
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for AccountInfoType {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(
                        f,
                        match self {
                            AccountInfoType::Key => "Key",
                            AccountInfoType::Lamports => "Lamports",
                            AccountInfoType::DataLength => "DataLength",
                            AccountInfoType::Owner => "Owner",
                            AccountInfoType::KnownOwner => "KnownOwner",
                            AccountInfoType::RentEpoch => "RentEpoch",
                            AccountInfoType::IsSigner => "IsSigner",
                            AccountInfoType::IsWritable => "IsWritable",
                            AccountInfoType::Executable => "Executable",
                        },
                    )
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for AccountInfoType {
                #[inline]
                fn clone(&self) -> AccountInfoType {
                    match self {
                        AccountInfoType::Key => AccountInfoType::Key,
                        AccountInfoType::Lamports => AccountInfoType::Lamports,
                        AccountInfoType::DataLength => AccountInfoType::DataLength,
                        AccountInfoType::Owner => AccountInfoType::Owner,
                        AccountInfoType::KnownOwner => AccountInfoType::KnownOwner,
                        AccountInfoType::RentEpoch => AccountInfoType::RentEpoch,
                        AccountInfoType::IsSigner => AccountInfoType::IsSigner,
                        AccountInfoType::IsWritable => AccountInfoType::IsWritable,
                        AccountInfoType::Executable => AccountInfoType::Executable,
                    }
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for AccountInfoType {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for AccountInfoType {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for AccountInfoType {
                #[inline]
                fn eq(&self, other: &AccountInfoType) -> bool {
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
                ValueFromAccountData {
                    index: Box<TaskAccount>,
                    offset: Box<Expression>,
                    value_type: ValueType,
                },
                ValueFromAccountInfo {
                    index: Box<TaskAccount>,
                    field_name: AccountInfoType,
                },
                SafeCast(Box<Expression>, ValueType),
                Multiply(Box<Expression>, Box<Expression>, ArithmeticBehavior),
                Divide(Box<Expression>, Box<Expression>, ArithmeticBehavior),
                Add(Box<Expression>, Box<Expression>, ArithmeticBehavior),
                Subtract(Box<Expression>, Box<Expression>, ArithmeticBehavior),
                Rent(Box<Expression>),
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
                            Expression::StaticValue(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        Expression::CachedValue(__self_0) => {
                            Expression::CachedValue(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        Expression::ValueFromAccountData {
                            index: __self_0,
                            offset: __self_1,
                            value_type: __self_2,
                        } => {
                            Expression::ValueFromAccountData {
                                index: ::core::clone::Clone::clone(__self_0),
                                offset: ::core::clone::Clone::clone(__self_1),
                                value_type: ::core::clone::Clone::clone(__self_2),
                            }
                        }
                        Expression::ValueFromAccountInfo {
                            index: __self_0,
                            field_name: __self_1,
                        } => {
                            Expression::ValueFromAccountInfo {
                                index: ::core::clone::Clone::clone(__self_0),
                                field_name: ::core::clone::Clone::clone(__self_1),
                            }
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
                        Expression::Rent(__self_0) => {
                            Expression::Rent(::core::clone::Clone::clone(__self_0))
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
                        Expression::ValueFromAccountData {
                            index: __self_0,
                            offset: __self_1,
                            value_type: __self_2,
                        } => {
                            ::core::fmt::Formatter::debug_struct_field3_finish(
                                f,
                                "ValueFromAccountData",
                                "index",
                                __self_0,
                                "offset",
                                __self_1,
                                "value_type",
                                &__self_2,
                            )
                        }
                        Expression::ValueFromAccountInfo {
                            index: __self_0,
                            field_name: __self_1,
                        } => {
                            ::core::fmt::Formatter::debug_struct_field2_finish(
                                f,
                                "ValueFromAccountInfo",
                                "index",
                                __self_0,
                                "field_name",
                                &__self_1,
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
                        Expression::Rent(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Rent",
                                &__self_0,
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
                    let _: ::core::cmp::AssertParamIsEq<Box<TaskAccount>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<ValueType>;
                    let _: ::core::cmp::AssertParamIsEq<Box<TaskAccount>>;
                    let _: ::core::cmp::AssertParamIsEq<AccountInfoType>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<ArithmeticBehavior>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
                    let _: ::core::cmp::AssertParamIsEq<Box<Expression>>;
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
                                Expression::ValueFromAccountData {
                                    index: __self_0,
                                    offset: __self_1,
                                    value_type: __self_2,
                                },
                                Expression::ValueFromAccountData {
                                    index: __arg1_0,
                                    offset: __arg1_1,
                                    value_type: __arg1_2,
                                },
                            ) => {
                                *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                    && *__self_2 == *__arg1_2
                            }
                            (
                                Expression::ValueFromAccountInfo {
                                    index: __self_0,
                                    field_name: __self_1,
                                },
                                Expression::ValueFromAccountInfo {
                                    index: __arg1_0,
                                    field_name: __arg1_1,
                                },
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
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
                            (Expression::Rent(__self_0), Expression::Rent(__arg1_0)) => {
                                *__self_0 == *__arg1_0
                            }
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl borsh::BorshDeserialize for Expression
            where
                Value: borsh::BorshDeserialize,
                ArithmeticBehavior: borsh::BorshDeserialize,
                Box<TaskAccount>: borsh::BorshDeserialize,
                Box<Expression>: borsh::BorshDeserialize,
                ValueType: borsh::BorshDeserialize,
                AccountInfoType: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
            {
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
                            Ok(Expression::ValueFromAccountData {
                                index: Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                offset: Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                value_type: borsh::BorshDeserialize::deserialize_reader(
                                    reader,
                                )?,
                            })
                        }
                        5u8 => {
                            Ok(Expression::ValueFromAccountInfo {
                                index: Box::new(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                                field_name: borsh::BorshDeserialize::deserialize_reader(
                                    reader,
                                )?,
                            })
                        }
                        6u8 => {
                            Ok(
                                Expression::SafeCast(
                                    Box::new(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        7u8 => {
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
                        8u8 => {
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
                        9u8 => {
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
                        10u8 => {
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
                        11u8 => {
                            Ok(
                                Expression::Rent(
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
                                    {
                                        let res = ::alloc::fmt::format(
                                            format_args!("Invalid tag for enum {0}", "Expression"),
                                        );
                                        res
                                    },
                                ),
                            )
                        }
                    }
                }
            }
            impl borsh::BorshSerialize for Expression
            where
                ValueType: borsh::BorshSerialize,
                Box<Expression>: borsh::BorshSerialize,
                ArithmeticBehavior: borsh::BorshSerialize,
                Value: borsh::BorshSerialize,
                u8: borsh::BorshSerialize,
                Box<TaskAccount>: borsh::BorshSerialize,
                AccountInfoType: borsh::BorshSerialize,
            {
                fn serialize<W: std::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> std::io::Result<()> {
                    match self {
                        Expression::Literal(field0) => {
                            borsh::BorshSerialize::serialize(&0u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Expression::InputValue(field0) => {
                            borsh::BorshSerialize::serialize(&1u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Expression::StaticValue(field0) => {
                            borsh::BorshSerialize::serialize(&2u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Expression::CachedValue(field0) => {
                            borsh::BorshSerialize::serialize(&3u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Expression::ValueFromAccountData {
                            index,
                            offset,
                            value_type,
                        } => {
                            borsh::BorshSerialize::serialize(&4u8, writer)?;
                            borsh::BorshSerialize::serialize(&**index, writer)?;
                            borsh::BorshSerialize::serialize(&**offset, writer)?;
                            borsh::BorshSerialize::serialize(&value_type, writer)?;
                            Ok(())
                        }
                        Expression::ValueFromAccountInfo { index, field_name } => {
                            borsh::BorshSerialize::serialize(&5u8, writer)?;
                            borsh::BorshSerialize::serialize(&**index, writer)?;
                            borsh::BorshSerialize::serialize(&field_name, writer)?;
                            Ok(())
                        }
                        Expression::SafeCast(field0, field1) => {
                            borsh::BorshSerialize::serialize(&6u8, writer)?;
                            borsh::BorshSerialize::serialize(&**field0, writer)?;
                            borsh::BorshSerialize::serialize(&field1, writer)?;
                            Ok(())
                        }
                        Expression::Multiply(field0, field1, field2) => {
                            borsh::BorshSerialize::serialize(&7u8, writer)?;
                            borsh::BorshSerialize::serialize(&**field0, writer)?;
                            borsh::BorshSerialize::serialize(&**field1, writer)?;
                            borsh::BorshSerialize::serialize(&field2, writer)?;
                            Ok(())
                        }
                        Expression::Divide(field0, field1, field2) => {
                            borsh::BorshSerialize::serialize(&8u8, writer)?;
                            borsh::BorshSerialize::serialize(&**field0, writer)?;
                            borsh::BorshSerialize::serialize(&**field1, writer)?;
                            borsh::BorshSerialize::serialize(&field2, writer)?;
                            Ok(())
                        }
                        Expression::Add(field0, field1, field2) => {
                            borsh::BorshSerialize::serialize(&9u8, writer)?;
                            borsh::BorshSerialize::serialize(&**field0, writer)?;
                            borsh::BorshSerialize::serialize(&**field1, writer)?;
                            borsh::BorshSerialize::serialize(&field2, writer)?;
                            Ok(())
                        }
                        Expression::Subtract(field0, field1, field2) => {
                            borsh::BorshSerialize::serialize(&10u8, writer)?;
                            borsh::BorshSerialize::serialize(&**field0, writer)?;
                            borsh::BorshSerialize::serialize(&**field1, writer)?;
                            borsh::BorshSerialize::serialize(&field2, writer)?;
                            Ok(())
                        }
                        Expression::Rent(field0) => {
                            borsh::BorshSerialize::serialize(&11u8, writer)?;
                            borsh::BorshSerialize::serialize(&**field0, writer)?;
                            Ok(())
                        }
                    }
                }
            }
            impl Expression {
                pub fn literal(value: Value) -> Self {
                    Expression::Literal(value)
                }
                pub fn input(index: u8) -> Self {
                    Expression::InputValue(index)
                }
                pub fn shared_value(index: u8) -> Self {
                    Expression::StaticValue(index)
                }
                pub fn cached_value(index: u8) -> Self {
                    Expression::CachedValue(index)
                }
                pub fn safe_cast(&self, target_type: ValueType) -> Self {
                    Expression::SafeCast(Box::new(self.clone()), target_type)
                }
                pub fn safe_cast_to_u64(&self) -> Self {
                    Expression::SafeCast(Box::new(self.clone()), ValueType::U64)
                }
                pub fn multiply(
                    &self,
                    right: Expression,
                    behavior: ArithmeticBehavior,
                ) -> Self {
                    Expression::Multiply(
                        Box::new(self.clone()),
                        Box::new(right),
                        behavior,
                    )
                }
                pub fn checked_multiply(&self, right: Expression) -> Self {
                    Expression::Multiply(
                        Box::new(self.clone()),
                        Box::new(right),
                        ArithmeticBehavior::Checked,
                    )
                }
                pub fn divide(
                    &self,
                    right: Expression,
                    behavior: ArithmeticBehavior,
                ) -> Self {
                    Expression::Divide(Box::new(self.clone()), Box::new(right), behavior)
                }
                pub fn add(
                    &self,
                    right: Expression,
                    behavior: ArithmeticBehavior,
                ) -> Self {
                    Expression::Add(Box::new(self.clone()), Box::new(right), behavior)
                }
                pub fn checked_add(&self, right: &Expression) -> Self {
                    Expression::Add(
                        Box::new(self.clone()),
                        Box::new(right.clone()),
                        ArithmeticBehavior::Checked,
                    )
                }
                pub fn subtract(
                    &self,
                    right: Expression,
                    behavior: ArithmeticBehavior,
                ) -> Self {
                    Expression::Subtract(
                        Box::new(self.clone()),
                        Box::new(right),
                        behavior,
                    )
                }
            }
            impl From<Value> for Expression {
                fn from(value: Value) -> Self {
                    Expression::Literal(value)
                }
            }
        }
        pub mod validation {
            use borsh::{BorshDeserialize, BorshSerialize};
            use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};
            use crate::types::task::task_account::TaskAccount;
            pub enum Validation {
                IsTokenAccount(TaskAccount),
                IsEmpty(TaskAccount),
            }
            #[automatically_derived]
            impl ::core::clone::Clone for Validation {
                #[inline]
                fn clone(&self) -> Validation {
                    match self {
                        Validation::IsTokenAccount(__self_0) => {
                            Validation::IsTokenAccount(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        Validation::IsEmpty(__self_0) => {
                            Validation::IsEmpty(::core::clone::Clone::clone(__self_0))
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for Validation {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        Validation::IsTokenAccount(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "IsTokenAccount",
                                &__self_0,
                            )
                        }
                        Validation::IsEmpty(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "IsEmpty",
                                &__self_0,
                            )
                        }
                    }
                }
            }
            impl borsh::BorshDeserialize for Validation
            where
                TaskAccount: borsh::BorshDeserialize,
            {
                fn deserialize_reader<R: std::io::Read>(
                    reader: &mut R,
                ) -> std::io::Result<Self> {
                    let tag = u8::deserialize_reader(reader)?;
                    match tag {
                        0u8 => {
                            Ok(
                                Validation::IsTokenAccount(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        1u8 => {
                            Ok(
                                Validation::IsEmpty(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        _ => {
                            Err(
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    {
                                        let res = ::alloc::fmt::format(
                                            format_args!("Invalid tag for enum {0}", "Validation"),
                                        );
                                        res
                                    },
                                ),
                            )
                        }
                    }
                }
            }
            impl borsh::BorshSerialize for Validation
            where
                TaskAccount: borsh::BorshSerialize,
            {
                fn serialize<W: std::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> std::io::Result<()> {
                    match self {
                        Validation::IsTokenAccount(field0) => {
                            borsh::BorshSerialize::serialize(&0u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Validation::IsEmpty(field0) => {
                            borsh::BorshSerialize::serialize(&1u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for Validation {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                }
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for Validation {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for Validation {
                #[inline]
                fn eq(&self, other: &Validation) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                Validation::IsTokenAccount(__self_0),
                                Validation::IsTokenAccount(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                Validation::IsEmpty(__self_0),
                                Validation::IsEmpty(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
        }
        pub mod value {
            use borsh::{BorshDeserialize, BorshSerialize};
            use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};
            use crate::types::task::command::defined_instruction::SerializationType;
            use super::Expression;
            pub enum ValueType {
                U8,
                I8,
                U16,
                I16,
                U32,
                I32,
                U64,
                I64,
                I128,
                U128,
                Bytes,
                Vec,
                Option,
                Struct,
                String,
                Bool,
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
                        ValueType::I128 => ValueType::I128,
                        ValueType::U128 => ValueType::U128,
                        ValueType::Bytes => ValueType::Bytes,
                        ValueType::Vec => ValueType::Vec,
                        ValueType::Option => ValueType::Option,
                        ValueType::Struct => ValueType::Struct,
                        ValueType::String => ValueType::String,
                        ValueType::Bool => ValueType::Bool,
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
                            ValueType::I128 => "I128",
                            ValueType::U128 => "U128",
                            ValueType::Bytes => "Bytes",
                            ValueType::Vec => "Vec",
                            ValueType::Option => "Option",
                            ValueType::Struct => "Struct",
                            ValueType::String => "String",
                            ValueType::Bool => "Bool",
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
                        8u8 => ValueType::I128,
                        9u8 => ValueType::U128,
                        10u8 => ValueType::Bytes,
                        11u8 => ValueType::Vec,
                        12u8 => ValueType::Option,
                        13u8 => ValueType::Struct,
                        14u8 => ValueType::String,
                        15u8 => ValueType::Bool,
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
                        ValueType::I128 => 8u8,
                        ValueType::U128 => 9u8,
                        ValueType::Bytes => 10u8,
                        ValueType::Vec => 11u8,
                        ValueType::Option => 12u8,
                        ValueType::Struct => 13u8,
                        ValueType::String => 14u8,
                        ValueType::Bool => 15u8,
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
                        ValueType::I128 => {}
                        ValueType::U128 => {}
                        ValueType::Bytes => {}
                        ValueType::Vec => {}
                        ValueType::Option => {}
                        ValueType::Struct => {}
                        ValueType::String => {}
                        ValueType::Bool => {}
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
                Vec(Vec<Value>),
                Option(Option<Box<Value>>),
                Struct(Vec<Value>),
                String(String),
                Pubkey([u8; 32]),
                Bool(bool),
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
                        Value::Vec(__self_0) => {
                            Value::Vec(::core::clone::Clone::clone(__self_0))
                        }
                        Value::Option(__self_0) => {
                            Value::Option(::core::clone::Clone::clone(__self_0))
                        }
                        Value::Struct(__self_0) => {
                            Value::Struct(::core::clone::Clone::clone(__self_0))
                        }
                        Value::String(__self_0) => {
                            Value::String(::core::clone::Clone::clone(__self_0))
                        }
                        Value::Pubkey(__self_0) => {
                            Value::Pubkey(::core::clone::Clone::clone(__self_0))
                        }
                        Value::Bool(__self_0) => {
                            Value::Bool(::core::clone::Clone::clone(__self_0))
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
                        Value::Vec(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Vec",
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
                        Value::Struct(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Struct",
                                &__self_0,
                            )
                        }
                        Value::String(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "String",
                                &__self_0,
                            )
                        }
                        Value::Pubkey(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Pubkey",
                                &__self_0,
                            )
                        }
                        Value::Bool(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Bool",
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
                    let _: ::core::cmp::AssertParamIsEq<Vec<Value>>;
                    let _: ::core::cmp::AssertParamIsEq<Option<Box<Value>>>;
                    let _: ::core::cmp::AssertParamIsEq<Vec<Value>>;
                    let _: ::core::cmp::AssertParamIsEq<String>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<bool>;
                }
            }
            impl borsh::BorshDeserialize for Value
            where
                Vec<Value>: borsh::BorshDeserialize,
                i8: borsh::BorshDeserialize,
                i64: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
                Option<Box<Value>>: borsh::BorshDeserialize,
                [u8; 32]: borsh::BorshDeserialize,
                i16: borsh::BorshDeserialize,
                u16: borsh::BorshDeserialize,
                u128: borsh::BorshDeserialize,
                Vec<u8>: borsh::BorshDeserialize,
                bool: borsh::BorshDeserialize,
                i32: borsh::BorshDeserialize,
                u64: borsh::BorshDeserialize,
                u32: borsh::BorshDeserialize,
                i128: borsh::BorshDeserialize,
                String: borsh::BorshDeserialize,
            {
                fn deserialize_reader<R: std::io::Read>(
                    reader: &mut R,
                ) -> std::io::Result<Self> {
                    let tag = u8::deserialize_reader(reader)?;
                    match tag {
                        0u8 => {
                            Ok(
                                Value::U8(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        1u8 => {
                            Ok(
                                Value::I8(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        2u8 => {
                            Ok(
                                Value::U16(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        3u8 => {
                            Ok(
                                Value::I16(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        4u8 => {
                            Ok(
                                Value::U32(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        5u8 => {
                            Ok(
                                Value::I32(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        6u8 => {
                            Ok(
                                Value::U64(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        7u8 => {
                            Ok(
                                Value::I64(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        8u8 => {
                            Ok(
                                Value::I128(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        9u8 => {
                            Ok(
                                Value::U128(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        10u8 => {
                            Ok(
                                Value::Bytes(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        11u8 => {
                            Ok(
                                Value::Vec(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        12u8 => {
                            Ok(
                                Value::Option(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        13u8 => {
                            Ok(
                                Value::Struct(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        14u8 => {
                            Ok(
                                Value::String(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        15u8 => {
                            Ok(
                                Value::Pubkey(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        16u8 => {
                            Ok(
                                Value::Bool(
                                    borsh::BorshDeserialize::deserialize_reader(reader)?,
                                ),
                            )
                        }
                        _ => {
                            Err(
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    {
                                        let res = ::alloc::fmt::format(
                                            format_args!("Invalid tag for enum {0}", "Value"),
                                        );
                                        res
                                    },
                                ),
                            )
                        }
                    }
                }
            }
            impl borsh::BorshSerialize for Value
            where
                i16: borsh::BorshSerialize,
                u128: borsh::BorshSerialize,
                [u8; 32]: borsh::BorshSerialize,
                String: borsh::BorshSerialize,
                i32: borsh::BorshSerialize,
                Vec<Value>: borsh::BorshSerialize,
                i8: borsh::BorshSerialize,
                u16: borsh::BorshSerialize,
                Option<Box<Value>>: borsh::BorshSerialize,
                i64: borsh::BorshSerialize,
                u32: borsh::BorshSerialize,
                u8: borsh::BorshSerialize,
                i128: borsh::BorshSerialize,
                bool: borsh::BorshSerialize,
                u64: borsh::BorshSerialize,
                Vec<u8>: borsh::BorshSerialize,
            {
                fn serialize<W: std::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> std::io::Result<()> {
                    match self {
                        Value::U8(field0) => {
                            borsh::BorshSerialize::serialize(&0u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::I8(field0) => {
                            borsh::BorshSerialize::serialize(&1u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::U16(field0) => {
                            borsh::BorshSerialize::serialize(&2u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::I16(field0) => {
                            borsh::BorshSerialize::serialize(&3u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::U32(field0) => {
                            borsh::BorshSerialize::serialize(&4u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::I32(field0) => {
                            borsh::BorshSerialize::serialize(&5u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::U64(field0) => {
                            borsh::BorshSerialize::serialize(&6u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::I64(field0) => {
                            borsh::BorshSerialize::serialize(&7u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::I128(field0) => {
                            borsh::BorshSerialize::serialize(&8u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::U128(field0) => {
                            borsh::BorshSerialize::serialize(&9u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::Bytes(field0) => {
                            borsh::BorshSerialize::serialize(&10u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::Vec(field0) => {
                            borsh::BorshSerialize::serialize(&11u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::Option(field0) => {
                            borsh::BorshSerialize::serialize(&12u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::Struct(field0) => {
                            borsh::BorshSerialize::serialize(&13u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::String(field0) => {
                            borsh::BorshSerialize::serialize(&14u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::Pubkey(field0) => {
                            borsh::BorshSerialize::serialize(&15u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                        Value::Bool(field0) => {
                            borsh::BorshSerialize::serialize(&16u8, writer)?;
                            borsh::BorshSerialize::serialize(&field0, writer)?;
                            Ok(())
                        }
                    }
                }
            }
            impl Value {
                pub fn expr(self) -> Expression {
                    Expression::Literal(self)
                }
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
                        Value::Struct(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert struct to u128"),
                            );
                        }
                        Value::String(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert string to u128"),
                            );
                        }
                        Value::Bool(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert bool to u128"),
                            );
                        }
                        Value::Vec(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert vec to u128"),
                            );
                        }
                        Value::Pubkey(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert pubkey to u128"),
                            );
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
                        Value::Struct(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert struct to i128"),
                            );
                        }
                        Value::String(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert string to i128"),
                            );
                        }
                        Value::Bool(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert bool to i128"),
                            );
                        }
                        Value::Vec(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert vec to i128"),
                            );
                        }
                        Value::Pubkey(_) => {
                            ::core::panicking::panic_fmt(
                                format_args!("Cannot convert pubkey to i128"),
                            );
                        }
                    }
                }
                pub fn as_bytes(
                    &self,
                    serialization_type: SerializationType,
                    output: &mut Vec<u8>,
                ) {
                    match self {
                        Value::U8(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::I8(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::U16(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::I16(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::U32(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::I32(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::U64(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::I64(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::I128(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::U128(v) => output.extend_from_slice(&v.to_le_bytes()),
                        Value::Bytes(v) => output.extend_from_slice(v),
                        Value::Vec(v) => {
                            output.extend_from_slice(&(v.len() as u32).to_le_bytes());
                            for value in v.iter() {
                                value.as_bytes(serialization_type, output);
                            }
                        }
                        Value::Option(v) => {
                            output.extend_from_slice(&[v.is_some() as u8]);
                            match v {
                                Some(v) => {
                                    v.as_bytes(serialization_type, output);
                                }
                                None => output.extend_from_slice(&[]),
                            }
                        }
                        Value::Struct(v) => {
                            for value in v.iter() {
                                value.as_bytes(serialization_type, output);
                            }
                        }
                        Value::String(v) => {
                            output.extend_from_slice(&(v.len() as u32).to_le_bytes());
                            output.extend_from_slice(v.as_bytes())
                        }
                        Value::Bool(v) => output.extend_from_slice(&[*v as u8]),
                        Value::Pubkey(v) => output.extend_from_slice(v),
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
                        ValueType::I128 => {
                            if (i128::MIN as i128..=i128::MAX as i128).contains(&v) {
                                Ok(Value::I128(v as i128))
                            } else {
                                Err("Out of range for I128".into())
                            }
                        }
                        ValueType::U128 => {
                            if (0..=u128::MAX as i128).contains(&v) {
                                Ok(Value::U128(v as u128))
                            } else {
                                Err("Out of range for U128".into())
                            }
                        }
                        ValueType::Bytes => Err("Cannot cast to bytes".into()),
                        ValueType::Vec => Err("Cannot cast to vec".into()),
                        ValueType::Option => Err("Cannot cast to option".into()),
                        ValueType::Struct => Err("Cannot cast to struct".into()),
                        ValueType::String => Err("Cannot cast to string".into()),
                        ValueType::Bool => Err("Cannot cast to bool".into()),
                    }
                }
                pub fn is_unsigned(&self) -> bool {
                    match self {
                        Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) => {
                            true
                        }
                        _ => false,
                    }
                }
                pub fn is_signed(&self) -> bool {
                    match self {
                        Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) => {
                            true
                        }
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
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
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
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
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
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
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
                                (_, Value::U128(b)) => {
                                    (self.as_i128(), (*b as u128) as i128)
                                }
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
                            (Value::U8(a), Value::U8(b)) => {
                                Value::U8((*a).wrapping_mul(*b))
                            }
                            (Value::I8(a), Value::I8(b)) => {
                                Value::I8((*a).wrapping_mul(*b))
                            }
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
                            (Value::U8(a), Value::U8(b)) => {
                                Value::U8((*a).wrapping_add(*b))
                            }
                            (Value::I8(a), Value::I8(b)) => {
                                Value::I8((*a).wrapping_add(*b))
                            }
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
                            (Value::U8(a), Value::U8(b)) => {
                                Value::U8((*a).wrapping_sub(*b))
                            }
                            (Value::I8(a), Value::I8(b)) => {
                                Value::I8((*a).wrapping_sub(*b))
                            }
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
                            (Value::U8(a), Value::U8(b)) => {
                                Value::U8((*a).wrapping_div(*b))
                            }
                            (Value::I8(a), Value::I8(b)) => {
                                Value::I8((*a).wrapping_div(*b))
                            }
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
        }
        pub use condition::*;
        pub use expression::*;
        pub use validation::*;
        pub use value::*;
    }
    pub mod task {
        pub mod command {
            pub mod associated_token_program_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::types::task::task_account::TaskAccount;
                pub enum AssociatedTokenProgramInstruction {
                    InitializeAccount {
                        payer: TaskAccount,
                        token_account: TaskAccount,
                        owner: TaskAccount,
                        mint: TaskAccount,
                        token_program_id: TaskAccount,
                        system_program_id: TaskAccount,
                    },
                }
                #[automatically_derived]
                impl ::core::clone::Clone for AssociatedTokenProgramInstruction {
                    #[inline]
                    fn clone(&self) -> AssociatedTokenProgramInstruction {
                        match self {
                            AssociatedTokenProgramInstruction::InitializeAccount {
                                payer: __self_0,
                                token_account: __self_1,
                                owner: __self_2,
                                mint: __self_3,
                                token_program_id: __self_4,
                                system_program_id: __self_5,
                            } => {
                                AssociatedTokenProgramInstruction::InitializeAccount {
                                    payer: ::core::clone::Clone::clone(__self_0),
                                    token_account: ::core::clone::Clone::clone(__self_1),
                                    owner: ::core::clone::Clone::clone(__self_2),
                                    mint: ::core::clone::Clone::clone(__self_3),
                                    token_program_id: ::core::clone::Clone::clone(__self_4),
                                    system_program_id: ::core::clone::Clone::clone(__self_5),
                                }
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for AssociatedTokenProgramInstruction {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            AssociatedTokenProgramInstruction::InitializeAccount {
                                payer: __self_0,
                                token_account: __self_1,
                                owner: __self_2,
                                mint: __self_3,
                                token_program_id: __self_4,
                                system_program_id: __self_5,
                            } => {
                                let names: &'static _ = &[
                                    "payer",
                                    "token_account",
                                    "owner",
                                    "mint",
                                    "token_program_id",
                                    "system_program_id",
                                ];
                                let values: &[&dyn ::core::fmt::Debug] = &[
                                    __self_0,
                                    __self_1,
                                    __self_2,
                                    __self_3,
                                    __self_4,
                                    &__self_5,
                                ];
                                ::core::fmt::Formatter::debug_struct_fields_finish(
                                    f,
                                    "InitializeAccount",
                                    names,
                                    values,
                                )
                            }
                        }
                    }
                }
                impl borsh::de::BorshDeserialize for AssociatedTokenProgramInstruction
                where
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
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
                impl borsh::de::EnumExt for AssociatedTokenProgramInstruction
                where
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => {
                                AssociatedTokenProgramInstruction::InitializeAccount {
                                    payer: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    token_account: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    owner: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    mint: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    token_program_id: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    system_program_id: borsh::BorshDeserialize::deserialize_reader(
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
                impl borsh::ser::BorshSerialize for AssociatedTokenProgramInstruction
                where
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            AssociatedTokenProgramInstruction::InitializeAccount {
                                ..
                            } => 0u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            AssociatedTokenProgramInstruction::InitializeAccount {
                                payer,
                                token_account,
                                owner,
                                mint,
                                token_program_id,
                                system_program_id,
                            } => {
                                borsh::BorshSerialize::serialize(payer, writer)?;
                                borsh::BorshSerialize::serialize(token_account, writer)?;
                                borsh::BorshSerialize::serialize(owner, writer)?;
                                borsh::BorshSerialize::serialize(mint, writer)?;
                                borsh::BorshSerialize::serialize(token_program_id, writer)?;
                                borsh::BorshSerialize::serialize(
                                    system_program_id,
                                    writer,
                                )?;
                            }
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for AssociatedTokenProgramInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq
                for AssociatedTokenProgramInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for AssociatedTokenProgramInstruction {
                    #[inline]
                    fn eq(&self, other: &AssociatedTokenProgramInstruction) -> bool {
                        match (self, other) {
                            (
                                AssociatedTokenProgramInstruction::InitializeAccount {
                                    payer: __self_0,
                                    token_account: __self_1,
                                    owner: __self_2,
                                    mint: __self_3,
                                    token_program_id: __self_4,
                                    system_program_id: __self_5,
                                },
                                AssociatedTokenProgramInstruction::InitializeAccount {
                                    payer: __arg1_0,
                                    token_account: __arg1_1,
                                    owner: __arg1_2,
                                    mint: __arg1_3,
                                    token_program_id: __arg1_4,
                                    system_program_id: __arg1_5,
                                },
                            ) => {
                                *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                    && *__self_2 == *__arg1_2 && *__self_3 == *__arg1_3
                                    && *__self_4 == *__arg1_4 && *__self_5 == *__arg1_5
                            }
                        }
                    }
                }
            }
            pub mod command_struct {
                use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};
                use super::associated_token_program_instruction::AssociatedTokenProgramInstruction;
                use super::defined_instruction::DefinedInstruction;
                use super::raw_instruction::RawInstruction;
                use super::set_cache::SetCacheType;
                use super::system_instruction::SystemInstruction;
                use super::token_program_instruction::TokenProgramInstruction;
                use crate::types::logical_components::{Condition, Expression};
                pub enum Command {
                    InvokeRawInstruction(RawInstruction),
                    InvokeDefinedInstruction(DefinedInstruction),
                    InvokeSystemProgram(SystemInstruction),
                    InvokeAssociatedTokenProgram(AssociatedTokenProgramInstruction),
                    InvokeTokenProgram(TokenProgramInstruction),
                    SetCache(SetCacheType),
                    Conditional { condition: Condition, true_action: Box<Command> },
                    Loop { condition: Condition, actions: Vec<Box<Command>> },
                    Log(Expression),
                }
                #[automatically_derived]
                impl ::core::clone::Clone for Command {
                    #[inline]
                    fn clone(&self) -> Command {
                        match self {
                            Command::InvokeRawInstruction(__self_0) => {
                                Command::InvokeRawInstruction(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            Command::InvokeDefinedInstruction(__self_0) => {
                                Command::InvokeDefinedInstruction(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            Command::InvokeSystemProgram(__self_0) => {
                                Command::InvokeSystemProgram(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            Command::InvokeAssociatedTokenProgram(__self_0) => {
                                Command::InvokeAssociatedTokenProgram(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            Command::InvokeTokenProgram(__self_0) => {
                                Command::InvokeTokenProgram(
                                    ::core::clone::Clone::clone(__self_0),
                                )
                            }
                            Command::SetCache(__self_0) => {
                                Command::SetCache(::core::clone::Clone::clone(__self_0))
                            }
                            Command::Conditional {
                                condition: __self_0,
                                true_action: __self_1,
                            } => {
                                Command::Conditional {
                                    condition: ::core::clone::Clone::clone(__self_0),
                                    true_action: ::core::clone::Clone::clone(__self_1),
                                }
                            }
                            Command::Loop { condition: __self_0, actions: __self_1 } => {
                                Command::Loop {
                                    condition: ::core::clone::Clone::clone(__self_0),
                                    actions: ::core::clone::Clone::clone(__self_1),
                                }
                            }
                            Command::Log(__self_0) => {
                                Command::Log(::core::clone::Clone::clone(__self_0))
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for Command {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            Command::InvokeRawInstruction(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "InvokeRawInstruction",
                                    &__self_0,
                                )
                            }
                            Command::InvokeDefinedInstruction(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "InvokeDefinedInstruction",
                                    &__self_0,
                                )
                            }
                            Command::InvokeSystemProgram(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "InvokeSystemProgram",
                                    &__self_0,
                                )
                            }
                            Command::InvokeAssociatedTokenProgram(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "InvokeAssociatedTokenProgram",
                                    &__self_0,
                                )
                            }
                            Command::InvokeTokenProgram(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "InvokeTokenProgram",
                                    &__self_0,
                                )
                            }
                            Command::SetCache(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "SetCache",
                                    &__self_0,
                                )
                            }
                            Command::Conditional {
                                condition: __self_0,
                                true_action: __self_1,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field2_finish(
                                    f,
                                    "Conditional",
                                    "condition",
                                    __self_0,
                                    "true_action",
                                    &__self_1,
                                )
                            }
                            Command::Loop { condition: __self_0, actions: __self_1 } => {
                                ::core::fmt::Formatter::debug_struct_field2_finish(
                                    f,
                                    "Loop",
                                    "condition",
                                    __self_0,
                                    "actions",
                                    &__self_1,
                                )
                            }
                            Command::Log(__self_0) => {
                                ::core::fmt::Formatter::debug_tuple_field1_finish(
                                    f,
                                    "Log",
                                    &__self_0,
                                )
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for Command {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<RawInstruction>;
                        let _: ::core::cmp::AssertParamIsEq<DefinedInstruction>;
                        let _: ::core::cmp::AssertParamIsEq<SystemInstruction>;
                        let _: ::core::cmp::AssertParamIsEq<
                            AssociatedTokenProgramInstruction,
                        >;
                        let _: ::core::cmp::AssertParamIsEq<TokenProgramInstruction>;
                        let _: ::core::cmp::AssertParamIsEq<SetCacheType>;
                        let _: ::core::cmp::AssertParamIsEq<Condition>;
                        let _: ::core::cmp::AssertParamIsEq<Box<Command>>;
                        let _: ::core::cmp::AssertParamIsEq<Vec<Box<Command>>>;
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for Command {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for Command {
                    #[inline]
                    fn eq(&self, other: &Command) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    Command::InvokeRawInstruction(__self_0),
                                    Command::InvokeRawInstruction(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    Command::InvokeDefinedInstruction(__self_0),
                                    Command::InvokeDefinedInstruction(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    Command::InvokeSystemProgram(__self_0),
                                    Command::InvokeSystemProgram(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    Command::InvokeAssociatedTokenProgram(__self_0),
                                    Command::InvokeAssociatedTokenProgram(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    Command::InvokeTokenProgram(__self_0),
                                    Command::InvokeTokenProgram(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    Command::SetCache(__self_0),
                                    Command::SetCache(__arg1_0),
                                ) => *__self_0 == *__arg1_0,
                                (
                                    Command::Conditional {
                                        condition: __self_0,
                                        true_action: __self_1,
                                    },
                                    Command::Conditional {
                                        condition: __arg1_0,
                                        true_action: __arg1_1,
                                    },
                                ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                                (
                                    Command::Loop { condition: __self_0, actions: __self_1 },
                                    Command::Loop { condition: __arg1_0, actions: __arg1_1 },
                                ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                                (Command::Log(__self_0), Command::Log(__arg1_0)) => {
                                    *__self_0 == *__arg1_0
                                }
                                _ => unsafe { ::core::intrinsics::unreachable() }
                            }
                    }
                }
                impl borsh::BorshDeserialize for Command
                where
                    Vec<Box<Command>>: borsh::BorshDeserialize,
                    AssociatedTokenProgramInstruction: borsh::BorshDeserialize,
                    Box<Command>: borsh::BorshDeserialize,
                    SetCacheType: borsh::BorshDeserialize,
                    TokenProgramInstruction: borsh::BorshDeserialize,
                    Condition: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    SystemInstruction: borsh::BorshDeserialize,
                    DefinedInstruction: borsh::BorshDeserialize,
                    RawInstruction: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: std::io::Read>(
                        reader: &mut R,
                    ) -> std::io::Result<Self> {
                        let tag = u8::deserialize_reader(reader)?;
                        match tag {
                            0u8 => {
                                Ok(
                                    Command::InvokeRawInstruction(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            1u8 => {
                                Ok(
                                    Command::InvokeDefinedInstruction(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            2u8 => {
                                Ok(
                                    Command::InvokeSystemProgram(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            3u8 => {
                                Ok(
                                    Command::InvokeAssociatedTokenProgram(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            4u8 => {
                                Ok(
                                    Command::InvokeTokenProgram(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            5u8 => {
                                Ok(
                                    Command::SetCache(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            6u8 => {
                                Ok(Command::Conditional {
                                    condition: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    true_action: Box::new(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                })
                            }
                            7u8 => {
                                Ok(Command::Loop {
                                    condition: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    actions: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                })
                            }
                            8u8 => {
                                Ok(
                                    Command::Log(
                                        borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    ),
                                )
                            }
                            _ => {
                                Err(
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        {
                                            let res = ::alloc::fmt::format(
                                                format_args!("Invalid tag for enum {0}", "Command"),
                                            );
                                            res
                                        },
                                    ),
                                )
                            }
                        }
                    }
                }
                impl borsh::BorshSerialize for Command
                where
                    DefinedInstruction: borsh::BorshSerialize,
                    Box<Command>: borsh::BorshSerialize,
                    Vec<Box<Command>>: borsh::BorshSerialize,
                    SystemInstruction: borsh::BorshSerialize,
                    Expression: borsh::BorshSerialize,
                    SetCacheType: borsh::BorshSerialize,
                    Condition: borsh::BorshSerialize,
                    AssociatedTokenProgramInstruction: borsh::BorshSerialize,
                    RawInstruction: borsh::BorshSerialize,
                    TokenProgramInstruction: borsh::BorshSerialize,
                {
                    fn serialize<W: std::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> std::io::Result<()> {
                        match self {
                            Command::InvokeRawInstruction(field0) => {
                                borsh::BorshSerialize::serialize(&0u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                            Command::InvokeDefinedInstruction(field0) => {
                                borsh::BorshSerialize::serialize(&1u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                            Command::InvokeSystemProgram(field0) => {
                                borsh::BorshSerialize::serialize(&2u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                            Command::InvokeAssociatedTokenProgram(field0) => {
                                borsh::BorshSerialize::serialize(&3u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                            Command::InvokeTokenProgram(field0) => {
                                borsh::BorshSerialize::serialize(&4u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                            Command::SetCache(field0) => {
                                borsh::BorshSerialize::serialize(&5u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                            Command::Conditional { condition, true_action } => {
                                borsh::BorshSerialize::serialize(&6u8, writer)?;
                                borsh::BorshSerialize::serialize(&condition, writer)?;
                                borsh::BorshSerialize::serialize(&**true_action, writer)?;
                                Ok(())
                            }
                            Command::Loop { condition, actions } => {
                                borsh::BorshSerialize::serialize(&7u8, writer)?;
                                borsh::BorshSerialize::serialize(&condition, writer)?;
                                borsh::BorshSerialize::serialize(&actions, writer)?;
                                Ok(())
                            }
                            Command::Log(field0) => {
                                borsh::BorshSerialize::serialize(&8u8, writer)?;
                                borsh::BorshSerialize::serialize(&field0, writer)?;
                                Ok(())
                            }
                        }
                    }
                }
                impl Command {
                    pub fn loop_action(
                        condition: Condition,
                        actions: Vec<Command>,
                    ) -> Command {
                        Command::Loop {
                            condition,
                            actions: actions.into_iter().map(Box::new).collect(),
                        }
                    }
                }
            }
            pub mod defined_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::{
                    types::logical_components::Expression,
                    types::task::task_account::TaskAccount,
                };
                #[repr(u8)]
                pub enum SerializationType {
                    Borsh,
                    Bytemuck,
                }
                #[automatically_derived]
                impl ::core::marker::Copy for SerializationType {}
                #[automatically_derived]
                impl ::core::clone::Clone for SerializationType {
                    #[inline]
                    fn clone(&self) -> SerializationType {
                        *self
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for SerializationType {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::write_str(
                            f,
                            match self {
                                SerializationType::Borsh => "Borsh",
                                SerializationType::Bytemuck => "Bytemuck",
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
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            SerializationType::Borsh => {}
                            SerializationType::Bytemuck => {}
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
                pub struct DefinedInstruction {
                    pub serialization_type: SerializationType,
                    pub program: TaskAccount,
                    pub accounts: Vec<DefinedAccount>,
                    pub arguments: Vec<DefinedArgument>,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for DefinedInstruction {
                    #[inline]
                    fn clone(&self) -> DefinedInstruction {
                        DefinedInstruction {
                            serialization_type: ::core::clone::Clone::clone(
                                &self.serialization_type,
                            ),
                            program: ::core::clone::Clone::clone(&self.program),
                            accounts: ::core::clone::Clone::clone(&self.accounts),
                            arguments: ::core::clone::Clone::clone(&self.arguments),
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for DefinedInstruction {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::debug_struct_field4_finish(
                            f,
                            "DefinedInstruction",
                            "serialization_type",
                            &self.serialization_type,
                            "program",
                            &self.program,
                            "accounts",
                            &self.accounts,
                            "arguments",
                            &&self.arguments,
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for DefinedInstruction
                where
                    SerializationType: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Vec<DefinedAccount>: borsh::BorshDeserialize,
                    Vec<DefinedArgument>: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        Ok(Self {
                            serialization_type: borsh::BorshDeserialize::deserialize_reader(
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
                impl borsh::ser::BorshSerialize for DefinedInstruction
                where
                    SerializationType: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    Vec<DefinedAccount>: borsh::ser::BorshSerialize,
                    Vec<DefinedArgument>: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        borsh::BorshSerialize::serialize(
                            &self.serialization_type,
                            writer,
                        )?;
                        borsh::BorshSerialize::serialize(&self.program, writer)?;
                        borsh::BorshSerialize::serialize(&self.accounts, writer)?;
                        borsh::BorshSerialize::serialize(&self.arguments, writer)?;
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for DefinedInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<SerializationType>;
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                        let _: ::core::cmp::AssertParamIsEq<Vec<DefinedAccount>>;
                        let _: ::core::cmp::AssertParamIsEq<Vec<DefinedArgument>>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for DefinedInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for DefinedInstruction {
                    #[inline]
                    fn eq(&self, other: &DefinedInstruction) -> bool {
                        self.serialization_type == other.serialization_type
                            && self.program == other.program
                            && self.accounts == other.accounts
                            && self.arguments == other.arguments
                    }
                }
                pub struct DefinedAccount {
                    pub writable: bool,
                    pub signer: bool,
                    pub task_account: TaskAccount,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for DefinedAccount {
                    #[inline]
                    fn clone(&self) -> DefinedAccount {
                        DefinedAccount {
                            writable: ::core::clone::Clone::clone(&self.writable),
                            signer: ::core::clone::Clone::clone(&self.signer),
                            task_account: ::core::clone::Clone::clone(&self.task_account),
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for DefinedAccount {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::debug_struct_field3_finish(
                            f,
                            "DefinedAccount",
                            "writable",
                            &self.writable,
                            "signer",
                            &self.signer,
                            "task_account",
                            &&self.task_account,
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for DefinedAccount
                where
                    bool: borsh::BorshDeserialize,
                    bool: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        Ok(Self {
                            writable: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                            signer: borsh::BorshDeserialize::deserialize_reader(reader)?,
                            task_account: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                        })
                    }
                }
                impl borsh::ser::BorshSerialize for DefinedAccount
                where
                    bool: borsh::ser::BorshSerialize,
                    bool: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        borsh::BorshSerialize::serialize(&self.writable, writer)?;
                        borsh::BorshSerialize::serialize(&self.signer, writer)?;
                        borsh::BorshSerialize::serialize(&self.task_account, writer)?;
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for DefinedAccount {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<bool>;
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for DefinedAccount {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for DefinedAccount {
                    #[inline]
                    fn eq(&self, other: &DefinedAccount) -> bool {
                        self.writable == other.writable && self.signer == other.signer
                            && self.task_account == other.task_account
                    }
                }
                pub struct DefinedArgument {
                    pub value: Expression,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for DefinedArgument {
                    #[inline]
                    fn clone(&self) -> DefinedArgument {
                        DefinedArgument {
                            value: ::core::clone::Clone::clone(&self.value),
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for DefinedArgument {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::debug_struct_field1_finish(
                            f,
                            "DefinedArgument",
                            "value",
                            &&self.value,
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for DefinedArgument
                where
                    Expression: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        Ok(Self {
                            value: borsh::BorshDeserialize::deserialize_reader(reader)?,
                        })
                    }
                }
                impl borsh::ser::BorshSerialize for DefinedArgument
                where
                    Expression: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        borsh::BorshSerialize::serialize(&self.value, writer)?;
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for DefinedArgument {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for DefinedArgument {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for DefinedArgument {
                    #[inline]
                    fn eq(&self, other: &DefinedArgument) -> bool {
                        self.value == other.value
                    }
                }
            }
            pub mod raw_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::types::logical_components::Expression;
                use crate::types::task::task_account::{TaskAccount, TaskAccounts};
                pub struct RawInstruction {
                    pub program: TaskAccount,
                    pub data: Expression,
                    pub accounts: TaskAccounts,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for RawInstruction {
                    #[inline]
                    fn clone(&self) -> RawInstruction {
                        RawInstruction {
                            program: ::core::clone::Clone::clone(&self.program),
                            data: ::core::clone::Clone::clone(&self.data),
                            accounts: ::core::clone::Clone::clone(&self.accounts),
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
                        ::core::fmt::Formatter::debug_struct_field3_finish(
                            f,
                            "RawInstruction",
                            "program",
                            &self.program,
                            "data",
                            &self.data,
                            "accounts",
                            &&self.accounts,
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for RawInstruction
                where
                    TaskAccount: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    TaskAccounts: borsh::BorshDeserialize,
                {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        Ok(Self {
                            program: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                            data: borsh::BorshDeserialize::deserialize_reader(reader)?,
                            accounts: borsh::BorshDeserialize::deserialize_reader(
                                reader,
                            )?,
                        })
                    }
                }
                impl borsh::ser::BorshSerialize for RawInstruction
                where
                    TaskAccount: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                    TaskAccounts: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        borsh::BorshSerialize::serialize(&self.program, writer)?;
                        borsh::BorshSerialize::serialize(&self.data, writer)?;
                        borsh::BorshSerialize::serialize(&self.accounts, writer)?;
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for RawInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                        let _: ::core::cmp::AssertParamIsEq<TaskAccounts>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for RawInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for RawInstruction {
                    #[inline]
                    fn eq(&self, other: &RawInstruction) -> bool {
                        self.program == other.program && self.data == other.data
                            && self.accounts == other.accounts
                    }
                }
            }
            pub mod set_cache {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::types::logical_components::{Expression, ValueType};
                pub enum SetCacheType {
                    AccountData { account: u8, offset: u8, value_type: ValueType },
                    Expression { index: u8, expression: Expression },
                }
                #[automatically_derived]
                impl ::core::clone::Clone for SetCacheType {
                    #[inline]
                    fn clone(&self) -> SetCacheType {
                        match self {
                            SetCacheType::AccountData {
                                account: __self_0,
                                offset: __self_1,
                                value_type: __self_2,
                            } => {
                                SetCacheType::AccountData {
                                    account: ::core::clone::Clone::clone(__self_0),
                                    offset: ::core::clone::Clone::clone(__self_1),
                                    value_type: ::core::clone::Clone::clone(__self_2),
                                }
                            }
                            SetCacheType::Expression {
                                index: __self_0,
                                expression: __self_1,
                            } => {
                                SetCacheType::Expression {
                                    index: ::core::clone::Clone::clone(__self_0),
                                    expression: ::core::clone::Clone::clone(__self_1),
                                }
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for SetCacheType {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            SetCacheType::AccountData {
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
                            SetCacheType::Expression {
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
                impl borsh::de::BorshDeserialize for SetCacheType
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
                impl borsh::de::EnumExt for SetCacheType
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
                                SetCacheType::AccountData {
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
                                SetCacheType::Expression {
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
                impl borsh::ser::BorshSerialize for SetCacheType
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
                            SetCacheType::AccountData { .. } => 0u8,
                            SetCacheType::Expression { .. } => 1u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            SetCacheType::AccountData {
                                account,
                                offset,
                                value_type,
                            } => {
                                borsh::BorshSerialize::serialize(account, writer)?;
                                borsh::BorshSerialize::serialize(offset, writer)?;
                                borsh::BorshSerialize::serialize(value_type, writer)?;
                            }
                            SetCacheType::Expression { index, expression } => {
                                borsh::BorshSerialize::serialize(index, writer)?;
                                borsh::BorshSerialize::serialize(expression, writer)?;
                            }
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for SetCacheType {
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
                impl ::core::marker::StructuralPartialEq for SetCacheType {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for SetCacheType {
                    #[inline]
                    fn eq(&self, other: &SetCacheType) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    SetCacheType::AccountData {
                                        account: __self_0,
                                        offset: __self_1,
                                        value_type: __self_2,
                                    },
                                    SetCacheType::AccountData {
                                        account: __arg1_0,
                                        offset: __arg1_1,
                                        value_type: __arg1_2,
                                    },
                                ) => {
                                    *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                        && *__self_2 == *__arg1_2
                                }
                                (
                                    SetCacheType::Expression {
                                        index: __self_0,
                                        expression: __self_1,
                                    },
                                    SetCacheType::Expression {
                                        index: __arg1_0,
                                        expression: __arg1_1,
                                    },
                                ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                                _ => unsafe { ::core::intrinsics::unreachable() }
                            }
                    }
                }
            }
            pub mod system_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::types::logical_components::Expression;
                use crate::types::task::task_account::TaskAccount;
                pub enum SystemInstruction {
                    CreateAccount {
                        payer: TaskAccount,
                        account: TaskAccount,
                        program_owner: TaskAccount,
                        space: Expression,
                        lamports: Expression,
                    },
                    Transfer { from: TaskAccount, to: TaskAccount, amount: Expression },
                    Allocate,
                    AllocateWithSeed,
                    AssignWithSeed,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for SystemInstruction {
                    #[inline]
                    fn clone(&self) -> SystemInstruction {
                        match self {
                            SystemInstruction::CreateAccount {
                                payer: __self_0,
                                account: __self_1,
                                program_owner: __self_2,
                                space: __self_3,
                                lamports: __self_4,
                            } => {
                                SystemInstruction::CreateAccount {
                                    payer: ::core::clone::Clone::clone(__self_0),
                                    account: ::core::clone::Clone::clone(__self_1),
                                    program_owner: ::core::clone::Clone::clone(__self_2),
                                    space: ::core::clone::Clone::clone(__self_3),
                                    lamports: ::core::clone::Clone::clone(__self_4),
                                }
                            }
                            SystemInstruction::Transfer {
                                from: __self_0,
                                to: __self_1,
                                amount: __self_2,
                            } => {
                                SystemInstruction::Transfer {
                                    from: ::core::clone::Clone::clone(__self_0),
                                    to: ::core::clone::Clone::clone(__self_1),
                                    amount: ::core::clone::Clone::clone(__self_2),
                                }
                            }
                            SystemInstruction::Allocate => SystemInstruction::Allocate,
                            SystemInstruction::AllocateWithSeed => {
                                SystemInstruction::AllocateWithSeed
                            }
                            SystemInstruction::AssignWithSeed => {
                                SystemInstruction::AssignWithSeed
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for SystemInstruction {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            SystemInstruction::CreateAccount {
                                payer: __self_0,
                                account: __self_1,
                                program_owner: __self_2,
                                space: __self_3,
                                lamports: __self_4,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field5_finish(
                                    f,
                                    "CreateAccount",
                                    "payer",
                                    __self_0,
                                    "account",
                                    __self_1,
                                    "program_owner",
                                    __self_2,
                                    "space",
                                    __self_3,
                                    "lamports",
                                    &__self_4,
                                )
                            }
                            SystemInstruction::Transfer {
                                from: __self_0,
                                to: __self_1,
                                amount: __self_2,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field3_finish(
                                    f,
                                    "Transfer",
                                    "from",
                                    __self_0,
                                    "to",
                                    __self_1,
                                    "amount",
                                    &__self_2,
                                )
                            }
                            SystemInstruction::Allocate => {
                                ::core::fmt::Formatter::write_str(f, "Allocate")
                            }
                            SystemInstruction::AllocateWithSeed => {
                                ::core::fmt::Formatter::write_str(f, "AllocateWithSeed")
                            }
                            SystemInstruction::AssignWithSeed => {
                                ::core::fmt::Formatter::write_str(f, "AssignWithSeed")
                            }
                        }
                    }
                }
                impl borsh::de::BorshDeserialize for SystemInstruction
                where
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
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
                impl borsh::de::EnumExt for SystemInstruction
                where
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => {
                                SystemInstruction::CreateAccount {
                                    payer: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    account: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    program_owner: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    space: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    lamports: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                }
                            }
                            1u8 => {
                                SystemInstruction::Transfer {
                                    from: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    to: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    amount: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                }
                            }
                            2u8 => SystemInstruction::Allocate,
                            3u8 => SystemInstruction::AllocateWithSeed,
                            4u8 => SystemInstruction::AssignWithSeed,
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
                impl borsh::ser::BorshSerialize for SystemInstruction
                where
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            SystemInstruction::CreateAccount { .. } => 0u8,
                            SystemInstruction::Transfer { .. } => 1u8,
                            SystemInstruction::Allocate => 2u8,
                            SystemInstruction::AllocateWithSeed => 3u8,
                            SystemInstruction::AssignWithSeed => 4u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            SystemInstruction::CreateAccount {
                                payer,
                                account,
                                program_owner,
                                space,
                                lamports,
                            } => {
                                borsh::BorshSerialize::serialize(payer, writer)?;
                                borsh::BorshSerialize::serialize(account, writer)?;
                                borsh::BorshSerialize::serialize(program_owner, writer)?;
                                borsh::BorshSerialize::serialize(space, writer)?;
                                borsh::BorshSerialize::serialize(lamports, writer)?;
                            }
                            SystemInstruction::Transfer { from, to, amount } => {
                                borsh::BorshSerialize::serialize(from, writer)?;
                                borsh::BorshSerialize::serialize(to, writer)?;
                                borsh::BorshSerialize::serialize(amount, writer)?;
                            }
                            SystemInstruction::Allocate => {}
                            SystemInstruction::AllocateWithSeed => {}
                            SystemInstruction::AssignWithSeed => {}
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for SystemInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for SystemInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for SystemInstruction {
                    #[inline]
                    fn eq(&self, other: &SystemInstruction) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    SystemInstruction::CreateAccount {
                                        payer: __self_0,
                                        account: __self_1,
                                        program_owner: __self_2,
                                        space: __self_3,
                                        lamports: __self_4,
                                    },
                                    SystemInstruction::CreateAccount {
                                        payer: __arg1_0,
                                        account: __arg1_1,
                                        program_owner: __arg1_2,
                                        space: __arg1_3,
                                        lamports: __arg1_4,
                                    },
                                ) => {
                                    *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                        && *__self_2 == *__arg1_2 && *__self_3 == *__arg1_3
                                        && *__self_4 == *__arg1_4
                                }
                                (
                                    SystemInstruction::Transfer {
                                        from: __self_0,
                                        to: __self_1,
                                        amount: __self_2,
                                    },
                                    SystemInstruction::Transfer {
                                        from: __arg1_0,
                                        to: __arg1_1,
                                        amount: __arg1_2,
                                    },
                                ) => {
                                    *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                        && *__self_2 == *__arg1_2
                                }
                                _ => true,
                            }
                    }
                }
            }
            pub mod token_program_instruction {
                use borsh::{BorshDeserialize, BorshSerialize};
                use crate::types::logical_components::Expression;
                use crate::types::task::task_account::TaskAccount;
                pub enum TokenProgramVersion {
                    Legacy,
                    Token2022,
                }
                #[automatically_derived]
                impl ::core::clone::Clone for TokenProgramVersion {
                    #[inline]
                    fn clone(&self) -> TokenProgramVersion {
                        match self {
                            TokenProgramVersion::Legacy => TokenProgramVersion::Legacy,
                            TokenProgramVersion::Token2022 => {
                                TokenProgramVersion::Token2022
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for TokenProgramVersion {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        ::core::fmt::Formatter::write_str(
                            f,
                            match self {
                                TokenProgramVersion::Legacy => "Legacy",
                                TokenProgramVersion::Token2022 => "Token2022",
                            },
                        )
                    }
                }
                impl borsh::de::BorshDeserialize for TokenProgramVersion {
                    fn deserialize_reader<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let tag = <u8 as borsh::de::BorshDeserialize>::deserialize_reader(
                            reader,
                        )?;
                        <Self as borsh::de::EnumExt>::deserialize_variant(reader, tag)
                    }
                }
                impl borsh::de::EnumExt for TokenProgramVersion {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => TokenProgramVersion::Legacy,
                            1u8 => TokenProgramVersion::Token2022,
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
                impl borsh::ser::BorshSerialize for TokenProgramVersion {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            TokenProgramVersion::Legacy => 0u8,
                            TokenProgramVersion::Token2022 => 1u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            TokenProgramVersion::Legacy => {}
                            TokenProgramVersion::Token2022 => {}
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for TokenProgramVersion {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {}
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for TokenProgramVersion {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for TokenProgramVersion {
                    #[inline]
                    fn eq(&self, other: &TokenProgramVersion) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                    }
                }
                pub enum TokenProgramInstruction {
                    Transfer {
                        program_version: TokenProgramVersion,
                        from: TaskAccount,
                        from_token_account: TaskAccount,
                        to_token_account: TaskAccount,
                        multisig: Option<Vec<TaskAccount>>,
                        amount: Expression,
                    },
                    InitializeAccount {
                        program_version: TokenProgramVersion,
                        account: TaskAccount,
                        owner: TaskAccount,
                        mint: TaskAccount,
                    },
                }
                #[automatically_derived]
                impl ::core::clone::Clone for TokenProgramInstruction {
                    #[inline]
                    fn clone(&self) -> TokenProgramInstruction {
                        match self {
                            TokenProgramInstruction::Transfer {
                                program_version: __self_0,
                                from: __self_1,
                                from_token_account: __self_2,
                                to_token_account: __self_3,
                                multisig: __self_4,
                                amount: __self_5,
                            } => {
                                TokenProgramInstruction::Transfer {
                                    program_version: ::core::clone::Clone::clone(__self_0),
                                    from: ::core::clone::Clone::clone(__self_1),
                                    from_token_account: ::core::clone::Clone::clone(__self_2),
                                    to_token_account: ::core::clone::Clone::clone(__self_3),
                                    multisig: ::core::clone::Clone::clone(__self_4),
                                    amount: ::core::clone::Clone::clone(__self_5),
                                }
                            }
                            TokenProgramInstruction::InitializeAccount {
                                program_version: __self_0,
                                account: __self_1,
                                owner: __self_2,
                                mint: __self_3,
                            } => {
                                TokenProgramInstruction::InitializeAccount {
                                    program_version: ::core::clone::Clone::clone(__self_0),
                                    account: ::core::clone::Clone::clone(__self_1),
                                    owner: ::core::clone::Clone::clone(__self_2),
                                    mint: ::core::clone::Clone::clone(__self_3),
                                }
                            }
                        }
                    }
                }
                #[automatically_derived]
                impl ::core::fmt::Debug for TokenProgramInstruction {
                    #[inline]
                    fn fmt(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        match self {
                            TokenProgramInstruction::Transfer {
                                program_version: __self_0,
                                from: __self_1,
                                from_token_account: __self_2,
                                to_token_account: __self_3,
                                multisig: __self_4,
                                amount: __self_5,
                            } => {
                                let names: &'static _ = &[
                                    "program_version",
                                    "from",
                                    "from_token_account",
                                    "to_token_account",
                                    "multisig",
                                    "amount",
                                ];
                                let values: &[&dyn ::core::fmt::Debug] = &[
                                    __self_0,
                                    __self_1,
                                    __self_2,
                                    __self_3,
                                    __self_4,
                                    &__self_5,
                                ];
                                ::core::fmt::Formatter::debug_struct_fields_finish(
                                    f,
                                    "Transfer",
                                    names,
                                    values,
                                )
                            }
                            TokenProgramInstruction::InitializeAccount {
                                program_version: __self_0,
                                account: __self_1,
                                owner: __self_2,
                                mint: __self_3,
                            } => {
                                ::core::fmt::Formatter::debug_struct_field4_finish(
                                    f,
                                    "InitializeAccount",
                                    "program_version",
                                    __self_0,
                                    "account",
                                    __self_1,
                                    "owner",
                                    __self_2,
                                    "mint",
                                    &__self_3,
                                )
                            }
                        }
                    }
                }
                impl borsh::de::BorshDeserialize for TokenProgramInstruction
                where
                    TokenProgramVersion: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Option<Vec<TaskAccount>>: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    TokenProgramVersion: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
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
                impl borsh::de::EnumExt for TokenProgramInstruction
                where
                    TokenProgramVersion: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    Option<Vec<TaskAccount>>: borsh::BorshDeserialize,
                    Expression: borsh::BorshDeserialize,
                    TokenProgramVersion: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                    TaskAccount: borsh::BorshDeserialize,
                {
                    fn deserialize_variant<R: borsh::maybestd::io::Read>(
                        reader: &mut R,
                        variant_idx: u8,
                    ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                        let mut return_value = match variant_idx {
                            0u8 => {
                                TokenProgramInstruction::Transfer {
                                    program_version: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    from: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    from_token_account: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    to_token_account: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    multisig: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    amount: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                }
                            }
                            1u8 => {
                                TokenProgramInstruction::InitializeAccount {
                                    program_version: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    account: borsh::BorshDeserialize::deserialize_reader(
                                        reader,
                                    )?,
                                    owner: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                    mint: borsh::BorshDeserialize::deserialize_reader(reader)?,
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
                impl borsh::ser::BorshSerialize for TokenProgramInstruction
                where
                    TokenProgramVersion: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    Option<Vec<TaskAccount>>: borsh::ser::BorshSerialize,
                    Expression: borsh::ser::BorshSerialize,
                    TokenProgramVersion: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                    TaskAccount: borsh::ser::BorshSerialize,
                {
                    fn serialize<W: borsh::maybestd::io::Write>(
                        &self,
                        writer: &mut W,
                    ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                        let variant_idx: u8 = match self {
                            TokenProgramInstruction::Transfer { .. } => 0u8,
                            TokenProgramInstruction::InitializeAccount { .. } => 1u8,
                        };
                        writer.write_all(&variant_idx.to_le_bytes())?;
                        match self {
                            TokenProgramInstruction::Transfer {
                                program_version,
                                from,
                                from_token_account,
                                to_token_account,
                                multisig,
                                amount,
                            } => {
                                borsh::BorshSerialize::serialize(program_version, writer)?;
                                borsh::BorshSerialize::serialize(from, writer)?;
                                borsh::BorshSerialize::serialize(
                                    from_token_account,
                                    writer,
                                )?;
                                borsh::BorshSerialize::serialize(to_token_account, writer)?;
                                borsh::BorshSerialize::serialize(multisig, writer)?;
                                borsh::BorshSerialize::serialize(amount, writer)?;
                            }
                            TokenProgramInstruction::InitializeAccount {
                                program_version,
                                account,
                                owner,
                                mint,
                            } => {
                                borsh::BorshSerialize::serialize(program_version, writer)?;
                                borsh::BorshSerialize::serialize(account, writer)?;
                                borsh::BorshSerialize::serialize(owner, writer)?;
                                borsh::BorshSerialize::serialize(mint, writer)?;
                            }
                        }
                        Ok(())
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Eq for TokenProgramInstruction {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<TokenProgramVersion>;
                        let _: ::core::cmp::AssertParamIsEq<TaskAccount>;
                        let _: ::core::cmp::AssertParamIsEq<Option<Vec<TaskAccount>>>;
                        let _: ::core::cmp::AssertParamIsEq<Expression>;
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for TokenProgramInstruction {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for TokenProgramInstruction {
                    #[inline]
                    fn eq(&self, other: &TokenProgramInstruction) -> bool {
                        let __self_tag = ::core::intrinsics::discriminant_value(self);
                        let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                        __self_tag == __arg1_tag
                            && match (self, other) {
                                (
                                    TokenProgramInstruction::Transfer {
                                        program_version: __self_0,
                                        from: __self_1,
                                        from_token_account: __self_2,
                                        to_token_account: __self_3,
                                        multisig: __self_4,
                                        amount: __self_5,
                                    },
                                    TokenProgramInstruction::Transfer {
                                        program_version: __arg1_0,
                                        from: __arg1_1,
                                        from_token_account: __arg1_2,
                                        to_token_account: __arg1_3,
                                        multisig: __arg1_4,
                                        amount: __arg1_5,
                                    },
                                ) => {
                                    *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                        && *__self_2 == *__arg1_2 && *__self_3 == *__arg1_3
                                        && *__self_4 == *__arg1_4 && *__self_5 == *__arg1_5
                                }
                                (
                                    TokenProgramInstruction::InitializeAccount {
                                        program_version: __self_0,
                                        account: __self_1,
                                        owner: __self_2,
                                        mint: __self_3,
                                    },
                                    TokenProgramInstruction::InitializeAccount {
                                        program_version: __arg1_0,
                                        account: __arg1_1,
                                        owner: __arg1_2,
                                        mint: __arg1_3,
                                    },
                                ) => {
                                    *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1
                                        && *__self_2 == *__arg1_2 && *__self_3 == *__arg1_3
                                }
                                _ => unsafe { ::core::intrinsics::unreachable() }
                            }
                    }
                }
            }
            pub use command_struct::*;
        }
        pub mod task_account {
            use borsh::{BorshDeserialize, BorshSerialize};
            use crate::types::logical_components::Expression;
            pub enum TaskAccount {
                FeePayer,
                FromInput(u8),
                FromInputWithSeed { index: u8, seed: Option<u32> },
                FromGroup { group_index: u8, account_index: u8 },
                Evaluated(Expression),
            }
            #[automatically_derived]
            impl ::core::clone::Clone for TaskAccount {
                #[inline]
                fn clone(&self) -> TaskAccount {
                    match self {
                        TaskAccount::FeePayer => TaskAccount::FeePayer,
                        TaskAccount::FromInput(__self_0) => {
                            TaskAccount::FromInput(::core::clone::Clone::clone(__self_0))
                        }
                        TaskAccount::FromInputWithSeed {
                            index: __self_0,
                            seed: __self_1,
                        } => {
                            TaskAccount::FromInputWithSeed {
                                index: ::core::clone::Clone::clone(__self_0),
                                seed: ::core::clone::Clone::clone(__self_1),
                            }
                        }
                        TaskAccount::FromGroup {
                            group_index: __self_0,
                            account_index: __self_1,
                        } => {
                            TaskAccount::FromGroup {
                                group_index: ::core::clone::Clone::clone(__self_0),
                                account_index: ::core::clone::Clone::clone(__self_1),
                            }
                        }
                        TaskAccount::Evaluated(__self_0) => {
                            TaskAccount::Evaluated(::core::clone::Clone::clone(__self_0))
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TaskAccount {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        TaskAccount::FeePayer => {
                            ::core::fmt::Formatter::write_str(f, "FeePayer")
                        }
                        TaskAccount::FromInput(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "FromInput",
                                &__self_0,
                            )
                        }
                        TaskAccount::FromInputWithSeed {
                            index: __self_0,
                            seed: __self_1,
                        } => {
                            ::core::fmt::Formatter::debug_struct_field2_finish(
                                f,
                                "FromInputWithSeed",
                                "index",
                                __self_0,
                                "seed",
                                &__self_1,
                            )
                        }
                        TaskAccount::FromGroup {
                            group_index: __self_0,
                            account_index: __self_1,
                        } => {
                            ::core::fmt::Formatter::debug_struct_field2_finish(
                                f,
                                "FromGroup",
                                "group_index",
                                __self_0,
                                "account_index",
                                &__self_1,
                            )
                        }
                        TaskAccount::Evaluated(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Evaluated",
                                &__self_0,
                            )
                        }
                    }
                }
            }
            impl borsh::de::BorshDeserialize for TaskAccount
            where
                u8: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
                Option<u32>: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
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
            impl borsh::de::EnumExt for TaskAccount
            where
                u8: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
                Option<u32>: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
                u8: borsh::BorshDeserialize,
                Expression: borsh::BorshDeserialize,
            {
                fn deserialize_variant<R: borsh::maybestd::io::Read>(
                    reader: &mut R,
                    variant_idx: u8,
                ) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
                    let mut return_value = match variant_idx {
                        0u8 => TaskAccount::FeePayer,
                        1u8 => {
                            TaskAccount::FromInput(
                                borsh::BorshDeserialize::deserialize_reader(reader)?,
                            )
                        }
                        2u8 => {
                            TaskAccount::FromInputWithSeed {
                                index: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                seed: borsh::BorshDeserialize::deserialize_reader(reader)?,
                            }
                        }
                        3u8 => {
                            TaskAccount::FromGroup {
                                group_index: borsh::BorshDeserialize::deserialize_reader(
                                    reader,
                                )?,
                                account_index: borsh::BorshDeserialize::deserialize_reader(
                                    reader,
                                )?,
                            }
                        }
                        4u8 => {
                            TaskAccount::Evaluated(
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
            impl borsh::ser::BorshSerialize for TaskAccount
            where
                u8: borsh::ser::BorshSerialize,
                u8: borsh::ser::BorshSerialize,
                Option<u32>: borsh::ser::BorshSerialize,
                u8: borsh::ser::BorshSerialize,
                u8: borsh::ser::BorshSerialize,
                Expression: borsh::ser::BorshSerialize,
            {
                fn serialize<W: borsh::maybestd::io::Write>(
                    &self,
                    writer: &mut W,
                ) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
                    let variant_idx: u8 = match self {
                        TaskAccount::FeePayer => 0u8,
                        TaskAccount::FromInput(..) => 1u8,
                        TaskAccount::FromInputWithSeed { .. } => 2u8,
                        TaskAccount::FromGroup { .. } => 3u8,
                        TaskAccount::Evaluated(..) => 4u8,
                    };
                    writer.write_all(&variant_idx.to_le_bytes())?;
                    match self {
                        TaskAccount::FeePayer => {}
                        TaskAccount::FromInput(id0) => {
                            borsh::BorshSerialize::serialize(id0, writer)?;
                        }
                        TaskAccount::FromInputWithSeed { index, seed } => {
                            borsh::BorshSerialize::serialize(index, writer)?;
                            borsh::BorshSerialize::serialize(seed, writer)?;
                        }
                        TaskAccount::FromGroup { group_index, account_index } => {
                            borsh::BorshSerialize::serialize(group_index, writer)?;
                            borsh::BorshSerialize::serialize(account_index, writer)?;
                        }
                        TaskAccount::Evaluated(id0) => {
                            borsh::BorshSerialize::serialize(id0, writer)?;
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
                    let _: ::core::cmp::AssertParamIsEq<Option<u32>>;
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
                                TaskAccount::FromInput(__self_0),
                                TaskAccount::FromInput(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                TaskAccount::FromInputWithSeed {
                                    index: __self_0,
                                    seed: __self_1,
                                },
                                TaskAccount::FromInputWithSeed {
                                    index: __arg1_0,
                                    seed: __arg1_1,
                                },
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                TaskAccount::FromGroup {
                                    group_index: __self_0,
                                    account_index: __self_1,
                                },
                                TaskAccount::FromGroup {
                                    group_index: __arg1_0,
                                    account_index: __arg1_1,
                                },
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                TaskAccount::Evaluated(__self_0),
                                TaskAccount::Evaluated(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => true,
                        }
                }
            }
            pub enum TaskAccounts {
                FromInput { start: u8, length: u8 },
                Evaluated { start: Expression, length: Expression },
            }
            #[automatically_derived]
            impl ::core::clone::Clone for TaskAccounts {
                #[inline]
                fn clone(&self) -> TaskAccounts {
                    match self {
                        TaskAccounts::FromInput { start: __self_0, length: __self_1 } => {
                            TaskAccounts::FromInput {
                                start: ::core::clone::Clone::clone(__self_0),
                                length: ::core::clone::Clone::clone(__self_1),
                            }
                        }
                        TaskAccounts::Evaluated { start: __self_0, length: __self_1 } => {
                            TaskAccounts::Evaluated {
                                start: ::core::clone::Clone::clone(__self_0),
                                length: ::core::clone::Clone::clone(__self_1),
                            }
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TaskAccounts {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        TaskAccounts::FromInput { start: __self_0, length: __self_1 } => {
                            ::core::fmt::Formatter::debug_struct_field2_finish(
                                f,
                                "FromInput",
                                "start",
                                __self_0,
                                "length",
                                &__self_1,
                            )
                        }
                        TaskAccounts::Evaluated { start: __self_0, length: __self_1 } => {
                            ::core::fmt::Formatter::debug_struct_field2_finish(
                                f,
                                "Evaluated",
                                "start",
                                __self_0,
                                "length",
                                &__self_1,
                            )
                        }
                    }
                }
            }
            impl borsh::de::BorshDeserialize for TaskAccounts
            where
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
            impl borsh::de::EnumExt for TaskAccounts
            where
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
                            TaskAccounts::FromInput {
                                start: borsh::BorshDeserialize::deserialize_reader(reader)?,
                                length: borsh::BorshDeserialize::deserialize_reader(reader)?,
                            }
                        }
                        1u8 => {
                            TaskAccounts::Evaluated {
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
            impl borsh::ser::BorshSerialize for TaskAccounts
            where
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
                        TaskAccounts::FromInput { .. } => 0u8,
                        TaskAccounts::Evaluated { .. } => 1u8,
                    };
                    writer.write_all(&variant_idx.to_le_bytes())?;
                    match self {
                        TaskAccounts::FromInput { start, length } => {
                            borsh::BorshSerialize::serialize(start, writer)?;
                            borsh::BorshSerialize::serialize(length, writer)?;
                        }
                        TaskAccounts::Evaluated { start, length } => {
                            borsh::BorshSerialize::serialize(start, writer)?;
                            borsh::BorshSerialize::serialize(length, writer)?;
                        }
                    }
                    Ok(())
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for TaskAccounts {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<u8>;
                    let _: ::core::cmp::AssertParamIsEq<Expression>;
                }
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for TaskAccounts {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for TaskAccounts {
                #[inline]
                fn eq(&self, other: &TaskAccounts) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                TaskAccounts::FromInput {
                                    start: __self_0,
                                    length: __self_1,
                                },
                                TaskAccounts::FromInput {
                                    start: __arg1_0,
                                    length: __arg1_1,
                                },
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            (
                                TaskAccounts::Evaluated {
                                    start: __self_0,
                                    length: __self_1,
                                },
                                TaskAccounts::Evaluated {
                                    start: __arg1_0,
                                    length: __arg1_1,
                                },
                            ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl TaskAccount {
                pub fn evaluated(expression: &Expression) -> Self {
                    Self::Evaluated(expression.clone())
                }
            }
        }
    }
    pub mod execution_state {
        use pinocchio::{account_info::AccountInfo, instruction::{Account, AccountMeta}};
        use crate::accounts::task_definition::TaskDefinition;
        use super::logical_components::Value;
        ///
        /// The state of the execution of a task.
        /// Used to reference task definition.
        pub struct ExecutionState<'a> {
            pub task_definition: &'a TaskDefinition,
            pub payer: &'a AccountInfo,
            pub input_values: &'a [Value],
            pub input_accounts: &'a [AccountInfo],
            pub cached_values: Vec<Value>,
            pub account_meta_cache: Vec<AccountMeta<'a>>,
            pub account_info_cache: Vec<Account<'a>>,
        }
        impl<'a> ExecutionState<'a> {
            pub fn purge_caches(&mut self) {
                self.account_meta_cache.clear();
                self.account_info_cache.clear();
            }
        }
    }
}
