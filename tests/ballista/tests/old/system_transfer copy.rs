
use anchor_lang::prelude::AccountMeta;
use anchor_lang::system_program;
use ballista_common::task_definition::expression::condition::Condition;
use ballista_common::task_definition::expression::{ArithmeticBehavior, Expression};
use ballista_common::task_definition::instruction::{AccountDefinition, ArgumentDefinition, InstructionDefinition, SerializationType};
use ballista_common::task_definition::task_definition::GlobalSchema;
use ballista_common::task_definition::task::action::record_value::RecordValue;
use ballista_common::task_definition::task::action::schema_instruction::{SchemaInstruction, TaskAccount, TaskArgument};
use ballista_common::task_definition::task::task::{TaskAction, TaskDefinition};
use ballista_common::task_definition::value::{Value, ValueType};
use ballista_sdk::generated::instructions::{
    CreateSchema, CreateSchemaInstructionArgs, ExecuteTask, ExecuteTaskInstructionArgs,
};
use ballista_sdk::{find_task_definition_pda, BALLISTA_ID};
use solana_program_test::tokio;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::signature::Keypair;
use solana_sdk::transaction::VersionedTransaction;
use solana_sdk::signer::EncodableKeypair;

use crate::utils::process_transaction_assert_success;
use crate::utils::{context::TestContext, create_user_with_balance};


