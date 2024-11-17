use solana_sdk::pubkey::Pubkey;

pub mod generated;
pub mod hooked;

pub const ID: Pubkey = generated::BALLISTA_ID;
pub const BALLISTA_ID: Pubkey = generated::BALLISTA_ID;

pub fn find_task_definition_pda(user: Pubkey, id: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            "task-definition".to_string().as_ref(),
            user.as_ref(),
            id.to_le_bytes().as_ref(),
        ],
        &crate::ID,
    )
}

pub fn find_seeded_pda(user: Pubkey, seed_index: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            "henchman".as_bytes(),
            user.as_ref(),
            seed_index.to_le_bytes().as_ref(),
        ],
        &crate::ID,
    )
}

// let mut seeds = [
//     "henchman".as_bytes(),
//     $user.key().as_ref(),
//     user_seed_bytes,
//     &[0],
// ];
