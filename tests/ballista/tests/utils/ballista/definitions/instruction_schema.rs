use anchor_lang::prelude::AccountMeta;
use ballista_common::{
    logical_components::Value,
    task::{action::defined_instruction::DefinedAccount, shared::TaskAccount},
};
use ballista_sdk::{
    find_task_definition_pda,
    generated::instructions::{
        CreateTask, CreateTaskInstructionArgs, ExecuteTask, ExecuteTaskInstructionArgs,
    },
};
use num_traits::ToPrimitive;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::EncodableKeypair,
    system_program, transaction::VersionedTransaction,
};
use std::collections::HashMap;
use strum::{EnumCount, IntoEnumIterator};

use crate::utils::{test_context::TestContext, transaction::utils::create_transaction};

pub trait InstructionSchema {
    type InstructionAccount: IntoEnumIterator + EnumCount + Copy + ToPrimitive;
    type InstructionAccountParams;

    fn is_signer(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> bool;
    fn is_writable(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> bool;
    fn get_pubkey(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> Pubkey;

    fn get_payer(params: &Self::InstructionAccountParams) -> Pubkey;
}

pub fn get_account_meta<S: InstructionSchema>(
    account: &S::InstructionAccount,
    params: &S::InstructionAccountParams,
) -> AccountMeta {
    build_account_meta(
        S::get_pubkey(account, params),
        S::is_signer(account, params),
        S::is_writable(account, params),
    )
}

// trait Schema<T> {
// }

// pub trait GetPayer {
//     fn get_payer(&self) -> Pubkey;
// }

// pub trait InstructionSchemaParams {
//     type InstructionAccount: IntoEnumIterator + EnumCount + Copy;

// }

pub fn build_account_meta(pubkey: Pubkey, is_signer: bool, is_writable: bool) -> AccountMeta {
    match is_writable {
        true => AccountMeta::new(pubkey, is_signer),
        false => AccountMeta::new_readonly(pubkey, is_signer),
    }
}

// #[macro_export]
// macro_rules! impl_instruction_schema {
//     (
//     //   $schema_name:ident,
//       $account_enum:ident,
//       $build_params:ident,
//     //   $get_account_meta_fn:ident
//   ) => {
//         impl From<u8> for $enum_name {
//             fn from(value: u8) -> Self {
//                 unsafe { std::mem::transmute(value) }
//             }
//         }

//         impl From<$enum_name> for u8 {
//             fn from(account: $enum_name) -> Self {
//                 account as u8
//             }
//         }

//         // pub struct $schema_name;

//         impl $crate::utils::ballista::definitions::InstructionSchema<$build_params>
//             for account_enum
//         {
//             // type InstructionAccount = $enum_name;

//             fn get_account_meta(
//                 account: Self::InstructionAccount,
//                 params: &$build_params,
//             ) -> AccountMeta {
//                 $crate::utils::ballista::definitions::build_account_meta(
//                     params.get_pubkey(account),
//                     params.is_signer(account),
//                     params.is_writable(account),
//                 )
//             }
//         }
//     };
// }

pub fn build_remaining_accounts<S: InstructionSchema>(
    params: &S::InstructionAccountParams,
) -> Vec<AccountMeta> {
    let mut account_metas = HashMap::<u8, AccountMeta>::new();
    for account_index in S::InstructionAccount::iter() {
        let account_meta = get_account_meta::<S>(&account_index, params);
        // account_metas.insert(account_index.into(), account_meta);

        // let account_index_u8 = unsafe { std::mem::transmute(account_index) };
        account_metas.insert(account_index.to_u8().unwrap(), account_meta);
    }

    let mut remaining_accounts = Vec::<AccountMeta>::new();
    for i in 0u8..S::InstructionAccount::COUNT as u8 {
        remaining_accounts.push(account_metas[&i].clone());
    }

    remaining_accounts
}

pub fn execute_task_with_args_and_fn<T: InstructionSchema>(
    task: Pubkey,
    params: &T::InstructionAccountParams,
    task_values: Vec<Value>,
    additional_account_fn: fn(&T::InstructionAccountParams) -> Vec<AccountMeta>,
) -> Instruction {
    let mut remaining_accounts = build_remaining_accounts::<T>(params);

    remaining_accounts.extend(additional_account_fn(params));

    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        &ExecuteTask {
            task,
            payer: T::get_payer(params),
        },
        ExecuteTaskInstructionArgs { task_values },
        &remaining_accounts,
    )
}

pub fn execute_task_with_args<T: InstructionSchema>(
    task: Pubkey,
    params: &T::InstructionAccountParams,
    task_values: Vec<Value>,
) -> Instruction {
    execute_task_with_args_and_fn::<T>(task, params, task_values, |_| vec![])
}

pub async fn create_task_transaction(
    context: &mut TestContext,
    user: &Keypair,
    args: CreateTaskInstructionArgs,
) -> VersionedTransaction {
    let task_definition = find_task_definition_pda(user.encodable_pubkey(), 0).0;

    create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateTask::instruction(
                &CreateTask {
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    task_definition,
                },
                args,
            ),
        ],
        &[user],
    )
    .await
}

pub fn build_defined_accounts<T: InstructionSchema>(
    params: &T::InstructionAccountParams,
    skip_fn: fn(&T::InstructionAccount) -> bool,
) -> Vec<DefinedAccount> {
    let mut defined_accounts: Vec<DefinedAccount> = vec![];
    for account in T::InstructionAccount::iter() {
        if skip_fn(&account) {
            continue;
        }

        defined_accounts.push(DefinedAccount {
            writable: T::is_writable(&account, params),
            signer: T::is_signer(&account, params),
            task_account: TaskAccount::FromInput(account.to_u8().unwrap()),
        });
    }

    defined_accounts
}

// let tx = VersionedTransaction::try_new(
//   VersionedMessage::V0(
//       v0::Message::try_compile(
//           &user.encodable_pubkey(),
//           &[build_single_execute_task_instruction(
//               &user.encodable_pubkey(),
//               &schema,
//               &destination,
//           )],
//           &[],
//           context.get_blockhash().await,
//       )
//       .unwrap(),
//   ),
//   &[&user],
// )
// .unwrap();
