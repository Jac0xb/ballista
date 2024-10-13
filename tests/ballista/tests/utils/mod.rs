pub mod ballista;
pub mod cloning;
pub mod error;
pub mod jupiter;
pub mod program_test;
pub mod record;
pub mod setup;
pub mod test_context;
pub mod transaction;

use bincode::serialize;
use record::TestLogger;
use solana_banks_interface::BanksTransactionResultWithMetadata;
use solana_program_test::{BanksClient, BanksClientError};
use solana_sdk::{
    instruction::InstructionError,
    signature::Keypair,
    transaction::{TransactionError, VersionedTransaction},
};
use std::result;

use self::{error::Error, test_context::TestContext};

pub type Result<T> = result::Result<T, Box<error::Error>>;
pub type BanksResult<T> = std::result::Result<T, BanksClientError>;

// Helper method to copy keypairs for testing, since they don't implement
// `Copy/Clone` themselves (for some good reasons).
pub fn clone_keypair(k: &Keypair) -> Keypair {
    Keypair::from_bytes(k.to_bytes().as_slice()).unwrap()
}

// pub async fn mint_to(
//     ctx: &mut TestContext,
//     mint: &Pubkey,
//     authority: &Keypair,
//     dest: &Pubkey,
//     amount: u64,
// ) -> Result<Transaction> {
//     let token_account = associated_token::get_associated_token_address(dest, mint);
//     let create_account_ix =
//         spl_associated_token_account::instruction::create_associated_token_account(
//             &authority.pubkey(),
//             dest,
//             mint,
//             &spl_token::id(),
//         );

//     let mint_to_ix = spl_token::instruction::mint_to(
//         &spl_token::id(),
//         mint,
//         &token_account,
//         &authority.pubkey(),
//         &[],
//         amount,
//     )
//     .unwrap();

//     let mut tx =
//         Transaction::new_with_payer(&[create_account_ix, mint_to_ix], Some(&authority.pubkey()));

//     let signers: &[Keypair; 1] = &[authority.insecure_clone()];

//     tx.try_partial_sign(
//         &signers.iter().collect::<Vec<_>>(),
//         ctx.client().get_latest_blockhash().await.unwrap(),
//     )
//     .unwrap();

//     Ok(tx)
// }

// pub async fn create_test_account(
//     ctx: &mut TestContext,
//     payer: &Keypair,
//     random: bool,
// ) -> Result<Keypair> {
//     let account_keypair = Keypair::new();
//     let program = TestProgram {};

//     let tx = program
//         .create_test_account(
//             payer.encodable_pubkey(),
//             account_keypair.encodable_pubkey(),
//             random,
//         )
//         .to_transaction_and_sign(
//             vec![payer],
//             payer.encodable_pubkey(),
//             ctx.get_blockhash().await,
//         )
//         .unwrap();

//     process_transaction_assert_success(ctx, tx).await.unwrap();

//     Ok(account_keypair)
// }

pub async fn process_transaction(
    client: &mut BanksClient,
    tx: &VersionedTransaction,
) -> Result<BanksTransactionResultWithMetadata> {
    let result: solana_banks_interface::BanksTransactionResultWithMetadata = client
        .process_transaction_with_metadata(tx.clone())
        .await
        .unwrap();

    Ok(result)
}

pub async fn process_transaction_assert_success(
    context: &mut TestContext,
    tx: VersionedTransaction,
    logger: &mut TestLogger,
) -> Result<BanksTransactionResultWithMetadata> {
    let tx_metadata = process_transaction(&mut context.client(), &tx).await;

    if let Err(err) = tx_metadata {
        panic!("Transaction failed to process: {:?}", err);
    }

    assert!(
        serialize(&tx).unwrap().len() < 1232,
        "tx too large {}",
        serialize(&tx).unwrap().len()
    );

    let tx_metadata = tx_metadata.unwrap();

    logger.write("");
    logger.write(&format!(
        "Transaction size: {}",
        serialize(&tx).unwrap().len()
    ));

    if let Some(logs) = tx_metadata.metadata.clone().map(|m| m.log_messages) {
        logger.write("Transaction Logs:");
        for log in logs {
            logger.write(&log);
        }
    }

    if tx_metadata.result.is_err() {
        return Err(Box::new(Error::TransactionFailed(format!(
            "Tx Result {:?}",
            tx_metadata
        ))));
    }

    logger
        .record_compute(tx_metadata.metadata.clone().unwrap().compute_units_consumed)
        .unwrap();

    Ok(tx_metadata)
}