#[tokio::test]
async fn simple() {
    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let dest = Keypair::new();

    // 32 random pubkeys from keypairs
    // let mut accounts = (0..4)
    //     .map(|_| AccountMeta::new(Keypair::new().encodable_pubkey(), false))
    //     .collect::<Vec<_>>();

    // accounts.push(AccountMeta::new(System::id(), false));

    // let lamport_transfer_ix = transfer(
    //     &user.encodable_pubkey(),
    //     &user.encodable_pubkey(),
    //     100_000_000,
    // );

    // println!(
    //     "ix: {:?}",
    //     ballista_sdk::generated::instructions::CreateSchema::instruction(
    //         &CreateSchema {
    //             program_id: BALLISTA_ID,
    //             system_program: system_program::ID,
    //             payer: user.encodable_pubkey(),
    //             schema: find_task_definition_pda(user.encodable_pubkey(), 0).0,
    //         },
    //         CreateSchemaInstructionArgs {
    //             instructions: vec![],
    //             tasks: vec![],
    //         },
    //     )
    // );

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    ballista_sdk::generated::instructions::CreateSchema::instruction(
                        &CreateSchema {
                            program_id: BALLISTA_ID,
                            system_program: system_program::ID,
                            payer: user.encodable_pubkey(),
                            schema: find_task_definition_pda(user.encodable_pubkey(), 0).0,
                        },
                        CreateSchemaInstructionArgs {
                            schema_arg: GlobalSchema {
                            instructions: vec![InstructionDefinition {
                                serialization: SerializationType::Borsh,
                                arguments: vec![
                                    ArgumentDefinition::Constant {
                                        value: Value::Bytes(vec![2, 0, 0, 0]),
                                    },
                                    ArgumentDefinition::Input {
                                        value_type: ValueType::U64,
                                    },
                                ],
                                accounts: vec![
                                    AccountDefinition {
                                        name: "from".to_string(),
                                        signer: true,
                                        writable: true,
                                        validate: None,
                                    },
                                    AccountDefinition {
                                        name: "to".to_string(),
                                        signer: false,
                                        writable: true,
                                        validate: None,
                                    },
                                ],
                            }],
                            tasks: vec![TaskDefinition {
                                actions: vec![
                                    TaskAction::RecordValue(
                                        RecordValue::Expression {
                                            index: 0,
                                            expression: Expression::Literal(Value::U8(0)),
                                        }
                                    ),
                                    TaskAction::SchemaInstruction(SchemaInstruction {
                                        program: TaskAccount::Input(0),
                                        instruction_id: 0,
                                        accounts: vec![
                                            TaskAccount::Input(1),
                                            TaskAccount::Input(2),
                                        ],
                                        arguments: vec![
                                            TaskArgument::Expression( Expression::Multiply(
                                                    Box::new(Expression::SafeCast(
                                                        Box::new(Expression::InputValue(0)),
                                                        ValueType::U64,
                                                    )
                                                ),
                                                Box::new(Expression::StaticValue(0)),
                                                ArithmeticBehavior::Checked,
                                            ))
                                        ],
                                    }),
                                    TaskAction::RecordValue(
                                        RecordValue::Expression {
                                            index: 0,
                                            expression: Expression::Add(
                                                Box::new(Expression::CachedValue(0)),
                                                Box::new(Expression::Literal(Value::U8(1))),
                                                ArithmeticBehavior::Checked,
                                            ),
                                        }
                                    ),
                                    TaskAction::Branch { 
                                        condition: Condition::LessThan(
                                            Box::new(Expression::CachedValue(0)),
                                            Box::new(Expression::Literal(Value::U8(10)))
                                        ), 
                                        true_action: Expression::Literal(Value::U8(1)),
                                        false_action: Expression::Literal(Value::U8(128)),
                                    },
                                ],
                                shared_values: vec![Value::U64(10_000_000)],
                                }],
                            },
                        },
                    ),
                ],
                &[],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&user],
    )
    .unwrap();

    process_transaction_assert_success(context, tx)
        .await
        .unwrap();

    let dest_prebalance = 0i64;

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
                        &ExecuteTask {
                            schema: find_task_definition_pda(user.encodable_pubkey(), 0).0,
                        },
                        ExecuteTaskInstructionArgs {
                            input_values: vec![Value::U8(10)],
                        },
                    
                   &[
                        AccountMeta::new_readonly(system_program::ID, false),
                        AccountMeta::new(user.encodable_pubkey(), true),
                        AccountMeta::new(dest.encodable_pubkey(), false),
                   ]
                    ),
                ],
                &[],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&user],
    )
    .unwrap();

    println!("tx: {:?}", tx);

    process_transaction_assert_success(context, tx)
        .await
        .unwrap();

    let dest_postbalance = context
        .client()
        .get_account(dest.encodable_pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    println!("pre  dest balance: {:?}", dest_prebalance);
    println!("post dest balance: {:?}", dest_postbalance);

    assert_eq!(dest_postbalance as i64 - dest_prebalance, 10_000_000 * 10 * 10);

    // // context.warp_to_slot(10_000).unwrap();

    // // let mint = Keypair::new();
    // // // let create

    // // let create_account_ix = create_account(
    // //     &user.encodable_pubkey(),
    // //     &mint.encodable_pubkey(),
    // //     context
    // //         .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)
    // //         .await,
    // //     spl_token::state::Mint::LEN as u64,
    // //     &spl_token::ID,
    // // );
    // // let create_mint_ix = spl_token::instruction::initialize_mint(
    // //     &spl_token::ID,
    // //     &mint.encodable_pubkey(),
    // //     &user.encodable_pubkey(),
    // //     None,
    // //     0,
    // // )
    // // .unwrap();
    // // let mint_to_ix = spl_token::instruction::mint_to(
    // //     &spl_token::ID,
    // //     &mint.encodable_pubkey(),
    // //     &get_associated_token_address(&user.encodable_pubkey(), &mint.encodable_pubkey()),
    // //     &user.encodable_pubkey(),
    // //     &[],
    // //     10,
    // // )
    // // .unwrap();

    // // let token_transfer_ix = spl_token::instruction::transfer(
    // //     &spl_token::ID,
    // //     &get_associated_token_address(&user.encodable_pubkey(), &spl_token::ID),
    // //     &get_associated_token_address(&dest.encodable_pubkey(), &spl_token::ID),
    // //     // &dest.encodable_pubkey(),
    // //     &user.encodable_pubkey(),
    // //     &[],
    // //     10,
    // // )
    // // .unwrap();

    // // println!("transfer {:?}", token_transfer_ix.clone());

    // // let (create_lut_ix, lut_table_pubkey) = create_lookup_table(
    // //     user.encodable_pubkey(),
    // //     user.encodable_pubkey(),
    // //     context
    // //         .client()
    // //         .get_slot_with_context(
    // //             context::current(),
    // //             solana_sdk::commitment_config::CommitmentLevel::Confirmed,
    // //         )
    // //         .await
    // //         .unwrap()
    // //         - 1,
    // // );

    // // let create_lut_tx = VersionedTransaction::try_new(
    // //     VersionedMessage::V0(
    // //         v0::Message::try_compile(
    // //             &user.encodable_pubkey(),
    // //             &[
    // //                 create_lut_ix,
    // //                 extend_lookup_table(
    // //                     lut_table_pubkey,
    // //                     user.encodable_pubkey(),
    // //                     Some(user.encodable_pubkey()),
    // //                     vec![
    // //                         get_associated_token_address(&user.encodable_pubkey(), &spl_token::ID),
    // //                         spl_token::ID,
    // //                     ],
    // //                 ),
    // //             ],
    // //             &[],
    // //             context.get_blockhash().await,
    // //         )
    // //         .unwrap(),
    // //     ),
    // //     &[&user],
    // // )
    // // .unwrap();

    // // process_transaction_assert_success(context, create_lut_tx)
    // //     .await
    // //     .unwrap();

    // let values = vec![Value::U8(10)];

    // println!("values: {:?}", values.try_to_vec().unwrap());
    // println!("value: {:?}", Value::U8(10).try_to_vec().unwrap());

    // let tx = VersionedTransaction::try_new(
    //     VersionedMessage::V0(
    //         v0::Message::try_compile(
    //             &user.encodable_pubkey(),
    //             &[token_transfer_ix],
    //             &[
    //             //     AddressLookupTableAccount {
    //             //     key: lut_table_pubkey,
    //             //     addresses: vec![
    //             //         get_associated_token_address(&user.encodable_pubkey(), &spl_token::ID),
    //             //         spl_token::ID,
    //             //     ],
    //             // }
    //             ],
    //             context.get_blockhash().await,
    //         )
    //         .unwrap(),
    //     ),
    //     &[&user],
    //     //
    //     // & [
    //     //     // Instruction {
    //     //     //     program_id: lighthouse_sdk::ID,
    //     //     //     accounts: vec![
    //     //     //         AccountMeta::new_readonly(System::id(), false),
    //     //     //         AccountMeta::new(user.encodable_pubkey(), true),
    //     //     //         AccountMeta::new(dest.encodable_pubkey(), false),
    //     //     //         AccountMeta::new(dest2.encodable_pubkey(), false),
    //     //     //         AccountMeta::new(dest3.encodable_pubkey(), false),
    //     //     //         AccountMeta::new(dest4.encodable_pubkey(), false),
    //     //     //     ],
    //     //     //     data: values.try_to_vec().unwrap(),
    //     //     // },
    //     //     // create_account_ix,
    //     //     // create_mint_ix,
    //     //     // mint_to_ix,
    //     //     token_transfer_ix,
    //     // ],
    //     // }
    //     // Some(&user.encodable_pubkey()),
    //     // &[&user], //, &mint],
    //     // context.get_blockhash().await,
    // )
    // .unwrap();

    // // println!("transaction: {:?}", tx.);
    // println!("transaction size: {:?}", tx.message.instructions()[0]);

    // let pre_user_account_balance = context
    //     .client()
    //     .get_account(user.encodable_pubkey())
    //     .await
    //     .unwrap()
    //     .unwrap()
    //     .lamports;

    // let result = process_transaction_assert_success(context, tx)
    //     .await
    //     .unwrap();

    // let post_user_account_balance = context
    //     .client()
    //     .get_account(user.encodable_pubkey())
    //     .await
    //     .unwrap()
    //     .unwrap()
    //     .lamports;

    // println!("{:?}", result.metadata.unwrap().compute_units_consumed);
    // println!(
    //     "{:?}",
    //     post_user_account_balance as i128 - pre_user_account_balance as i128
    // );

    panic!("test");
}
