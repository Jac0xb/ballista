use crate::utils::test_context::TestContext;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable, pubkey::Pubkey,
    transaction::VersionedTransaction,
};

use super::{set_account_from_refs, set_accounts_from_rpc};

pub async fn copy_accounts_from_transaction(
    context: &mut Box<dyn TestContext>,
    tx: &VersionedTransaction,
    rpc_client: &RpcClient,
) {
    let static_account_keys = tx.message.static_account_keys();
    let fee_payer = static_account_keys[0];

    let static_accounts_minus_fee_payer = static_account_keys
        .iter()
        .enumerate()
        .filter(|(_, key)| !key.eq(&&fee_payer))
        .map(|(_, key)| *key)
        .collect::<Vec<Pubkey>>();

    set_accounts_from_rpc(context, rpc_client, &static_accounts_minus_fee_payer).await;

    let lut = tx
        .message
        .address_table_lookups()
        .map_or(vec![], |lut| lut.to_vec());

    for i in 0..lut.len() {
        let local_lut = lut[i].clone();

        let lookup_account = rpc_client
            .get_account(&local_lut.account_key)
            .await
            .unwrap();

        let mut lookup_data = AddressLookupTable::deserialize(&lookup_account.data).unwrap();
        lookup_data.meta.last_extended_slot = 0;
        lookup_data.meta.last_extended_slot_start_index = 0;

        let serialized_lookup_data: Vec<u8> = lookup_data.clone().serialize_for_tests().unwrap();
        set_account_from_refs(
            context,
            &lut[i].account_key,
            &serialized_lookup_data,
            &lookup_account.owner,
        )
        .await;

        let mut accounts_to_clone = vec![];
        for index in local_lut.clone().readonly_indexes {
            accounts_to_clone.push(lookup_data.addresses.to_vec().as_slice()[index as usize]);
        }

        for index in local_lut.clone().writable_indexes {
            accounts_to_clone.push(lookup_data.addresses.to_vec().as_slice()[index as usize]);
        }

        set_accounts_from_rpc(context, rpc_client, &accounts_to_clone).await;
    }
}
