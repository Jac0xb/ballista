use anchor_lang::prelude::AccountMeta;
use ballista_common::{
    logical_components::{Condition, Expression, Value},
    schema::{ExecutionSettings, TaskDefinition},
    task::{
        action::{
            defined_instruction::{
                DefinedAccount, DefinedArgument, DefinedInstruction, SerializationType,
            },
            set_cache::SetCacheType,
        },
        shared::TaskAccount,
        task_action::TaskAction,
    },
};
use ballista_sdk::generated::instructions::ExecuteTaskBuilder;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::EncodableKeypair,
};

pub fn mint_bubblegum_nft_task_definition(loop_count: u8) -> TaskDefinition {
    TaskDefinition {
        execution_settings: ExecutionSettings {
            preallocated_instruction_data_cache_size: None,
            preallocated_account_meta_cache_size: None,
            preallocated_account_info_cache_size: None,
        },
        actions: vec![
            TaskAction::SetCache(SetCacheType::Expression {
                index: 0,
                expression: Value::U8(0).expr(),
            }),
            TaskAction::loop_action(
                Condition::less_than(Expression::cached_value(0), Value::U8(loop_count).expr()),
                vec![
                    TaskAction::DefinedInstruction(DefinedInstruction {
                        program: TaskAccount::FromInput(0),
                        accounts: vec![
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(1),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(2),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(3),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(4),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(5),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(6),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(7),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(8),
                            },
                            DefinedAccount {
                                writable: false,
                                signer: false,
                                task_account: TaskAccount::FromInput(9),
                            },
                        ],
                        arguments: vec![
                            DefinedArgument {
                                value: Expression::literal(Value::Bytes(vec![
                                    145, 98, 192, 118, 184, 147, 118, 104,
                                ])),
                            },
                            DefinedArgument {
                                value: Expression::shared_value(0),
                            },
                        ],
                        serialization_type: SerializationType::Borsh,
                    }),
                    TaskAction::SetCache(SetCacheType::Expression {
                        index: 0,
                        expression: Expression::cached_value(0).checked_add(&Value::U8(1).expr()),
                    }),
                ],
            ),
        ],
        shared_values: vec![Value::Struct(vec![
            ("name".to_string(), Value::String("Test #1000".to_string())),
            ("symbol".to_string(), Value::String("SYMBOL".to_string())),
            ("uri".to_string(), Value::String("".to_string())),
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
            (
                "creators".to_string(),
                Value::Vec(vec![Value::Struct(vec![
                    (
                        "address".to_string(),
                        Value::Bytes(Keypair::new().encodable_pubkey().to_bytes().to_vec()),
                    ),
                    ("verified".to_string(), Value::Bool(false)),
                    ("share".to_string(), Value::U8(100)),
                ])]),
            ),
        ])],
        account_groups: vec![],
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
        .payer(*payer)
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