pub async fn process_transaction_assert_failure(
    context: &mut TestContext,
    tx: VersionedTransaction,
    expected_tx_error: TransactionError,
    log_match_regex: Option<&[String]>,
    logger: &mut TestLogger,
) -> Result<BanksTransactionResultWithMetadata> {
    let tx_metadata = &mut process_transaction(&mut context.client(), &tx)
        .await
        .unwrap();

    if tx_metadata.metadata.is_none() {
        logger.write("No metadata found in transaction");
        logger.write(&format!("{:?}", tx_metadata.result));

        return Err(Box::new(Error::TransactionFailed(
            "No metadata found in transaction".to_string(),
        )));
    }

    let logs = tx_metadata.metadata.clone().unwrap().log_messages;
    logger.write("Transaction Logs:");
    for log in logs {
        logger.write(&log);
    }

    if tx_metadata.result.is_ok() {
        return Err(Box::new(Error::TransactionExpectedFailure(
            "Transaction was expected to fail".to_string(),
        )));
    }

    let actual_tx_error = tx_metadata.result.clone().unwrap_err();

    if actual_tx_error != expected_tx_error {
        match &actual_tx_error {
            TransactionError::InstructionError(ix_index, program_error) => {
                match &expected_tx_error {
                    TransactionError::InstructionError(
                        expected_ix_index,
                        expected_program_error,
                    ) => {
                        if ix_index != expected_ix_index || program_error != expected_program_error
                        {
                            return Err(Box::new(Error::TransactionExpectedFailure(format!(
                                "Expected error code: {:?}, got: {:?}",
                                expected_tx_error, &actual_tx_error
                            ))));
                        }
                    }
                    _ => {
                        return Err(Box::new(Error::TransactionExpectedFailure(format!(
                            "Expected error code: {:?}, got: {:?}",
                            expected_tx_error, actual_tx_error
                        ))));
                    }
                }

                return Err(Box::new(Error::TransactionExpectedFailure(format!(
                    "Expected error code: {:?}, got: {:?}",
                    expected_tx_error, program_error
                ))));
            }
            _ => {
                return Err(Box::new(Error::TransactionExpectedFailure(format!(
                    "Expected error code: {:?}, got: {:?}",
                    expected_tx_error, actual_tx_error
                ))));
            }
        }
    }

    if let Some(log_regex) = log_match_regex {
        let regexes = log_regex
            .iter()
            .map(|s| regex::Regex::new(s).unwrap())
            .collect::<Vec<regex::Regex>>();

        let logs = tx_metadata.metadata.clone().unwrap().log_messages;
        for log in &logs {
            logger.write(&format!("{:?}", log));
        }

        // find one log that matches each regex
        for regex in regexes {
            let mut found = false;
            for log in &logs {
                if regex.is_match(log) {
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(Box::new(Error::LogNotFound(format!(
                    "Log not found: {}",
                    regex
                ))));
            }
        }
    }

    Ok(tx_metadata.clone())
}

// pub fn to_transaction_error(ix_index: u8, program_error: LighthouseError) -> TransactionError {
//     TransactionError::InstructionError(
//         ix_index,
//         InstructionError::Custom(6000 + program_error as u32),
//     )
// }

pub fn to_transaction_error_u8(ix_index: u8, program_error: u32) -> TransactionError {
    TransactionError::InstructionError(ix_index, InstructionError::Custom(program_error))
}

// pub async fn set_account_from_rpc(
//     context: &mut TestContext,
//     connection: &RpcClient,
//     account_pubkey: &Pubkey,
// ) {
//     let account = connection.get_account(account_pubkey).await.unwrap();

//     let mut shared_account =
//         AccountSharedData::new(account.lamports, account.data.len(), &account.owner);
//     shared_account.set_data_from_slice(account.data.as_slice());

//     context
//         .program_context
//         .set_account(account_pubkey, &shared_account);
// }

// pub async fn set_account_from_refs(
//     context: &mut TestContext,
//     account_pubkey: &Pubkey,
//     data: &[u8],
//     owner: &Pubkey,
// ) {
//     let lamports = context
//         .get_minimum_balance_for_rent_exemption(data.len())
//         .await;

//     let mut shared_account = AccountSharedData::new(lamports, data.len(), owner);
//     shared_account.set_data_from_slice(data);

//     context
//         .program_context
//         .set_account(account_pubkey, &shared_account);
// }
