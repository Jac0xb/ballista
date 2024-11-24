use anchor_lang::{prelude::AccountMeta, system_program};
use ballista_common::{
    accounts::task_definition::{AccountGroupDefinition, ExecutionSettings, TaskDefinition},
    types::{
        logical_components::{Expression, Value},
        task::{
            action::system_instruction::SystemInstructionAction, task_account::TaskAccount,
            task_action::TaskAction,
        },
    },
};
use ballista_sdk::{
    find_seeded_pda,
    generated::instructions::{ExecuteTask, ExecuteTaskInstructionArgs},
};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

use crate::utils::ballista::definitions::utils::actions_for_loop;

pub enum AmountSourceType {
    Static(Value),
    Cached(u8),
    FromInput(u8),
}

impl AmountSourceType {
    fn generic_static_values(&self) -> Vec<Value> {
        match self {
            AmountSourceType::Static(value) => vec![value.clone()],
            AmountSourceType::Cached(_) => vec![],
            AmountSourceType::FromInput(_) => vec![],
        }
    }
}

fn create_builtin_action(from: TaskAccount, to: TaskAccount, amount: Expression) -> TaskAction {
    TaskAction::SystemInstruction(SystemInstructionAction::Transfer { from, to, amount })
}

pub fn create_single_task_definition(
    amount: Expression,
    amount_source_type: AmountSourceType,
) -> TaskDefinition {
    TaskDefinition {
        execution_settings: ExecutionSettings {
            preallocated_instruction_data_cache_size: None,
            preallocated_account_meta_cache_size: None,
            preallocated_account_info_cache_size: None,
        },
        actions: vec![create_builtin_action(
            TaskAccount::FromInput(1),
            TaskAccount::FromInput(2),
            amount,
        )],
        shared_values: amount_source_type.generic_static_values(),
        account_groups: vec![],
    }
}

pub fn create_single_task_definition_with_seed(
    amount: Expression,
    amount_source_type: AmountSourceType,
    seed_index: u32,
) -> TaskDefinition {
    TaskDefinition {
        execution_settings: ExecutionSettings {
            preallocated_instruction_data_cache_size: None,
            preallocated_account_meta_cache_size: None,
            preallocated_account_info_cache_size: None,
        },
        actions: vec![
            create_builtin_action(
                TaskAccount::FeePayer,
                TaskAccount::FromInput(1),
                amount.clone(),
            ),
            create_builtin_action(
                TaskAccount::FromInputWithSeed {
                    index: 1,
                    seed: Some(seed_index),
                },
                TaskAccount::FromInput(2),
                amount,
            ),
        ],
        shared_values: amount_source_type.generic_static_values(),
        account_groups: vec![],
    }
}

pub fn create_looping_task_definition(
    amount: Expression,
    amount_source_type: AmountSourceType,
    loop_count: u8,
) -> TaskDefinition {
    TaskDefinition {
        execution_settings: ExecutionSettings {
            preallocated_instruction_data_cache_size: None,
            preallocated_account_meta_cache_size: None,
            preallocated_account_info_cache_size: None,
        },
        actions: actions_for_loop(
            vec![create_builtin_action(
                TaskAccount::FromInput(1),
                TaskAccount::FromGroup {
                    group_index: 0,
                    account_index: 0,
                },
                amount,
            )],
            &Expression::Literal(Value::U8(loop_count)),
            1,
        ),
        shared_values: amount_source_type.generic_static_values(),
        account_groups: vec![AccountGroupDefinition {
            account_offset: Expression::CachedValue(0).checked_add(&Value::U8(2).expr()),
            length: 1,
        }],
    }
}

pub fn build_single_execute_task_instruction(
    user: &Pubkey,
    task: &Pubkey,
    destinations: &Pubkey,
) -> Instruction {
    let system_account = AccountMeta {
        pubkey: system_program::ID,
        is_signer: false,
        is_writable: false,
    };
    let user_account = AccountMeta {
        pubkey: *user,
        is_signer: true,
        is_writable: true,
    };

    let destination_account = AccountMeta {
        pubkey: *destinations,
        is_signer: false,
        is_writable: true,
    };

    let remaining_accounts = vec![system_account, user_account, destination_account];

    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        &ExecuteTask {
            task_definition: *task,
            payer: *user,
        },
        ExecuteTaskInstructionArgs {
            input_values: vec![],
        },
        &remaining_accounts,
    )
}

pub fn build_single_seeded_execute_task_instruction(
    user: &Pubkey,
    task: &Pubkey,
    destinations: &Pubkey,
    seed_index: u32,
) -> Instruction {
    let system_account = AccountMeta {
        pubkey: system_program::ID,
        is_signer: false,
        is_writable: false,
    };

    let (user_seeded_account, _) = find_seeded_pda(*user, seed_index);

    let user_seeded_account_meta = AccountMeta {
        pubkey: user_seeded_account,
        is_signer: false,
        is_writable: true,
    };

    let destination_account = AccountMeta {
        pubkey: *destinations,
        is_signer: false,
        is_writable: true,
    };

    let remaining_accounts = vec![
        system_account,
        user_seeded_account_meta,
        destination_account,
    ];

    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        &ExecuteTask {
            task_definition: *task,
            payer: *user,
        },
        ExecuteTaskInstructionArgs {
            input_values: vec![],
        },
        &remaining_accounts,
    )
}

pub fn build_looping_execute_task_instruction(
    user: &Pubkey,
    task: &Pubkey,
    destinations: Vec<Pubkey>,
) -> Instruction {
    let system_account = AccountMeta {
        pubkey: system_program::ID,
        is_signer: false,
        is_writable: false,
    };

    let user_account = AccountMeta {
        pubkey: *user,
        is_signer: true,
        is_writable: true,
    };

    let destination_accounts = destinations.into_iter().map(|destination| AccountMeta {
        pubkey: destination,
        is_signer: false,
        is_writable: true,
    });

    let remaining_accounts = std::iter::once(system_account)
        .chain(std::iter::once(user_account))
        .chain(destination_accounts)
        .collect::<Vec<_>>();

    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        &ExecuteTask {
            task_definition: *task,
            payer: *user,
        },
        ExecuteTaskInstructionArgs {
            input_values: vec![],
        },
        &remaining_accounts,
    )
}
