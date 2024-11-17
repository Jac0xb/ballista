use crate::utils::{
    cloning::copy_accounts_from_transaction,
    jupiter::types::{SwapRequest, SwapResponse},
    record::TestLogger,
    test_context::TestContext,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable, pubkey::Pubkey,
    transaction::VersionedTransaction,
};

use super::{quote::get_jupiter_quote, types::SwapArgs};

pub async fn clone_swap(
    context: &mut TestContext,
    user: &Pubkey,
    client: &reqwest::Client,
    rpc_client: &RpcClient,
    args: SwapArgs,
) -> Result<VersionedTransaction, Box<dyn std::error::Error>> {
    let quote_response = get_jupiter_quote(args).await?;

    let response = client
        .post("https://quote-api.jup.ag/v6/swap")
        .header("Content-Type", "application/json")
        .json(&SwapRequest {
            quoteResponse: quote_response,
            userPublicKey: user.to_string(),
            wrapAndUnwrapSol: true,
        })
        .send()
        .await
        .unwrap();

    let response: SwapResponse = response.json().await.unwrap();

    let tx_base64_decoded = base64::decode(&response.swapTransaction).unwrap();
    let mut tx: VersionedTransaction = bincode::deserialize(&tx_base64_decoded).unwrap();

    // Copy all the accounts from the RPC to the test context
    copy_accounts_from_transaction(context, &tx, rpc_client).await;

    tx.message
        .set_recent_blockhash(context.get_blockhash().await);

    Ok(tx)
}

