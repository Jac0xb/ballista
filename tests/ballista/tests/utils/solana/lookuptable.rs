use solana_sdk::{
    address_lookup_table::{
        instruction::{create_lookup_table_signed, extend_lookup_table},
        state::AddressLookupTable,
        AddressLookupTableAccount,
    },
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::Keypair,
    signer::EncodableKeypair,
    transaction::VersionedTransaction,
};

use crate::utils::{
    process_transaction_assert_success, record::TestLogger, test_context::TestContext,
};

pub async fn create_lut(
    ctx: &mut Box<dyn TestContext>,
    authority: &Keypair,
    add_accounts: &[Pubkey],
    logger: &mut TestLogger,
) -> AddressLookupTableAccount {
    let recent_slot = ctx.get_slot().await - 1;

    let (ix, lookup_table) = create_lookup_table_signed(
        authority.encodable_pubkey(),
        authority.encodable_pubkey(),
        recent_slot,
    );

    let create_tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &authority.encodable_pubkey(),
                &[ix],
                &[],
                ctx.get_blockhash().await,
            )
            .unwrap(),
        ),
        &[&authority],
    )
    .unwrap();

    process_transaction_assert_success(ctx, create_tx, logger)
        .await
        .unwrap();

    for i in (0..add_accounts.len()).step_by(20) {
        let end = std::cmp::min(i + 20, add_accounts.len());
        let batch_accounts = &add_accounts[i..end];

        let tx = VersionedTransaction::try_new(
            VersionedMessage::V0(
                v0::Message::try_compile(
                    &authority.encodable_pubkey(),
                    &[extend_lookup_table(
                        lookup_table,
                        authority.encodable_pubkey(),
                        Some(authority.encodable_pubkey()),
                        batch_accounts.to_vec(),
                    )],
                    &[],
                    ctx.get_blockhash().await,
                )
                .unwrap(),
            ),
            &[&authority],
        )
        .unwrap();

        process_transaction_assert_success(ctx, tx, logger)
            .await
            .unwrap();
    }

    let data = ctx.get_account(lookup_table).await.unwrap().data;
    let table = AddressLookupTable::deserialize(&data).unwrap();

    if table.addresses.len() != add_accounts.len() {
        panic!("lookup table addresses length mismatch");
    }

    let future_slot = ctx.get_slot().await + 1;

    // The bankclient doesn't respect the lookup table until the slot after it was created.
    ctx.warp_to_slot(future_slot).unwrap();

    AddressLookupTableAccount {
        key: lookup_table,
        addresses: table.addresses.to_vec(),
    }
}
