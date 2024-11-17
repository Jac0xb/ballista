use std::str::FromStr;

use crate::utils::{
    ballista::definitions::defined::jup_program::{
        create_jupiter_swap_and_transfer_task_definition, execute_jupiter_swap_and_transfer,
        JupiterSwapInstructionAccountParams,
    },
    cloning::{copy_accounts_from_transaction, set_account_from_refs},
    jupiter::{
        instruction_cloning::{clone_swap_instructions, SwapInstructions},
        transaction_cloning::print_transaction_info,
        types::SwapArgs,
    },
    process_transaction_assert_success,
    record::TestLogger,
    setup::user::create_user_with_balance,
    test_context::TestContext,
    transaction::utils::create_transaction,
};
use anchor_lang::prelude::AccountMeta;
use anchor_spl::token;
use ballista_common::{
    logical_components::{AccountInfoType, Expression, Value, ValueType},
    schema::{AccountGroupDefinition, ExecutionSettings, TaskDefinition},
    task::{
        action::{
            raw_instruction::RawInstruction,
            token_program_instruction::{TokenProgramInstruction, TokenProgramVersion},
        },
        shared::TaskAccount,
        task_action::TaskAction,
    },
};
use ballista_sdk::{
    find_task_definition_pda,
    generated::instructions::{
        CreateTask, CreateTaskInstructionArgs, ExecuteTask, ExecuteTaskInstructionArgs,
    },
    BALLISTA_ID,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program_test::tokio;
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    message::{v0, VersionedMessage},
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Keypair,
    signer::EncodableKeypair,
    system_program,
    transaction::VersionedTransaction,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::state::{Account, AccountState};

#[tokio::test]
async fn test() {
    let mut logger = TestLogger::new("jup_program", "test").unwrap();

    let context = &mut TestContext::new().await.unwrap();
    let client = reqwest::Client::new();
    let rpc_client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

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
                    task_id: 0,
                    task: create_jupiter_swap_and_transfer_task_definition(),
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx, &mut logger)
        .await
        .unwrap();

    let mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();

    let SwapInstructions {
        setup_instructions,
        swap_instruction,
    } = clone_swap_instructions(
        context,
        &user.encodable_pubkey(),
        &client,
        &rpc_client,
        SwapArgs {
            amount: 1_000_000_000,
            input_mint: "So11111111111111111111111111111111111111112",
            output_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            max_accounts: 20,
            slippage_bps: 1_000,
        },
    )
    .await
    .unwrap();

    let mut instructions = setup_instructions.clone();

    let token_account = get_associated_token_address(&user.encodable_pubkey(), &mint);

    let destination = Keypair::new().encodable_pubkey();
    let destination_token_account = get_associated_token_address(&destination, &mint);

    let mut remaining_accounts = vec![];
    for account in &swap_instruction.accounts {
        remaining_accounts.push(account.clone());
    }

    let mut account_data = vec![0; Account::LEN];
    Account::pack(
        Account {
            amount: 0,
            owner: destination,
            delegated_amount: 0,
            mint,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            close_authority: COption::None,
        },
        &mut account_data,
    )
    .unwrap();

    set_account_from_refs(
        context,
        &destination_token_account,
        &account_data,
        &token::ID,
    )
    .await;

    instructions.push(
        execute_jupiter_swap_and_transfer(&JupiterSwapInstructionAccountParams {
            payer: user.encodable_pubkey(),
            from_token_account: token_account,
            to_token_account: destination_token_account,
            jupiter_program: swap_instruction.program_id,
            swap_ix_data: swap_instruction.data,
            swap_jup_accounts: swap_instruction.accounts,
        }),
        // ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        //     &ExecuteTask {
        //         task: task_definition,
        //         payer: user.encodable_pubkey(),
        //     },
        //     ExecuteTaskInstructionArgs {
        //         task_values: vec![
        //             Value::Bytes(swap_instruction.data),
        //             Value::U8(3),
        //             Value::U8(swap_instruction.accounts.len() as u8),
        //         ],
        //     },
        //     &remaining_accounts,
        // ),
    );

    // let signed_tx = VersionedTransaction::try_new(tx.message.clone(), &[&user]).unwrap();

    let mut tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &instructions,
                &[] as &[AddressLookupTableAccount],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&user],
    )
    .unwrap();

    // Copy all the accounts from the RPC to the test context
    copy_accounts_from_transaction(context, &tx, &rpc_client).await;

    tx.message
        .set_recent_blockhash(context.get_blockhash().await);

    let signed_tx = VersionedTransaction::try_new(tx.message.clone(), &[&user]).unwrap();

    process_transaction_assert_success(context, signed_tx, &mut logger)
        .await
        .unwrap();

    let token_account_data = context.get_account(token_account).await.unwrap();

    let token_account_deserialized = Account::unpack(token_account_data.data.as_slice()).unwrap();

    // print_transaction_info(context, &tx).await.unwrap();

    assert_eq!(token_account_deserialized.amount, 0);
}
