use crate::utils::{
    ballista::definitions::builtin::system_program::transfer::{
        build_looping_execute_task_instruction, build_single_execute_task_instruction,
        create_looping_task_definition, create_single_task_definition, AmountSourceType,
    },
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
                    task: create_looping_task_definition(
                        Expression::StaticValue(0),
                        AmountSourceType::Static(Value::U64(100_000_000)),
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
                &[build_looping_execute_task_instruction(
                    &user.encodable_pubkey(),
                    &task_definition,
                    destinations.clone(),
                )],
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
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    task_definition: schema,
                },
                CreateTaskInstructionArgs {
                    task: create_single_task_definition(
                        Expression::StaticValue(0),
                        AmountSourceType::Static(Value::U64(100_000_000)),
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
                &[build_single_execute_task_instruction(
                    &user.encodable_pubkey(),
                    &schema,
                    &destination,
                )],
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
                    task: create_single_task_definition(
                        Expression::StaticValue(0),
                        AmountSourceType::Static(Value::U64(100_000_000)),
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
                &[build_single_execute_task_instruction(
                    &user.encodable_pubkey(),
                    &task_definition,
                    &destination,
                )],
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
                    task: create_looping_task_definition(
                        Expression::StaticValue(0),
                        AmountSourceType::Static(Value::U64(100_000_000)),
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
                &[build_looping_execute_task_instruction(
                    &user.encodable_pubkey(),
                    &task_definition,
                    destinations.clone(),
                )],
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
