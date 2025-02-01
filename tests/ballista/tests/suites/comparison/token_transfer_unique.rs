use crate::utils::{
    ballista::definitions::builtin::token_program::transfer::{
        create_batch_token_transfer_def, execute_batch_token_transfer_tx,
    },
    process_transaction_assert_success,
    record::TestLogger,
    setup::user::create_user_with_balance,
    test_context::create_test_context,
    transaction::{
        mint::mint_tokens, token::generate_max_bump_token_account_user, utils::create_transaction,
    },
};
use ballista_common::types::logical_components::{Expression, Value};
use ballista_sdk::{find_task_definition_pda, generated::instructions::CreateTaskBuilder};
use solana_program_test::tokio;
use solana_sdk::{program_pack::Pack, pubkey::Pubkey, signer::EncodableKeypair};
use spl_associated_token_account::get_associated_token_address;

const BALLISTA_TOKEN_TRANSFER_COUNT: usize = 10;
const NATIVE_TOKEN_TRANSFER_COUNT: usize = 10;

#[tokio::test]
async fn ballista() {
    let mut logger = TestLogger::new("comparison", "ballista-token_transfer_unique").unwrap();

    let context = &mut create_test_context().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let user_pubkey = user.encodable_pubkey();
    let mint = mint_tokens(context, &user, &mut logger).await;

    let task_definition: Pubkey = find_task_definition_pda(user_pubkey, 0).0;
    let tx = create_transaction(
        context,
        vec![CreateTaskBuilder::new()
            .payer(user_pubkey)
            .task_definition(task_definition)
            .task(create_batch_token_transfer_def(
                1_000_000,
                Expression::Literal(Value::U8(BALLISTA_TOKEN_TRANSFER_COUNT as u8)),
            ))
            .task_id(0)
            .instruction()],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let mut dest_accounts = vec![];
    for _ in 0..BALLISTA_TOKEN_TRANSFER_COUNT {
        let (_user, user_pubkey, _) = generate_max_bump_token_account_user(&mint);
        dest_accounts.push(user_pubkey);
    }

    let tx = create_transaction(
        context,
        vec![execute_batch_token_transfer_tx(
            task_definition,
            &user_pubkey,
            &mint,
            &dest_accounts,
        )],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx.clone(), &mut logger)
        .await
        .unwrap();

    for (i, dest) in dest_accounts.iter().enumerate() {
        let account_info = context
            .get_account(get_associated_token_address(dest, &mint))
            .await
            .unwrap();

        let token_account = spl_token::state::Account::unpack(&account_info.data).unwrap();

        assert_eq!(
            1_000_000, token_account.amount,
            "Token account amount was {} expected {} at index {}",
            token_account.amount, 1_000_000, i
        );
    }
}

#[tokio::test]
async fn native() {
    let mut logger = TestLogger::new("comparison", "native-token_transfer_unique").unwrap();

    let context = &mut create_test_context().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();
    let mint = mint_tokens(context, &user, &mut logger).await;
    let user_ata = get_associated_token_address(&user.encodable_pubkey(), &mint);

    let mut ixs = vec![];
    let mut dest_accounts = vec![];

    for _ in 0..NATIVE_TOKEN_TRANSFER_COUNT {
        let (dest_user, dest_pubkey, _) = generate_max_bump_token_account_user(&mint);
        let dest_ata = get_associated_token_address(&dest_pubkey, &mint);

        dest_accounts.push(dest_user);

        ixs.push(
            spl_associated_token_account::instruction::create_associated_token_account(
                &user.encodable_pubkey(),
                &dest_pubkey,
                &mint,
                &spl_token::ID,
            ),
        );
        ixs.push(
            spl_token::instruction::transfer(
                &spl_token::ID,
                &user_ata,
                &dest_ata,
                &user.encodable_pubkey(),
                &[],
                1_000_000,
            )
            .unwrap(),
        );
    }

    let tx = create_transaction(context, ixs, &[&user]).await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    for (i, dest) in dest_accounts.iter().enumerate() {
        let account_info = context
            .get_account(get_associated_token_address(
                &dest.encodable_pubkey(),
                &mint,
            ))
            .await
            .unwrap();

        let token_account = spl_token::state::Account::unpack(&account_info.data).unwrap();

        assert_eq!(
            1_000_000, token_account.amount,
            "Token account amount was {} expected {} at index {}",
            token_account.amount, 1_000_000, i
        );
    }
}
