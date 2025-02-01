use crate::utils::{
    ballista::definitions::{
        builtin::{
            self,
            system_program::transfer::{
                create_looping_task_definition, AmountSourceType,
                BatchSystemTransferInstructionAccount,
            },
        },
        instruction_schema::execute_task_no_args_with_args_and_fn,
    },
    process_transaction_assert_success,
    record::TestLogger,
    setup::user::create_user_with_balance,
    solana::lookuptable::create_lut,
    test_context::{create_test_context, TestContext},
    transaction::utils::{create_transaction, create_transaction_with_lookup_table},
};
use ballista_common::types::logical_components::{Expression, Value};
use ballista_sdk::{
    find_task_definition_pda,
    generated::instructions::{CreateTask, CreateTaskInstructionArgs},
};
use solana_program_test::tokio;
use solana_sdk::{
    bs58,
    instruction::{AccountMeta, Instruction},
    msg,
    signature::Keypair,
    signer::EncodableKeypair,
    system_instruction, system_program,
};

const BALLISTA_SYSTEM_TRANSFER_COUNT: usize = 63;
const NATIVE_SYSTEM_TRANSFER_COUNT: usize = 57;

#[tokio::test]
async fn ballista() {
    let mut logger = TestLogger::new("comparison", "ballista-system_transfer_lut_unique").unwrap();

    let context = &mut create_test_context().await.unwrap();
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
                        BALLISTA_SYSTEM_TRANSFER_COUNT as u8,
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
    for _ in 0..BALLISTA_SYSTEM_TRANSFER_COUNT {
        destinations.push(Keypair::new().encodable_pubkey());
    }

    let lookup_table = create_lut(context, &user, &destinations, &mut logger).await;

    let tx = create_transaction_with_lookup_table(
        context,
        vec![execute_task_no_args_with_args_and_fn::<
            BatchSystemTransferInstructionAccount,
        >(
            task_definition,
            &builtin::system_program::transfer::AccountMetaParams {
                user: user.encodable_pubkey(),
            },
            destinations
                .iter()
                .map(|d| AccountMeta::new(*d, false))
                .collect(),
        )],
        lookup_table,
        &[&user],
    )
    .await;

    println!("tx info {:?}", tx);

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    for i in 0..BALLISTA_SYSTEM_TRANSFER_COUNT {
        let pubkey = destinations.get(i).unwrap();
        let balance = context.get_account(*pubkey).await.unwrap();
        assert_eq!(balance.lamports, 100_000_000);
    }
}

#[tokio::test]
async fn native() {
    let mut logger = TestLogger::new("comparison", "native-system_transfer_unique").unwrap();

    let context = &mut create_test_context().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let mut ixs = vec![];

    let mut destinations = vec![];

    for i in 0..NATIVE_SYSTEM_TRANSFER_COUNT {
        destinations.push(Keypair::new().encodable_pubkey());
        ixs.push(system_instruction::transfer(
            &user.encodable_pubkey(),
            destinations.get(i).unwrap(),
            100_000_000,
        ));
    }

    let lookup_table = create_lut(context, &user, &destinations, &mut logger).await;

    let tx = create_transaction_with_lookup_table(context, ixs, lookup_table, &[&user]).await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    for i in 0..NATIVE_SYSTEM_TRANSFER_COUNT {
        let pubkey = destinations.get(i).unwrap();
        let balance = context.get_account(*pubkey).await.unwrap();
        assert_eq!(balance.lamports, 100_000_000);
    }
}

pub async fn log_instruction(context: &mut Box<dyn TestContext>, instruction: &Instruction) {
    println!("Instruction {:?}", instruction.data);

    println!(
        "{:<4} {:<44} {:<44} {:<10} {:<10} {:<10}",
        "IxId", "Key", "Owner", "Writable", "Signer", "Invoked"
    );

    for (i, account) in instruction.accounts.iter().enumerate() {
        let is_writable = account.is_writable;
        let is_signer = account.is_signer;

        let account_info = context
            .get_account(account.pubkey)
            .await
            .unwrap_or_default();

        msg!(
            "[{:<2}] {:<44} {:<44} {:<10} {:<10}",
            i,
            bs58::encode(account.pubkey).into_string(),
            bs58::encode(account_info.owner).into_string(),
            is_writable,
            is_signer,
        );
    }
}
