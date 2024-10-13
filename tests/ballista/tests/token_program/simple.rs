use crate::utils::ballista::definitions::token_program::transfer::{
    create_batch_token_transfer_def, create_token_transfer,
    get_associated_token_address_and_bump_seed_internal,
};
use crate::utils::process_transaction_assert_success;
use crate::utils::record::TestLogger;
use crate::utils::setup::user::create_user_with_balance;
use crate::utils::test_context::TestContext;
use crate::utils::transaction::utils::create_transaction;
use anchor_lang::prelude::AccountMeta;
use ballista_common::logical_components::{Expression, Value};
use ballista_sdk::find_task_definition_pda;
use ballista_sdk::generated::instructions::{
    CreateTaskBuilder, ExecuteTask, ExecuteTaskInstructionArgs,
};
use solana_program_test::tokio;
use solana_sdk::instruction::Instruction;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::EncodableKeypair;
use solana_sdk::transaction::VersionedTransaction;
use solana_sdk::{system_instruction, system_program};
use spl_associated_token_account::get_associated_token_address;

#[tokio::test]
async fn batch_token_transfer() {
    let mut logger = TestLogger::new("token_program", "batch_token_transfer").unwrap();

    let batch_size = 12;

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let mint_keypair = Keypair::new();

    let user_ata =
        get_associated_token_address(&user.encodable_pubkey(), &mint_keypair.encodable_pubkey());

    let mint_tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    system_instruction::create_account(
                        &user.encodable_pubkey(),
                        &mint_keypair.encodable_pubkey(),
                        context
                            .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)
                            .await,
                        spl_token::state::Mint::LEN as u64,
                        &spl_token::ID,
                    ),
                    spl_token::instruction::initialize_mint2(
                        &spl_token::ID,
                        &mint_keypair.encodable_pubkey(),
                        &user.encodable_pubkey(),
                        None,
                        6,
                    )
                    .unwrap(),
                    spl_associated_token_account::instruction::create_associated_token_account(
                        &user.encodable_pubkey(),
                        &user.encodable_pubkey(),
                        &mint_keypair.encodable_pubkey(),
                        &spl_token::ID,
                    ),
                    spl_token::instruction::mint_to(
                        &spl_token::ID,
                        &mint_keypair.encodable_pubkey(),
                        &user_ata,
                        &user.encodable_pubkey(),
                        &[],
                        100_000_000,
                    )
                    .unwrap(),
                ],
                &[],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&user, &mint_keypair],
    )
    .unwrap();

    process_transaction_assert_success(context, mint_tx, &mut logger)
        .await
        .unwrap();

    let user_ata_info = context
        .client()
        .get_account(user_ata)
        .await
        .unwrap()
        .unwrap();

    let task_definition: Pubkey = find_task_definition_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![CreateTaskBuilder::new()
            .payer(user.encodable_pubkey())
            .task_definition(task_definition)
            .task(create_batch_token_transfer_def(
                1_000_000,
                Expression::Literal(Value::U8(batch_size)),
            ))
            .task_id(0)
            .instruction()],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let mut dest_accounts_batch_one = vec![];
    for _ in 0..batch_size {
        loop {
            // dest_accounts_batch_one.push();
            let dest = Keypair::new().encodable_pubkey();
            let (_, bump) = get_associated_token_address_and_bump_seed_internal(
                &dest,
                &mint_keypair.encodable_pubkey(),
                &spl_associated_token_account::ID,
                &spl_token::ID,
            );

            if bump == 255 {
                dest_accounts_batch_one.push(dest);
                break;
            }
        }
    }

    // TODO: FIX
    let tx = create_transaction(
        context,
        vec![create_token_transfer(
            task_definition,
            &user,
            user_ata,
            &mint_keypair.encodable_pubkey(),
            &dest_accounts_batch_one,
        )],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    // for account in dest_accounts_batch_one {
    //     let account_info = context
    //         .client()
    //         .get_account(get_associated_token_address(
    //             &account,
    //             &mint_keypair.encodable_pubkey(),
    //         ))
    //         .await
    //         .unwrap()
    //         .unwrap();

    //     let token_account = spl_token::state::Account::unpack(&account_info.data).unwrap();

    //     assert_eq!(
    //         1_000_000, token_account.amount,
    //         "Token account amount was {} expected {}",
    //         token_account.amount, 1_000_000
    //     );
    // }

    // // panic!("done");
}
