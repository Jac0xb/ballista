use crate::utils::{
    process_transaction_assert_success, setup::user::create_user_with_balance,
    test_context::TestContext, transaction::utils::create_transaction,
};
use ballista_common::schema::Schema;
use ballista_sdk::{
    find_task_definition_pda,
    generated::instructions::{CreateSchema, CreateSchemaInstructionArgs},
    BALLISTA_ID,
};
use solana_program_test::tokio;
use solana_sdk::{signer::EncodableKeypair, system_program};

#[tokio::test]
async fn simple() {
    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let schema = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateSchema::instruction(
                &CreateSchema {
                    program_id: BALLISTA_ID,
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    schema,
                },
                CreateSchemaInstructionArgs {
                    schema_arg: Schema { tasks: vec![] },
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx)
        .await
        .unwrap();

    // let tx = create_transaction(
    //     context,
    //     vec![AddTaskBuilder::new()
    //         .payer(user.encodable_pubkey())
    //         .schema(schema)
    //         .task(create_system_transfer_task_definition(5, 10))
    //         .system_program(system_program::ID)
    //         .instruction()],
    //     &[&user],
    // )
    // .await;

    // process_transaction_assert_success(context, tx)
    //     .await
    //     .unwrap();
}
