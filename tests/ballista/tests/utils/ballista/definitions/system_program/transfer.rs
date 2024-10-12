use anchor_lang::{prelude::AccountMeta, system_program};
use ballista_common::{
    logical_components::{Expression, Value},
    schema::{AccountGroupDefinition, ExecutionSettings, TaskDefinition},
    task::{
        action::{
            defined_instruction::{
                DefinedAccount, DefinedArgument, DefinedInstruction, SerializationType,
            },
            system_instruction::SystemInstructionAction,
        },
        shared::TaskAccount,
        task_action::TaskAction,
    },
};
use ballista_sdk::generated::instructions::{ExecuteTask, ExecuteTaskInstructionArgs};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

use crate::utils::ballista::definitions::utils::actions_for_loop;

pub enum SourceType {
    Static(Value),
    Cached(u8),
    FromInput(u8),
}

impl SourceType {
    fn generic_static_values(&self) -> Vec<Value> {
        match self {
            SourceType::Static(value) => vec![value.clone()],
            SourceType::Cached(_) => vec![],
            SourceType::FromInput(_) => vec![],
        }
    }
}

pub struct SystemTransferFactory {}

impl SystemTransferFactory {
    fn create_instruction_definition(
        from: TaskAccount,
        to: TaskAccount,
        amount: Expression,
    ) -> DefinedInstruction {
        DefinedInstruction {
            serialization_type: SerializationType::Borsh,
            program: TaskAccount::FromInput(0),
            arguments: vec![
                DefinedArgument {
                    value: Expression::Literal(Value::Bytes(vec![2, 0, 0, 0])),
                },
                DefinedArgument { value: amount },
            ],
            accounts: vec![
                DefinedAccount {
                    task_account: from,
                    signer: true,
                    writable: true,
                },
                DefinedAccount {
                    task_account: to,
                    signer: false,
                    writable: true,
                },
            ],
        }
    }

    fn create_builtin_action(from: TaskAccount, to: TaskAccount, amount: Expression) -> TaskAction {
        TaskAction::SystemInstruction(SystemInstructionAction::Transfer { from, to, amount })
    }

    pub fn create_single_task_definition(
        amount: Expression,
        amount_source_type: SourceType,
    ) -> TaskDefinition {
        TaskDefinition {
            execution_settings: ExecutionSettings {
                preallocated_instruction_data_cache_size: None,
                preallocated_account_meta_cache_size: None,
                preallocated_account_info_cache_size: None,
            },
            actions: vec![SystemTransferFactory::create_builtin_action(
                TaskAccount::FromInput(1),
                TaskAccount::FromInput(2),
                amount,
            )],
            shared_values: amount_source_type.generic_static_values(),
            account_groups: vec![],
        }
    }

    pub fn create_looping_task_definition(
        amount: Expression,
        amount_source_type: SourceType,
        loop_count: u8,
    ) -> TaskDefinition {
        TaskDefinition {
            execution_settings: ExecutionSettings {
                preallocated_instruction_data_cache_size: None,
                preallocated_account_meta_cache_size: None,
                preallocated_account_info_cache_size: None,
            },
            actions: actions_for_loop(
                vec![SystemTransferFactory::create_builtin_action(
                    TaskAccount::FromInput(1),
                    TaskAccount::FromGroup {
                        group_index: 0,
                        account_index: 0,
                    },
                    amount,
                )],
                &Expression::Literal(Value::U8(loop_count)),
            ),
            shared_values: amount_source_type.generic_static_values(),
            account_groups: vec![AccountGroupDefinition {
                account_offset: Expression::CachedValue(0).checked_add(&Value::U8(2).expr()),
                length: 1,
            }],
        }
    }

    pub fn create_single_task_definition_from_defined(
        amount: Expression,
        amount_source_type: SourceType,
    ) -> TaskDefinition {
        TaskDefinition {
            execution_settings: ExecutionSettings {
                preallocated_instruction_data_cache_size: None,
                preallocated_account_meta_cache_size: None,
                preallocated_account_info_cache_size: None,
            },
            actions: vec![TaskAction::DefinedInstruction(
                SystemTransferFactory::create_instruction_definition(
                    TaskAccount::FromInput(1),
                    TaskAccount::FromInput(2),
                    amount,
                ),
            )],
            shared_values: amount_source_type.generic_static_values(),
            account_groups: vec![],
        }
    }

    pub fn create_looping_task_definition_from_defined(
        amount: Expression,
        amount_source_type: SourceType,
        loop_count: u8,
    ) -> TaskDefinition {
        TaskDefinition {
            execution_settings: ExecutionSettings {
                preallocated_instruction_data_cache_size: None,
                preallocated_account_meta_cache_size: None,
                preallocated_account_info_cache_size: None,
            },
            actions: actions_for_loop(
                vec![TaskAction::DefinedInstruction(
                    SystemTransferFactory::create_instruction_definition(
                        TaskAccount::FromInput(1),
                        TaskAccount::Evaluated(
                            Expression::CachedValue(0).checked_add(&Value::U8(2).expr()),
                        ),
                        amount,
                    ),
                )],
                &Expression::Literal(Value::U8(loop_count)),
            ),
            shared_values: amount_source_type.generic_static_values(),
            account_groups: vec![],
        }
    }

    pub fn build_single_execute_task_instruction(
        user: &Pubkey,
        schema: &Pubkey,
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
                schema: *schema,
                payer: *user,
            },
            ExecuteTaskInstructionArgs {
                task_values: vec![],
            },
            &remaining_accounts,
        )
    }

    pub fn build_looping_execute_task_instruction(
        user: &Pubkey,
        schema: &Pubkey,
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
                schema: *schema,
                payer: *user,
            },
            ExecuteTaskInstructionArgs {
                task_values: vec![],
            },
            &remaining_accounts,
        )
    }
}
