use solana_sdk::{
    program_pack::Pack, pubkey::Pubkey, rent::Rent, signature::Keypair, signer::Signer,
    system_instruction, transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token_2022::state::Mint;

use crate::utils::{test_context::TestContext, Result};

pub struct CreateMintParameters {
    pub token_program: Pubkey,
    pub mint_authority: Option<Option<Pubkey>>,
    pub freeze_authority: Option<Pubkey>,
    pub decimals: u8,
    pub mint_to: Option<(Pubkey, u64)>,
}

pub async fn create_mint(
    ctx: &mut Box<dyn TestContext>,
    payer: &Keypair,
    parameters: CreateMintParameters,
) -> Result<(Transaction, Keypair)> {
    let mint = Keypair::new();

    let mint_rent = Rent::default().minimum_balance(Mint::LEN);

    let mut ixs = Vec::new();

    let create_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        mint_rent,
        Mint::LEN as u64,
        &parameters.token_program,
    );
    let mint_ix = spl_token::instruction::initialize_mint2(
        &parameters.token_program,
        &mint.pubkey(),
        &payer.pubkey(),
        parameters.freeze_authority.as_ref(),
        parameters.decimals,
    )
    .unwrap();

    ixs.push(create_ix);
    ixs.push(mint_ix);

    if let Some((dest, amount)) = parameters.mint_to {
        let token_account = get_associated_token_address(&dest, &mint.pubkey());
        let create_account_ix =
            spl_associated_token_account::instruction::create_associated_token_account(
                &payer.pubkey(),
                &dest,
                &mint.pubkey(),
                &spl_token::id(),
            );

        let mint_to_ix = spl_token::instruction::mint_to(
            &spl_token::id(),
            &mint.pubkey(),
            &token_account,
            &payer.pubkey(),
            &[],
            amount,
        )
        .unwrap();

        ixs.push(create_account_ix);
        ixs.push(mint_to_ix);
    }

    if let Some(mint_authority) = parameters.mint_authority {
        let set_authority_ix = spl_token::instruction::set_authority(
            &parameters.token_program,
            &mint.pubkey(),
            mint_authority.as_ref(),
            spl_token::instruction::AuthorityType::MintTokens,
            &payer.pubkey(),
            &[],
        )
        .unwrap();
        ixs.push(set_authority_ix);
    }

    let mut tx = Transaction::new_with_payer(&ixs, Some(&payer.pubkey()));
    let signers: &[Keypair; 2] = &[payer.insecure_clone(), mint.insecure_clone()];

    // print all the accounts in tx and is_signer
    for (i, account) in tx.message().account_keys.iter().enumerate() {
        println!("account: {} {}", account, tx.message.is_signer(i));
    }

    // print the signers pubkey in array
    for signer in signers.iter() {
        let pos = tx.get_signing_keypair_positions(&[signer.pubkey()]);
        println!(
            "signer: {} {}",
            signer.insecure_clone().pubkey(),
            pos.unwrap()[0].unwrap_or(0)
        );
    }

    tx.try_partial_sign(
        &signers.iter().collect::<Vec<_>>(),
        ctx.get_blockhash().await,
    )
    .unwrap();

    Ok((tx, mint))
}

pub async fn set_authority_mint(
    ctx: &mut Box<dyn TestContext>,
    mint: &Pubkey,
    authority: &Keypair,
    new_authority: Option<Pubkey>,
    authority_type: spl_token::instruction::AuthorityType,
) -> Result<Transaction> {
    let ix = spl_token::instruction::set_authority(
        &spl_token::id(),
        mint,
        new_authority.as_ref(),
        authority_type,
        &authority.pubkey(),
        &[],
    )
    .unwrap();

    let mut tx = Transaction::new_with_payer(&[ix], Some(&authority.pubkey()));

    let signers: &[Keypair; 1] = &[authority.insecure_clone()];

    tx.try_partial_sign(
        &signers.iter().collect::<Vec<_>>(),
        ctx.get_blockhash().await,
    )
    .unwrap();

    Ok(tx)
}