pub async fn print_transaction_info(
    context: &mut TestContext,
    tx: &VersionedTransaction,
    logger: &mut TestLogger,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut lookup_table_vecs: Vec<Vec<u8>> = vec![];
    if let Some(address_table_lookups) = tx.message.address_table_lookups() {
        for lookup in address_table_lookups {
            let lookup_table_account = context.get_account(lookup.account_key).await.unwrap().data;

            lookup_table_vecs.push(lookup_table_account.clone());
        }
    }

    let mut lookup_table_accounts: Vec<(Pubkey, AddressLookupTable<'_>)> = vec![];
    if let Some(address_table_lookups) = tx.message.address_table_lookups() {
        for lookup in address_table_lookups {
            let lookup_table =
                AddressLookupTable::deserialize(lookup_table_vecs.last().unwrap()).unwrap();

            lookup_table_accounts.push((lookup.account_key, lookup_table));
        }
    }

    // Build the complete list of account keys
    let mut all_account_keys = tx.message.static_account_keys().to_vec();

    for (_lookup_key, lookup_table) in &lookup_table_accounts {
        all_account_keys.extend(lookup_table.addresses.to_vec());
    }

    // For each instruction
    for (i, instruction) in tx.message.instructions().iter().enumerate() {
        logger.write(&format!("Instruction {}: {:?}", i, instruction));

        logger.write(&format!(
            "Program {:?}",
            tx.message.static_account_keys()[instruction.program_id_index as usize]
        ));

        // Collect account indices used in this instruction
        // let mut account_indices = HashSet::new();
        // for &account_index in &instruction.accounts {
        //     account_indices.insert(account_index);
        // }

        // Prepare the table header
        logger.write(&format!(
            "{:<4} {:<4}  {:<44} {:<44} {:<10} {:<10} {:<10}",
            "IxId", "TxId", "Key", "Owner", "Writable", "Signer", "Invoked"
        ));

        for (i, account_index) in instruction.accounts.iter().enumerate() {
            let account_index = *account_index as usize;

            let account_key = all_account_keys.get(account_index).unwrap();
            let account_data = context.get_account(*account_key).await.unwrap_or_default();
            let owner = account_data.owner;

            let is_writable = tx.message.is_maybe_writable(account_index);
            let is_signer = tx.message.is_signer(account_index);
            let is_invoked = tx.message.is_invoked(account_index);

            logger.write(&format!(
                "[{:<2} | {:<2} ] {:<44} {:<44} {:<10} {:<10} {:<10}",
                i,
                account_index,
                account_key.to_string(),
                owner.to_string(),
                is_writable,
                is_signer,
                is_invoked
            ));
        }

        logger.write(""); // Add an empty line between instructions for readability
    }

    Ok(())
}

// let static_accounts_minus_user = static_account_keys
//     .iter()
//     .enumerate()
//     .filter(|(_, key)| !key.eq(&user))
//     .map(|(_, key)| *key)
//     .collect::<Vec<Pubkey>>();

// set_accounts_from_rpc(context, &rpc_client, &static_accounts_minus_user).await;

// for (i, key) in static_account_keys.iter().enumerate() {
//     println!(
//         "[{}] account {:?}               is signer {:?}   is writable {:?}   is invoked {:?}",
//         i,
//         key,
//         tx.message.is_signer(i),
//         tx.message.is_maybe_writable(i),
//         tx.message.is_invoked(i)
//     );
// }

// (0..static_account_keys.len()).for_each(|i| {
//     // see which accounts are invoked
//     if tx.message.is_invoked(i) {
//         let account_key = static_account_keys[i];
//         println!("account {:?} [{}] is invoked", account_key, i);
//     }
// });

// // Findings: IF WE FETCH ALL THE ACCOUNTS AFTER SIGNING THE TRANSACTION, THE BLOCKHASH BECOMES INVALI
// let signed_tx = VersionedTransaction::try_new(tx.message.clone(), &[user]).unwrap();

// // log info about instruction five
// let instruction_five = signed_tx
//     .clone()
//     .message
//     .instructions()
//     .get(5)
//     .unwrap()
//     .clone();
// println!("instruction five {:?}", instruction_five);

// for index in local_lut.readonly_indexes {
//     let account_key = lookup_data.addresses.to_vec().as_slice()[index as usize];
//     let account_data = context.get_account(account_key).await;

//     if let Some(account) = account_data {
//         println!(
//             "account {:?} [{}] owner {:?}",
//             account_key, index, account.owner
//         );
//     }
// }

// for index in local_lut.writable_indexes {
//     let account_key = lookup_data.addresses.to_vec().as_slice()[index as usize];
//     let account_data = context.get_account(account_key).await;

//     if let Some(account) = account_data {
//         println!(
//             "account {:?} [{}] owner {:?}",
//             account_key, index, account.owner
//         );
//     }
// }

// async fn ass() {
//     let url =
//         "https://quote-api.jup.ag/v6/quote?inputMint=So11111111111111111111111111111111111111112\
//     &outputMint=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v\
//     &amount=100000000\
//     &slippageBps=500&maxAccounts=15";

//     let response = reqwest::get(url).await.unwrap();

//     let quote_response: QuoteResponse = response.json().await.unwrap();
//     println!("{:#?}", quote_response);

//     let response = client
//         .post("https://quote-api.jup.ag/v6/swap")
//         .header("Content-Type", "application/json")
//         .json(&SwapRequest {
//             quoteResponse: quote_response,
//             userPublicKey: user.encodable_pubkey().to_string(),
//             wrapAndUnwrapSol: true,
//         })
//         .send()
//         .await
//         .unwrap();

//     let response: SwapResponse = response.json().await.unwrap();
//     println!("{:#?}", response);

//     let tx_base64_decoded = base64::decode(&response.swapTransaction).unwrap();
//     let mut tx: VersionedTransaction = bincode::deserialize(&tx_base64_decoded).unwrap();

//     let rpc_client = RpcClient::new(

//     );

//     let static_account_keys = tx.message.static_account_keys().clone();

//     let static_accounts_minus_user = static_account_keys
//         .iter()
//         .enumerate()
//         .filter(|(_, key)| !key.eq(&&user.encodable_pubkey()))
//         .map(|(_, key)| *key)
//         .collect::<Vec<Pubkey>>();

//     set_accounts_from_rpc(context, &rpc_client, &static_accounts_minus_user).await;

//     for (i, key) in static_account_keys.iter().enumerate() {
//         println!(
//             "[{}] account {:?}               is signer {:?}   is writable {:?}   is invoked {:?}",
//             i,
//             key,
//             tx.message.is_signer(i),
//             tx.message.is_maybe_writable(i),
//             tx.message.is_invoked(i)
//         );
//     }

//     let lut = tx
//         .message
//         .address_table_lookups()
//         .map_or(vec![], |lut| lut.to_vec());

//     for i in 0..lut.len() {
//         let local_lut = lut[i].clone();

//         let lookup_account = rpc_client
//             .get_account(&local_lut.account_key)
//             .await
//             .unwrap();

//         let mut lookup_data = AddressLookupTable::deserialize(&lookup_account.data).unwrap();
//         lookup_data.meta.last_extended_slot = 0;
//         lookup_data.meta.last_extended_slot_start_index = 0;

//         let serialized_lookup_data: Vec<u8> = lookup_data.clone().serialize_for_tests().unwrap();
//         set_account_from_refs(
//             context,
//             &lut[i].account_key,
//             &serialized_lookup_data,
//             &lookup_account.owner,
//         )
//         .await;

//         let mut filtered_addresses = vec![];
//         for index in local_lut.clone().readonly_indexes {
//             filtered_addresses.push(lookup_data.addresses.to_vec().as_slice()[index as usize]);
//         }

//         for index in local_lut.clone().writable_indexes {
//             filtered_addresses.push(lookup_data.addresses.to_vec().as_slice()[index as usize]);
//         }

//         set_accounts_from_rpc(context, &rpc_client, &filtered_addresses).await;

//         for index in local_lut.readonly_indexes {
//             let account_key = lookup_data.addresses.to_vec().as_slice()[index as usize];
//             let account_data = context.get_account(account_key).await;

//             if let Some(account) = account_data {
//                 println!(
//                     "account {:?} [{}] owner {:?}",
//                     account_key, index, account.owner
//                 );
//             }
//         }

//         for index in local_lut.writable_indexes {
//             let account_key = lookup_data.addresses.to_vec().as_slice()[index as usize];
//             let account_data = context.get_account(account_key).await;

//             if let Some(account) = account_data {
//                 println!(
//                     "account {:?} [{}] owner {:?}",
//                     account_key, index, account.owner
//                 );
//             }
//         }
//     }

//     (0..static_account_keys.len()).for_each(|i| {
//         // see which accounts are invoked
//         if tx.message.is_invoked(i) {
//             let account_key = static_account_keys[i];
//             println!("account {:?} [{}] is invoked", account_key, i);
//         }
//     });

//     println!("sending tx {:?}", tx);

//     let user_balance = context.get_account(user.encodable_pubkey()).await.unwrap();
//     assert!(user_balance.lamports > 0);

//     // Retrieve the blockhash first
//     let blockhash = context.get_blockhash().await;

//     // Then set the recent blockhash
//     tx.message.set_recent_blockhash(blockhash);

//     // Findings: IF WE FETCH ALL THE ACCOUNTS AFTER SIGNING THE TRANSACTION, THE BLOCKHASH BECOMES INVALI
//     let signed_tx = VersionedTransaction::try_new(tx.message.clone(), &[&user]).unwrap();

//     // log info about instruction five
//     let instruction_five = signed_tx
//         .clone()
//         .message
//         .instructions()
//         .get(5)
//         .unwrap()
//         .clone();
//     println!("instruction five {:?}", instruction_five);
// }
