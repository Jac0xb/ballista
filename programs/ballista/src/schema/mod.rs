// pub mod update_schema;

use solana_program::pubkey::Pubkey;

use crate::error::BallistaError;

pub fn get_task_definition_address(owner: &Pubkey, id: u16) -> Result<(Pubkey, u8), BallistaError> {
    let (pubkey, bump) = Pubkey::find_program_address(
        &[
            "task-definition".as_bytes(),
            owner.as_ref(),
            &id.to_le_bytes(),
        ],
        &crate::ID,
    );

    Ok((pubkey, bump))
}
