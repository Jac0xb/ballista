use ballista_common::{
    accounts::task_definition::{ExecutionSettings, TaskDefinition},
    types::{
        logical_components::{Condition, Expression, Value},
        task::{
            action::{
                defined_instruction::{DefinedArgument, DefinedInstruction, SerializationType},
                set_cache::SetCacheType,
            },
            task_account::TaskAccount,
            task_action::TaskAction,
        },
    },
};
use num_derive::ToPrimitive;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::EncodableKeypair};
use strum::EnumIter;
use strum_macros::EnumCount as EnumCountMacro;

use crate::utils::ballista::definitions::instruction_schema::{
    build_defined_accounts, InstructionSchema,
};

#[derive(Eq, PartialEq, Debug, Copy, Clone, EnumIter, EnumCountMacro, ToPrimitive)]
#[repr(u8)]
pub enum MintBubblegumNftInstructionAccount {
    BubblegumProgram = 0,
    TreeAuthority = 1,
    LeafOwner = 2,
    LeafDelegate = 3,
    MerkleTree = 4,
    Payer = 5,
    TreeDelegate = 6,
    Noop = 7,
    AccountCompression = 8,
    SystemProgram = 9,
}

impl InstructionSchema for MintBubblegumNftInstructionAccount {
    type InstructionAccount = Self;
    type InstructionAccountParams = MintBubblegumNftAccountMetaParams;

    fn is_signer(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(account, MintBubblegumNftInstructionAccount::Payer)
    }

    fn is_writable(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(
            account,
            MintBubblegumNftInstructionAccount::Payer
                | MintBubblegumNftInstructionAccount::TreeAuthority
                | MintBubblegumNftInstructionAccount::MerkleTree
        )
    }

    fn get_pubkey(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> Pubkey {
        match account {
            MintBubblegumNftInstructionAccount::BubblegumProgram => mpl_bubblegum::ID,
            MintBubblegumNftInstructionAccount::TreeAuthority => params.tree_authority,
            MintBubblegumNftInstructionAccount::LeafOwner => params.leaf_owner,
            MintBubblegumNftInstructionAccount::LeafDelegate => params.leaf_delegate,
            MintBubblegumNftInstructionAccount::AccountCompression => spl_account_compression::ID,
            MintBubblegumNftInstructionAccount::MerkleTree => params.merkle_tree,
            MintBubblegumNftInstructionAccount::TreeDelegate => params.tree_delegate,
            MintBubblegumNftInstructionAccount::SystemProgram => solana_program::system_program::ID,
            MintBubblegumNftInstructionAccount::Payer => params.payer,
            MintBubblegumNftInstructionAccount::Noop => spl_noop::ID,
        }
    }

    fn get_payer(params: &Self::InstructionAccountParams) -> Pubkey {
        params.payer
    }
}

pub struct MintBubblegumNftAccountMetaParams {
    pub tree_authority: Pubkey,
    pub leaf_owner: Pubkey,
    pub leaf_delegate: Pubkey,
    pub merkle_tree: Pubkey,
    pub payer: Pubkey,
    pub tree_delegate: Pubkey,
}

pub fn mint_bubblegum_nft_task_definition(loop_count: u8) -> TaskDefinition {
    let default_params = MintBubblegumNftAccountMetaParams {
        tree_authority: Pubkey::default(),
        leaf_owner: Pubkey::default(),
        leaf_delegate: Pubkey::default(),
        merkle_tree: Pubkey::default(),
        payer: Pubkey::default(),
        tree_delegate: Pubkey::default(),
    };

    let defined_accounts =
        build_defined_accounts::<MintBubblegumNftInstructionAccount>(&default_params, |account| {
            *account == MintBubblegumNftInstructionAccount::BubblegumProgram
        });

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
                        accounts: defined_accounts.clone(),
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
            Value::String("Test #1000".to_string()),     // name
            Value::String("SYMBOL".to_string()),         // symbol
            Value::String("".to_string()),               // uri
            Value::U16(10_000),                          // seller_fee_basis_points
            Value::Bool(false),                          // primary_sale_happened
            Value::Bool(true),                           // is_mutable
            Value::Option(None),                         // edition_nonce
            Value::Option(Some(Box::new(Value::U8(0)))), // token_standard
            Value::Option(None),                         // collection
            Value::Option(None),                         // uses
            Value::U8(0),                                // token_program_version
            Value::Vec(vec![
                // creators
                Value::Struct(vec![
                    // creator
                    Value::Bytes(Keypair::new().encodable_pubkey().to_bytes().to_vec()), // creator address
                    Value::Bool(false),                                                  // verified
                    Value::U8(100),                                                      // share
                ]),
                // Value::Struct(vec![
                //     // creator
                //     Value::Bytes(Keypair::new().encodable_pubkey().to_bytes().to_vec()), // creator address
                //     Value::Bool(false),                                                  // verified
                //     Value::U8(50),                                                       // share
                // ]),
            ]),
        ])],
        account_groups: vec![],
    }
}
