use crate::utils::{
    ballista::definitions::system_program::transfer::{SourceType, SystemTransferFactory},
    process_transaction_assert_success,
    record::TestLogger,
    setup::user::create_user_with_balance,
    test_context::TestContext,
    transaction::utils::create_transaction,
};
use ballista_common::logical_components::{Expression, Value};
use ballista_sdk::{
    find_task_definition_pda,
    generated::instructions::{CreateTask, CreateTaskInstructionArgs},
};
use solana_program_test::tokio;
use solana_sdk::{
    message::{v0, VersionedMessage},
    signature::Keypair,
    signer::EncodableKeypair,
    system_program,
    transaction::VersionedTransaction,
};

// #[tokio::test]
// async fn test() {
//     let context = &mut TestContext::new().await.unwrap();
//     let client = reqwest::Client::new();
//     let rpc_client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

//     let user = create_user_with_balance(context, 10e9 as u64)
//         .await
//         .unwrap();

//     // InputValue(0) -> Jup Swap Raw Bytes
//     // InputValue(1) -> Start of Jup Swap Accounts
//     // InputValue(2) -> Length of Jup Swap Accounts

//     // Byte offset
//     let token_account_amount_offset = Expression::Literal(Value::U64(64));

//     // Expression to pull the balance out of token account.
//     let from_token_balance_expr = Expression::ValueFromAccountData {
//         index: Box::new(TaskAccount::FromInput(0)),
//         offset: Box::new(token_account_amount_offset.clone()),
//         value_type: ValueType::U64,
//     };
//     let to_token_balance_expr = Expression::ValueFromAccountData {
//         index: Box::new(TaskAccount::FromInput(1)),
//         offset: Box::new(token_account_amount_offset),
//         value_type: ValueType::U64,
//     };

//     let jup_swap_task = TaskDefinition {
//         actions: vec![
//             // Execute jupiter swap using instructions from API.
//             TaskAction::RawInstruction({
//                 RawInstruction {
//                     program: TaskAccount::FromInput(2),
//                     data: Expression::InputValue(0),
//                     accounts: ballista_common::task::shared::TaskAccounts::Evaluated {
//                         start: Expression::InputValue(1),
//                         length: Expression::InputValue(2),
//                     },
//                 }
//             }),
//             // Transfer exact amount of tokens from input token account to output token account
//             TaskAction::TokenProgramInstruction(TokenProgramInstruction::Transfer {
//                 program_version: TokenProgramVersion::Legacy,
//                 multisig: None,
//                 from: TaskAccount::FeePayer,
//                 from_token_account: TaskAccount::FromGroup {
//                     group_index: 0,
//                     account_index: 0,
//                 },
//                 to_token_account: TaskAccount::FromGroup {
//                     group_index: 0,
//                     account_index: 1,
//                 },
//                 amount: from_token_balance_expr.clone(),
//             }),
//             // Log amount transferred + amount
//             TaskAction::Log(Expression::Literal(Value::String(
//                 "Transferred".to_string(),
//             ))),
//             TaskAction::Log(to_token_balance_expr),
//         ],
//         shared_values: vec![],
//         account_groups: vec![AccountGroupDefinition {
//             account_offset: Expression::Literal(Value::U8(0)),
//             length: 2,
//         }],
//     };

//     // Create jup schema
//     let schema = find_task_definition_pda(user.encodable_pubkey(), 0).0;
//     let tx = create_transaction(
//         context,
//         vec![
//             ballista_sdk::generated::instructions::CreateSchema::instruction(
//                 &CreateSchema {
//                     program_id: BALLISTA_ID,
//                     system_program: system_program::ID,
//                     payer: user.encodable_pubkey(),
//                     schema,
//                 },
//                 CreateSchemaInstructionArgs {
//                     schema_arg: Schema {
//                         tasks: vec![jup_swap_task],
//                     },
//                 },
//             ),
//         ],
//         &[&user],
//     )
//     .await;

//     process_transaction_assert_success(context, tx)
//         .await
//         .unwrap();

//     let mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();

//     let SwapInstructions {
//         setup_instructions,
//         swap_instruction,
//     } = clone_swap_instructions(
//         context,
//         &user.encodable_pubkey(),
//         &client,
//         &rpc_client,
//         SwapArgs {
//             amount: 1_000_000_000,
//             input_mint: "So11111111111111111111111111111111111111112",
//             output_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
//             max_accounts: 20,
//             slippage_bps: 1_000,
//         },
//     )
//     .await
//     .unwrap();

//     let mut instructions = setup_instructions.clone();

//     let token_account = get_associated_token_address(&user.encodable_pubkey(), &mint);

//     let destination = Keypair::new().encodable_pubkey();
//     let destination_token_account = get_associated_token_address(&destination, &mint);

//     println!("token_account pubkey {:?}", token_account);

//     let mut remaining_accounts = vec![
//         AccountMeta {
//             pubkey: token_account,
//             is_signer: false,
//             is_writable: true,
//         },
//         AccountMeta {
//             pubkey: destination_token_account,
//             is_signer: false,
//             is_writable: true,
//         },
//         AccountMeta {
//             pubkey: swap_instruction.program_id,
//             is_signer: false,
//             is_writable: false,
//         },
//     ];

//     for account in &swap_instruction.accounts {
//         remaining_accounts.push(account.clone());
//     }

//     let mut account_data = vec![0; Account::LEN];
//     Account::pack(
//         Account {
//             amount: 0,
//             owner: destination,
//             delegated_amount: 0,
//             mint,
//             delegate: COption::None,
//             state: AccountState::Initialized,
//             is_native: COption::None,
//             close_authority: COption::None,
//         },
//         &mut account_data,
//     )
//     .unwrap();

