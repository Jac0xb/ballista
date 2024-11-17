use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::EncodableKeypair};

pub fn get_associated_token_address_and_bump_seed_internal(
    wallet_address: &Pubkey,
    token_mint_address: &Pubkey,
    program_id: &Pubkey,
    token_program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            &wallet_address.to_bytes(),
            &token_program_id.to_bytes(),
            &token_mint_address.to_bytes(),
        ],
        program_id,
    )
}

pub fn generate_max_bump_token_account_user(mint: &Pubkey) -> (Pubkey, u8) {
    loop {
        let user = Keypair::new().encodable_pubkey();
        let (_, bump) = get_associated_token_address_and_bump_seed_internal(
            &user,
            mint,
            &spl_associated_token_account::ID,
            &spl_token::ID,
        );

        if bump == 255 {
            return (user, bump);
        }
    }
}
