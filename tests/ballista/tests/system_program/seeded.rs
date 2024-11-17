use crate::utils::{
    ballista::definitions::{
        builtin::system_program::transfer::{
            build_single_seeded_execute_task_instruction, create_single_task_definition_with_seed,
            AmountSourceType,
        },
        instruction_schema::create_task_transaction,
    },
    jupiter::transaction_cloning::print_transaction_info,
    process_transaction_assert_success,
    record::TestLogger,
    setup::user::create_user_with_balance,
    test_context::TestContext,
};
use ballista_common::logical_components::{Expression, Value};
use ballista_sdk::{find_task_definition_pda, generated::instructions::CreateTaskInstructionArgs};
use solana_program_test::tokio;
use solana_sdk::{
    message::{v0, VersionedMessage},
    signature::Keypair,
    signer::EncodableKeypair,
    transaction::VersionedTransaction,
};

#[tokio::test]
async fn seeded_transfer() {
    let mut logger = TestLogger::new("system_program", "seeded_transfer").unwrap();

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let tx = create_task_transaction(
        context,
        &user,
        CreateTaskInstructionArgs {
            task: create_single_task_definition_with_seed(
                Expression::StaticValue(0),
                AmountSourceType::Static(Value::U64(100_000_000)),
                0,
            ),
            task_id: 0,
        },
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let destination = Keypair::new().encodable_pubkey();

    let schema = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[build_single_seeded_execute_task_instruction(
                    &user.encodable_pubkey(),
                    &schema,
                    &destination,
                    0,
                )],
                &[],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&user],
    )
    .unwrap();

    print_transaction_info(context, &tx, &mut logger)
        .await
        .unwrap();

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let balance = context.get_account(destination).await.unwrap();
    assert_eq!(balance.lamports, 100_000_000);
}
