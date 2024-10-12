use solana_sdk::signature::Keypair;
use solana_sdk::signer::EncodableKeypair;

use crate::utils::test_context::{TestContext, DEFAULT_LAMPORTS_FUND_AMOUNT};
use crate::utils::Result;

pub async fn create_user(ctx: &mut TestContext) -> Result<Keypair> {
    let user = Keypair::new();
    let _ = ctx
        .fund_account(user.encodable_pubkey(), DEFAULT_LAMPORTS_FUND_AMOUNT)
        .await;

    Ok(user)
}

pub async fn create_user_with_balance(ctx: &mut TestContext, balance: u64) -> Result<Keypair> {
    let user = Keypair::new();
    let _ = ctx.fund_account(user.encodable_pubkey(), balance).await;

    Ok(user)
}
