use crate::utils::{test_context::TestContext, Result};
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::EncodableKeypair,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;

pub async fn set_authority_token_account(
    ctx: &mut Box<dyn TestContext>,
    token_account: &Pubkey,
    authority: &Keypair,
    new_authority: Option<Pubkey>,
    authority_type: spl_token::instruction::AuthorityType,
) -> Result<Transaction> {
    let ix = spl_token::instruction::set_authority(
        &spl_token::id(),
        token_account,
        new_authority.as_ref(),
        authority_type,
        &authority.encodable_pubkey(),
        &[],
    )
    .unwrap();

    let mut tx = Transaction::new_with_payer(&[ix], Some(&authority.encodable_pubkey()));

    let signers: &[Keypair; 1] = &[authority.insecure_clone()];

    tx.try_partial_sign(
        &signers.iter().collect::<Vec<_>>(),
        ctx.get_blockhash().await,
    )
    .unwrap();

    Ok(tx)
}

pub async fn create_and_transfer_token_account_ix(
    ctx: &mut Box<dyn TestContext>,
    sender: &Pubkey,
    mint: &Pubkey,
    dest: &Pubkey,
    amount: u64,
) -> Result<Vec<Instruction>> {
    let src_token_account = get_associated_token_address(sender, mint);
    let dest_token_account = get_associated_token_address(dest, mint);

    let mut ixs = Vec::new();

    if let Some(account) = ctx.get_account(dest_token_account).await {
        if account.lamports == 0 {
            ixs.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    sender,
                    dest,
                    mint,
                    &spl_token::id(),
                ),
            )
        }
    } else {
        ixs.push(
            spl_associated_token_account::instruction::create_associated_token_account(
                sender,
                dest,
                mint,
                &spl_token::id(),
            ),
        )
    }

    ixs.push(
        spl_token::instruction::transfer(
            &spl_token::id(),
            &src_token_account,
            &dest_token_account,
            sender,
            &[],
            amount,
        )
        .unwrap(),
    );

    Ok(ixs)
}
