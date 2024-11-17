use crate::utils::ballista::definitions::builtin::token_program::transfer::{
    create_batch_token_transfer_def, execute_batch_token_transfer_tx,
};
use crate::utils::process_transaction_assert_success;
use crate::utils::record::TestLogger;
use crate::utils::setup::user::create_user_with_balance;
use crate::utils::test_context::TestContext;
use crate::utils::transaction::mint::mint_tokens;
use crate::utils::transaction::token::generate_max_bump_token_account_user;
use crate::utils::transaction::utils::create_transaction;
use ballista_common::logical_components::{Expression, Value};
use ballista_sdk::find_task_definition_pda;
use ballista_sdk::generated::instructions::CreateTaskBuilder;
use solana_program_test::tokio;
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::EncodableKeypair;
use spl_associated_token_account::get_associated_token_address;

#[tokio::test]
async fn batch_token_transfer() {
    let mut logger = TestLogger::new("token_program", "batch_token_transfer").unwrap();

    let batch_size = 10;

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

    let user_pubkey = user.encodable_pubkey();
    let mint = mint_tokens(context, &user, &mut logger).await;
    // let user_ata: Pubkey = get_associated_token_address(&user_pubkey, &mint);
    let task_definition: Pubkey = find_task_definition_pda(user_pubkey, 0).0;
    let tx = create_transaction(
        context,
        vec![CreateTaskBuilder::new()
            .payer(user_pubkey)
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

    let mut dest_accounts = vec![];
    for _ in 0..batch_size {
        let (user, _) = generate_max_bump_token_account_user(&mint);
        dest_accounts.push(user);
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

    // for all accounts in tx
    for account in tx.message.static_account_keys().iter() {
        let account_info = context.client().get_account(*account).await.unwrap();

        logger.write(&format!("account: {:?}", account_info));
    }

    for (i, dest) in dest_accounts.iter().enumerate() {
        let account_info = context
            .client()
            .get_account(get_associated_token_address(dest, &mint))
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Account not found at index {}", i));

        let token_account = spl_token::state::Account::unpack(&account_info.data).unwrap();

        assert_eq!(
            1_000_000, token_account.amount,
            "Token account amount was {} expected {} at index {}",
            token_account.amount, 1_000_000, i
        );
    }
}