//     set_account_from_refs(
//         context,
//         &destination_token_account,
//         &account_data,
//         &token::ID,
//     )
//     .await;

//     instructions.push(
//         ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
//             &ExecuteTask {
//                 schema,
//                 payer: user.encodable_pubkey(),
//             },
//             ExecuteTaskInstructionArgs {
//                 task_values: vec![
//                     Value::Bytes(swap_instruction.data),
//                     Value::U8(3),
//                     Value::U8(swap_instruction.accounts.len() as u8),
//                 ],
//             },
//             &remaining_accounts,
//         ),
//     );

//     // let signed_tx = VersionedTransaction::try_new(tx.message.clone(), &[&user]).unwrap();

//     let mut tx = VersionedTransaction::try_new(
//         VersionedMessage::V0(
//             v0::Message::try_compile(
//                 &user.encodable_pubkey(),
//                 &instructions,
//                 &[] as &[AddressLookupTableAccount],
//                 context.get_blockhash().await,
//             )
//             .unwrap(),
//         ),
//         &[&user],
//     )
//     .unwrap();

//     // Copy all the accounts from the RPC to the test context
//     copy_accounts_from_transaction(context, &tx, &rpc_client).await;

//     tx.message
//         .set_recent_blockhash(context.get_blockhash().await);

//     let signed_tx = VersionedTransaction::try_new(tx.message.clone(), &[&user]).unwrap();

//     process_transaction_assert_success(context, signed_tx)
//         .await
//         .unwrap();

//     let token_account_data = context.get_account(token_account).await.unwrap();

//     let token_account_deserialized = Account::unpack(token_account_data.data.as_slice()).unwrap();

//     print_transaction_info(context, &tx).await.unwrap();

//     assert_eq!(token_account_deserialized.amount, 0);
// }

#[tokio::test]
async fn looping_builtin() {
    let mut logger = TestLogger::new("system_program", "looping_builtin").unwrap();

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let task_definition = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateTask::instruction(
                &CreateTask {
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    task_definition,
                },
                CreateTaskInstructionArgs {
                    task: SystemTransferFactory::create_looping_task_definition(
                        Expression::StaticValue(0),
                        SourceType::Static(Value::U64(100_000_000)),
                        29,
                    ),
                    task_id: 0,
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let mut destinations = vec![];
    for _ in 0..29 {
        destinations.push(Keypair::new().encodable_pubkey());
    }

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    SystemTransferFactory::build_looping_execute_task_instruction(
                        &user.encodable_pubkey(),
                        &task_definition,
                        destinations.clone(),
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

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    for i in 0..29 {
        let pubkey = destinations.get(i).unwrap();
        let balance = context.get_account(*pubkey).await.unwrap();
        assert_eq!(balance.lamports, 100_000_000);
    }
}

#[tokio::test]
async fn single_builtin() {
    let mut logger = TestLogger::new("system_program", "single_builtin").unwrap();

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let schema = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateTask::instruction(
                &CreateTask {
                    // program_id: BALLISTA_ID,
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    task_definition: schema,
                },
                CreateTaskInstructionArgs {
                    task: SystemTransferFactory::create_single_task_definition(
                        Expression::StaticValue(0),
                        SourceType::Static(Value::U64(100_000_000)),
                    ),
                    task_id: 0,
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let destination = Keypair::new().encodable_pubkey();

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    SystemTransferFactory::build_single_execute_task_instruction(
                        &user.encodable_pubkey(),
                        &schema,
                        &destination,
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

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let balance = context.get_account(destination).await.unwrap();
    assert_eq!(balance.lamports, 100_000_000);
}

#[tokio::test]
async fn fixed_defined() {
    let mut logger = TestLogger::new("system_program", "fixed_defined").unwrap();

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let task_definition = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateTask::instruction(
                &CreateTask {
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    task_definition,
                },
                CreateTaskInstructionArgs {
                    task: SystemTransferFactory::create_single_task_definition_from_defined(
                        Expression::StaticValue(0),
                        SourceType::Static(Value::U64(100_000_000)),
                    ),
                    task_id: 0,
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let destination = Keypair::new().encodable_pubkey();

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    SystemTransferFactory::build_single_execute_task_instruction(
                        &user.encodable_pubkey(),
                        &task_definition,
                        &destination,
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

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let balance = context.get_account(destination).await.unwrap();
    assert_eq!(balance.lamports, 100_000_000);
}

#[tokio::test]
async fn looping_defined() {
    let mut logger = TestLogger::new("system_program", "looping_defined").unwrap();

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let task_definition = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateTask::instruction(
                &CreateTask {
                    // program_id: BALLISTA_ID,
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    task_definition,
                },
                CreateTaskInstructionArgs {
                    task: SystemTransferFactory::create_looping_task_definition_from_defined(
                        Expression::StaticValue(0),
                        SourceType::Static(Value::U64(100_000_000)),
                        29,
                    ),
                    task_id: 0,
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let mut destinations = vec![];
    for _ in 0..29 {
        destinations.push(Keypair::new().encodable_pubkey());
    }

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    SystemTransferFactory::build_looping_execute_task_instruction(
                        &user.encodable_pubkey(),
                        &task_definition,
                        destinations.clone(),
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

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    for i in 0..29 {
        let pubkey = destinations.get(i).unwrap();
        let balance = context.get_account(*pubkey).await.unwrap();
        assert_eq!(balance.lamports, 100_000_000);
    }
}
