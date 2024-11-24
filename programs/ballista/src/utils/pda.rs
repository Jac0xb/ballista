use pinocchio::pubkey::{find_program_address, Pubkey};

pub const TASK_DEFINITION_SEED: &[u8] = b"task-definition";

pub fn get_task_definition_address(owner: &Pubkey, id: u16) -> (Pubkey, u8) {
    let (pubkey, bump) = find_program_address(
        &[TASK_DEFINITION_SEED, owner.as_ref(), &id.to_le_bytes()],
        &crate::ID,
    );

    (pubkey, bump)
}