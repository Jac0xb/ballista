use borsh::BorshDeserialize;
use solana_sdk::{bs58, pubkey::Pubkey};

pub mod generated;
pub mod hooked;

// pub const BALLISTA_ID: Pubkey = Pubkey::new_from_array([
//     150, 248, 245, 226, 109, 155, 96, 59, 21, 174, 18, 123, 188, 221, 84, 176, 87, 205, 234, 133,
//     229, 193, 223, 218, 219, 137, 12, 168, 10, 84, 183, 214,
// ]);

pub const ID: Pubkey = generated::BALLISTA_ID;
pub const BALLISTA_ID: Pubkey = generated::BALLISTA_ID;

pub fn find_schema_pda(user: Pubkey, id: u8) -> (solana_program::pubkey::Pubkey, u8) {
    println!("crate id: {:?}", crate::ID);

    solana_program::pubkey::Pubkey::find_program_address(
        &["schema".to_string().as_ref(), user.as_ref(), &[id]],
        &crate::ID,
    )
}
