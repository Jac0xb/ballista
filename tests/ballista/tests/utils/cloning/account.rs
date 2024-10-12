use std::{fs::File, io::Write, path::PathBuf};

use crate::utils::test_context::TestContext;
use anchor_lang::prelude::UpgradeableLoaderState;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program_test::read_file;
use solana_sdk::{
    account::{AccountSharedData, WritableAccount},
    account_utils::StateMut,
    bpf_loader, bpf_loader_deprecated, bpf_loader_upgradeable,
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};

pub async fn set_accounts_from_rpc(
    context: &mut TestContext,
    connection: &RpcClient,
    account_pubkeys: &[Pubkey],
) {
    // chunk into calls of 100
    let chunks = account_pubkeys.chunks(100);
    for chunk in chunks {
        let accounts = connection
            .get_multiple_accounts(chunk)
            .await
            .unwrap()
            .clone();

        for (i, account) in accounts.iter().enumerate() {
            if account.is_none() {
                continue;
            }

            let account = account.clone().unwrap();
            let pubkey: &Pubkey = chunk.get(i).unwrap();

            let local_account = context.get_account(*pubkey).await;
            if local_account.is_some() {
                println!("account {:?} already exists", pubkey);
                continue;
            }

            let mut shared_account =
                AccountSharedData::new(account.lamports, account.data.len(), &account.owner);

            match account.owner {
                id if id == bpf_loader_upgradeable::id() => {
                    println!("Found program account {:?} owner {:?}", pubkey, id);
                    set_program_account(context, connection, *pubkey).await;
                }
                _ => {
                    println!("Found account {:?} owner {:?}", pubkey, account.owner);
                    shared_account.set_data_from_slice(account.data.as_slice());
                    context.program_context.set_account(pubkey, &shared_account);
                }
            }
        }
    }
}

pub async fn set_account_from_rpc(
    context: &mut TestContext,
    connection: &RpcClient,
    account_pubkey: &Pubkey,
) {
    let account = connection.get_account(account_pubkey).await;
    match account {
        Ok(account) => {
            let mut shared_account =
                AccountSharedData::new(account.lamports, account.data.len(), &account.owner);
            shared_account.set_data_from_slice(account.data.as_slice());

            context
                .program_context
                .set_account(account_pubkey, &shared_account);
        }
        Err(e) => {
            println!("Error getting account {:?}: {:?}", account_pubkey, e);
        }
    };
}

pub async fn set_account_from_refs(
    context: &mut TestContext,
    account_pubkey: &Pubkey,
    data: &[u8],
    owner: &Pubkey,
) {
    let lamports = context
        .get_minimum_balance_for_rent_exemption(data.len())
        .await;

    let mut shared_account = AccountSharedData::new(lamports, data.len(), owner);
    shared_account.set_data_from_slice(data);

    context
        .program_context
        .set_account(account_pubkey, &shared_account);
}

async fn set_program_account(
    context: &mut TestContext,
    rpc_client: &RpcClient,
    account_pubkey: Pubkey,
) {
    let dir = PathBuf::from(std::env::var("BPF_OUT_DIR").unwrap());
    let file_name = format!("{}.so", account_pubkey);
    let full_path = dir.join(PathBuf::from(file_name));

    if full_path.exists() {
    } else {
        save_program_data(rpc_client, account_pubkey).await.unwrap();
    }

    let data = read_file(full_path);

    let lamports = context
        .get_minimum_balance_for_rent_exemption(data.len())
        .await;

    let mut shared_account = AccountSharedData::new(lamports, data.len(), &bpf_loader::id());
    shared_account.set_data_from_slice(data.as_slice());
    shared_account.set_executable(true);

    context
        .program_context
        .set_account(&account_pubkey, &shared_account);
}

async fn save_program_data(rpc_client: &RpcClient, account_pubkey: Pubkey) -> Result<(), String> {
    let dir = PathBuf::from(std::env::var("BPF_OUT_DIR").unwrap());
    let file_name = format!("{}.so", account_pubkey);
    let full_path = dir.join(PathBuf::from(file_name));

    if full_path.exists() {
        return Ok(());
    } else {
        println!(
            "Program {:?} not found in {}",
            account_pubkey,
            full_path.to_str().unwrap()
        );
    }

    let account = rpc_client
        .get_account_with_commitment(&account_pubkey, CommitmentConfig::processed())
        .await
        .unwrap()
        .value;

    if let Some(account) = account {
        if account.owner == bpf_loader::id() || account.owner == bpf_loader_deprecated::id() {
            let mut f = File::create_new(full_path).unwrap();
            f.write_all(account.data.as_slice()).unwrap();
            Ok(())
        } else if account.owner == bpf_loader_upgradeable::id() {
            if let Ok(UpgradeableLoaderState::Program {
                programdata_address,
            }) = account.state()
            {
                if let Some(programdata_account) = rpc_client
                    .get_account_with_commitment(
                        &programdata_address,
                        CommitmentConfig::processed(),
                    )
                    .await
                    .unwrap()
                    .value
                {
                    if let Ok(UpgradeableLoaderState::ProgramData { .. }) =
                        programdata_account.state()
                    {
                        let offset = UpgradeableLoaderState::size_of_programdata_metadata();
                        let program_data = &programdata_account.data[offset..];

                        let mut f = File::create_new(full_path).unwrap();
                        f.write_all(program_data).unwrap();

                        Ok(())
                    } else {
                        Err(format!("Program {account_pubkey} has been closed"))
                    }
                } else {
                    Err(format!("Program {account_pubkey} has been closed"))
                }
            } else if let Ok(UpgradeableLoaderState::Buffer { .. }) = account.state() {
                let offset = UpgradeableLoaderState::size_of_buffer_metadata();
                let program_data = &account.data[offset..];

                let mut f = File::create_new(full_path).unwrap();
                f.write_all(program_data).unwrap();

                Ok(())
            } else {
                Err(format!(
                    "{account_pubkey} is not an upgradeable loader buffer or program account"
                ))
            }
        } else {
            Err(format!("{account_pubkey} is not an SBF program"))
        }
    } else {
        Err(format!("Program {account_pubkey} not found"))
    }
}
