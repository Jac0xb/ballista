use crate::utils::process_transaction_assert_success;
use crate::utils::task_definition::create_system_transfer_task_definition;
use crate::utils::{context::TestContext, create_user_with_balance};
use anchor_lang::prelude::AccountMeta;
use ballista_common::task::action::schema_instruction::TaskAccount;
use ballista_common::task::action::set_cache::SetCacheType;
use ballista_common::task::action::system_instruction::SystemInstruction;
use ballista_common::task::command::Command;
use ballista_common::task_definition::{Schema, TaskDefinition};
use ballista_sdk::generated::instructions::{
    CreateSchema, CreateSchemaInstructionArgs, ExecuteTask, ExecuteTaskInstructionArgs,
};
use ballista_sdk::{find_task_definition_pda, BALLISTA_ID};
use bincode::serialize;
use solana_program_test::tokio;
use solana_sdk::address_lookup_table::instruction::{
    create_lookup_table_signed, extend_lookup_table,
};
use solana_sdk::address_lookup_table::state::AddressLookupTable;
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::EncodableKeypair;
use solana_sdk::system_program;
use solana_sdk::transaction::VersionedTransaction;

async fn create_lut(
    ctx: &mut TestContext,
    authority: &Keypair,
    add_accounts: Vec<Pubkey>,
) -> Pubkey {
    let recent_slot = ctx.client().get_root_slot().await.unwrap() - 1;

    let (ix, lookup_table) = create_lookup_table_signed(
        authority.encodable_pubkey(),
        authority.encodable_pubkey(),
        recent_slot,
    );
    let add_ix = extend_lookup_table(
        lookup_table,
        authority.encodable_pubkey(),
        Some(authority.encodable_pubkey()),
        add_accounts,
    );

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &authority.encodable_pubkey(),
                &[ix, add_ix],
                &[],
                ctx.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&authority],
    )
    .unwrap();

    process_transaction_assert_success(ctx, tx).await.unwrap();

    lookup_table
}

#[tokio::test]
async fn simple() {
    let batch_size = 29;

    let compute_ix = ComputeBudgetInstruction::set_compute_unit_limit(800_000);

    println!("compute_ix: {:?}", compute_ix);

    let context = &mut TestContext::new().await.unwrap();
    let user = create_user_with_balance(context, 10e9 as u64)
        .await
        .unwrap();

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
                            schema_arg: Schema {
                                // instructions: vec![create_system_transfer_task_definition()],
                                instructions: vec![],
                                tasks: vec![create_system_transfer_task_definition(batch_size)],
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

    let mut remaining_accounts = vec![
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(user.encodable_pubkey(), true),
    ];

    let mut lookup_tables: Vec<AddressLookupTableAccount> = vec![];

    // split up lookup tables by 30
    for i in (0..remaining_accounts.len()).step_by(30) {
        let end = (i + 30).min(remaining_accounts.len());
        let table_accounts: Vec<Pubkey> = remaining_accounts
            .clone()
            .into_iter()
            .skip(i)
            .take(end - i)
            .map(|a| a.pubkey)
            .collect();
        let lookup_table = create_lut(context, &user, table_accounts).await;

        let data = context
            .client()
            .get_account(lookup_table)
            .await
            .unwrap()
            .unwrap()
            .data;
        let table = AddressLookupTable::deserialize(&data).unwrap();

        lookup_tables.push(AddressLookupTableAccount {
            key: lookup_table,
            addresses: table.addresses.to_vec(),
        });
    }

    // add N keypairs to the remaining accounts
    for _ in 0..batch_size {
        remaining_accounts.push(AccountMeta::new(Keypair::new().encodable_pubkey(), false));
    }

    context.warp_to_slot(5).unwrap();

    let schema: Pubkey = find_task_definition_pda(user.encodable_pubkey(), 0).0;

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &user.encodable_pubkey(),
                &[
                    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
                        &ExecuteTask {
                            schema,
                        },
                        ExecuteTaskInstructionArgs {
                            input_values: vec![],
                        },
                   &remaining_accounts,
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

    println!("tx size: {}", serialize(&tx).unwrap().len());
    assert!(
        serialize(&tx).unwrap().len() < 1232,
        "tx too large {}",
        serialize(&tx).unwrap().len()
    );

    process_transaction_assert_success(context, tx)
        .await
        .unwrap();

    context.warp_to_slot(10).unwrap();

    remaining_accounts = remaining_accounts.into_iter().skip(2).collect();
    for i in 0..batch_size as usize {
        let meta = remaining_accounts
            .get(i)
            .unwrap_or_else(|| panic!("account {} not found", i));
        let account = context
            .client()
            .get_account(meta.pubkey)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("remaining account {} not found", i));

        println!("account {:?}", account);

        assert_eq!(
            account.lamports, 1_000_000,
            "account {} has wrong balance {}",
            i, account.lamports
        );
    }

    // panic!("done");
}
