use crate::utils::ballista::definitions::{
    instruction_schema::InstructionSchema, utils::actions_for_loop,
};
use anchor_lang::system_program;
use ballista_common::{
    accounts::task_definition::{ExecutionSettings, TaskDefinition},
    types::{
        logical_components::{Expression, Value},
        task::{
            action::defined_instruction::{
                DefinedAccount, DefinedArgument, DefinedInstruction, SerializationType,
            },
            task_account::TaskAccount,
            task_action::TaskAction,
        },
    },
};
use num_derive::ToPrimitive;
use solana_sdk::pubkey::Pubkey;
use strum::EnumIter;
use strum_macros::EnumCount as EnumCountMacro;

#[derive(Eq, PartialEq, Debug, Copy, Clone, EnumIter, EnumCountMacro, ToPrimitive)]
#[repr(u8)]
pub enum SystemProgramTransferInstructionAccount {
    SystemProgram = 0,
    From = 1,
    To = 2,
}

pub struct SystemProgramTransferInstructionAccountParams {
    pub system_program: Pubkey,
    pub from: Pubkey,
    pub to: Pubkey,
}

impl InstructionSchema for SystemProgramTransferInstructionAccount {
    type InstructionAccount = Self;
    type InstructionAccountParams = SystemProgramTransferInstructionAccountParams;

    fn is_signer(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(account, Self::From)
    }

    fn is_writable(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(account, Self::From | Self::To)
    }

    fn get_pubkey(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> Pubkey {
        match account {
            Self::From => params.from,
            Self::To => params.to,
            Self::SystemProgram => system_program::ID,
        }
    }

    fn get_payer(params: &Self::InstructionAccountParams) -> Pubkey {
        params.from
    }
}

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

fn create_instruction_definition(
    from: TaskAccount,
    to: TaskAccount,
    amount: Expression,
) -> DefinedInstruction {
    let system_program = TaskAccount::FromInput(0);

    let default_params = SystemProgramTransferInstructionAccountParams {
        system_program: system_program::ID,
        from: Pubkey::default(),
        to: Pubkey::default(),
    };

    let get_signer = |account: &SystemProgramTransferInstructionAccount| {
        SystemProgramTransferInstructionAccount::is_signer(account, &default_params)
    };

    let get_writable = |account: &SystemProgramTransferInstructionAccount| {
        SystemProgramTransferInstructionAccount::is_writable(account, &default_params)
    };

    DefinedInstruction {
        serialization_type: SerializationType::Borsh,
        program: system_program,
        arguments: vec![
            DefinedArgument {
                value: Expression::Literal(Value::Bytes(vec![2, 0, 0, 0])),
            },
            DefinedArgument { value: amount },
        ],
        accounts: vec![
            DefinedAccount {
                task_account: from,
                signer: get_signer(&SystemProgramTransferInstructionAccount::From),
                writable: get_writable(&SystemProgramTransferInstructionAccount::From),
            },
            DefinedAccount {
                task_account: to,
                signer: get_signer(&SystemProgramTransferInstructionAccount::To),
                writable: get_writable(&SystemProgramTransferInstructionAccount::To),
            },
        ],
    }
}

pub fn create_single_task_definition_from_defined(
    amount: Expression,
    amount_source_type: AmountSourceType,
) -> TaskDefinition {
    TaskDefinition {
        execution_settings: ExecutionSettings {
            preallocated_instruction_data_cache_size: None,
            preallocated_account_meta_cache_size: None,
            preallocated_account_info_cache_size: None,
        },
        actions: vec![TaskAction::DefinedInstruction(
            create_instruction_definition(
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
            vec![TaskAction::DefinedInstruction(
                create_instruction_definition(
                    TaskAccount::FromInput(1),
                    TaskAccount::Evaluated(
                        Expression::CachedValue(0).checked_add(&Value::U8(2).expr()),
                    ),
                    amount,
                ),
            )],
            &Expression::Literal(Value::U8(loop_count)),
            1,
        ),
        shared_values: amount_source_type.generic_static_values(),
        account_groups: vec![],
    }
}

// pub fn build_single_execute_task_instruction(
//     user: &Pubkey,
//     task: &Pubkey,
//     destinations: &Pubkey,
// ) -> Instruction {
//     let system_account = AccountMeta {
//         pubkey: system_program::ID,
//         is_signer: false,
//         is_writable: false,
//     };
//     let user_account = AccountMeta {
//         pubkey: *user,
//         is_signer: true,
//         is_writable: true,
//     };

//     let destination_account = AccountMeta {
//         pubkey: *destinations,
//         is_signer: false,
//         is_writable: true,
//     };

//     let remaining_accounts = vec![system_account, user_account, destination_account];

//     ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
//         &ExecuteTask {
//             task: *task,
//             payer: *user,
//         },
//         ExecuteTaskInstructionArgs {
//             input_values: vec![],
//         },
//         &remaining_accounts,
//     )
// }

// pub fn build_single_seeded_execute_task_instruction(
//     user: &Pubkey,
//     task: &Pubkey,
//     destinations: &Pubkey,
//     seed_index: u32,
// ) -> Instruction {
//     let system_account = AccountMeta {
//         pubkey: system_program::ID,
//         is_signer: false,
//         is_writable: false,
//     };

//     let (user_seeded_account, _) = find_seeded_pda(*user, seed_index);

//     let user_seeded_account_meta = AccountMeta {
//         pubkey: user_seeded_account,
//         is_signer: false,
//         is_writable: true,
//     };

//     let destination_account = AccountMeta {
//         pubkey: *destinations,
//         is_signer: false,
//         is_writable: true,
//     };

//     let remaining_accounts = vec![
//         system_account,
//         user_seeded_account_meta,
//         destination_account,
//     ];

//     ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
//         &ExecuteTask {
//             task: *task,
//             payer: *user,
//         },
//         ExecuteTaskInstructionArgs {
//             input_values: vec![],
//         },
//         &remaining_accounts,
//     )
// }

// pub fn build_looping_execute_task_instruction(
//     user: &Pubkey,
//     task: &Pubkey,
//     destinations: Vec<Pubkey>,
// ) -> Instruction {
//     let system_account = AccountMeta {
//         pubkey: system_program::ID,
//         is_signer: false,
//         is_writable: false,
//     };

//     let user_account = AccountMeta {
//         pubkey: *user,
//         is_signer: true,
//         is_writable: true,
//     };

//     let destination_accounts = destinations.into_iter().map(|destination| AccountMeta {
//         pubkey: destination,
//         is_signer: false,
//         is_writable: true,
//     });

//     let remaining_accounts = std::iter::once(system_account)
//         .chain(std::iter::once(user_account))
//         .chain(destination_accounts)
//         .collect::<Vec<_>>();

//     ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
//         &ExecuteTask {
//             task: *task,
//             payer: *user,
//         },
//         ExecuteTaskInstructionArgs {
//             input_values: vec![],
//         },
//         &remaining_accounts,
//     )
// }
