use crate::utils::process_transaction_assert_success;
use crate::utils::record::TestLogger;
use crate::utils::test_context::TestContext;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::EncodableKeypair;
use solana_sdk::system_instruction;
use solana_sdk::transaction::VersionedTransaction;
use spl_associated_token_account::get_associated_token_address;

pub async fn mint_tokens(
    context: &mut TestContext,
    user: &Keypair,
    logger: &mut TestLogger,
) -> Pubkey {
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
        &[user, &mint_keypair],
    )
    .unwrap();

    process_transaction_assert_success(context, mint_tx, logger)
        .await
        .unwrap();

    mint_keypair.encodable_pubkey()
}
