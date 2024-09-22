use anchor_lang::prelude::AccountMeta;
use ballista_common::{
    logical_components::{Condition, Expression, Value, ValueType},
    schema::{
        AccountDefinition, ArgumentDefinition, InstructionDefinition, SerializationType,
        TaskDefinition,
    },
    task::{
        action::{
            schema_instruction::{SchemaInstruction, TaskAccount, TaskArgument},
            set_cache::SetCacheType,
            system_instruction::SystemInstructionAction,
        },
        task_action::TaskAction,
    },
};
use ballista_sdk::generated::instructions::{ExecuteTask, ExecuteTaskBuilder};
use mpl_bubblegum::state::metaplex_adapter::TokenStandard;
use mpl_bubblegum_sdk::{types::MetadataArgs, ID};
use solana_program::account_info;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

// pub fn create_system_transfer_instruction_definition() -> InstructionDefinition {
//     InstructionDefinition {
//         serialization: SerializationType::Borsh,
//         arguments: vec![
//             ArgumentDefinition::Constant {
//                 value: Value::Bytes(vec![2, 0, 0, 0]),
//             },
//             ArgumentDefinition::Input {
//                 value_type: Some(ValueType::U64),
//             },
//         ],
//         accounts: vec![
//             AccountDefinition {
//                 name: "from".to_string(),
//                 signer: true,
//                 writable: true,
//             },
//             AccountDefinition {
//                 name: "to".to_string(),
//                 signer: false,
//                 writable: true,
//             },
//         ],
//     }
// }

pub fn create_system_transfer_task_definition(batch_size: u8) -> TaskDefinition {
    TaskDefinition {
        actions: vec![
            TaskAction::SetCache(SetCacheType::Expression {
                index: 0,
                expression: Value::U8(0).expr(),
            }),
            TaskAction::loop_action(
                Condition::less_than(Expression::cached_value(0), Value::U8(batch_size).expr()),
                vec![
                    TaskAction::SystemInstruction(SystemInstructionAction::Transfer {
                        from: TaskAccount::FromInput(1),
                        to: TaskAccount::Evaluated(
                            Expression::CachedValue(0).checked_add(Value::U8(2).expr()),
                        ),
                        amount: Expression::static_value(0),
                    }),
                    TaskAction::SetCache(SetCacheType::Expression {
                        index: 0,
                        expression: Expression::cached_value(0).checked_add(Value::U8(1).expr()),
                    }),
                ],
            ),
        ],
        static_values: vec![Value::U64(1_000_000)],
    }
}

pub fn mint_bubblegum_nft_task_definition() -> TaskDefinition {
    TaskDefinition {
        actions: vec![
            TaskAction::SetCache(SetCacheType::Expression {
                index: 0,
                expression: Value::U8(0).expr(),
            }),
            TaskAction::loop_action(
                Condition::less_than(Expression::cached_value(0), Value::U8(15).expr()),
                vec![
                    (TaskAction::SchemaInstruction(SchemaInstruction {
                        instruction_id: 0,
                        program: TaskAccount::FromInput(0),
                        accounts: vec![
                            TaskAccount::FromInput(1),
                            TaskAccount::FromInput(2),
                            TaskAccount::FromInput(3),
                            TaskAccount::FromInput(4),
                            TaskAccount::FromInput(5),
                            TaskAccount::FromInput(6),
                            TaskAccount::FromInput(7),
                            TaskAccount::FromInput(8),
                            TaskAccount::FromInput(9),
                            // TaskAccount::FromInput(9),
                        ],
                        arguments: vec![TaskArgument::Expression(Expression::static_value(0))],
                    })),
                    TaskAction::SetCache(SetCacheType::Expression {
                        index: 0,
                        expression: Expression::cached_value(0).checked_add(Value::U8(1).expr()),
                    }),
                ],
            ),
        ],
        static_values: vec![Value::Struct(vec![
            ("name".to_string(), Value::String("NFT #1".to_string())),
            ("symbol".to_string(), Value::String("NFT".to_string())),
            (
                "uri".to_string(),
                Value::String("http://pooper.com/10.jpeg".to_string()),
            ),
            ("seller_fee_basis_points".to_string(), Value::U16(10_000)),
            ("primary_sale_happened".to_string(), Value::Bool(false)),
            ("is_mutable".to_string(), Value::Bool(true)),
            ("edition_nonce".to_string(), Value::Option(None)),
            (
                "token_standard".to_string(),
                Value::Option(Some(Box::new(Value::U8(0)))),
            ),
            ("collection".to_string(), Value::Option(None)),
            ("uses".to_string(), Value::Option(None)),
            ("token_program_version".to_string(), Value::U8(0)),
            ("creators".to_string(), Value::Vec(vec![])),
        ])],
    }
}

pub fn mint_bubblegum_nft_instruction_definition() -> InstructionDefinition {
    InstructionDefinition {
        serialization: SerializationType::Borsh,
        arguments: vec![
            ArgumentDefinition::Constant {
                value: Value::Bytes(vec![145, 98, 192, 118, 184, 147, 118, 104]),
            },
            ArgumentDefinition::Input { value_type: None },
        ],
        accounts: vec![
            AccountDefinition {
                name: "tree_authority".to_string(),
                signer: true,
                writable: true,
            },
            AccountDefinition {
                name: "leaf_owner".to_string(),
                signer: false,
                writable: false,
            },
            AccountDefinition {
                name: "leaf_delegate".to_string(),
                signer: false,
                writable: false,
            },
            AccountDefinition {
                name: "merkle_tree".to_string(),
                signer: false,
                writable: true,
            },
            AccountDefinition {
                name: "payer".to_string(),
                signer: true,
                writable: false,
            },
            AccountDefinition {
                name: "tree_delegate".to_string(),
                signer: true,
                writable: false,
            },
            AccountDefinition {
                name: "log_wrapper".to_string(),
                signer: false,
                writable: false,
            },
            AccountDefinition {
                name: "compression_program".to_string(),
                signer: false,
                writable: false,
            },
            AccountDefinition {
                name: "system_program".to_string(),
                signer: false,
                writable: false,
            },
        ],
    }
}

pub fn build_mint_bubblegum_nft_task_execution(
    schema: &Pubkey,
    tree_authority: &Pubkey,
    leaf_owner: &Pubkey,
    leaf_delegate: &Pubkey,
    merkle_tree: &Pubkey,
    payer: &Pubkey,
) -> Instruction {
    let instruction = ExecuteTaskBuilder::new()
        .task_values(vec![])
        .schema(*schema)
        .add_remaining_accounts(&vec![
            AccountMeta {
                pubkey: mpl_bubblegum::ID,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *tree_authority,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *leaf_owner,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *leaf_delegate,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *merkle_tree,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *payer,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                // tree_delegate
                pubkey: *payer,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: spl_noop::ID,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: spl_account_compression::ID,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: solana_program::system_program::ID,
                is_signer: false,
                is_writable: false,
            },
        ])
        .instruction();

    instruction
}
