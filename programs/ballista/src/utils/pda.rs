use solana_address::Address;

pub const TEMPLATE_SEED: &[u8] = b"template";

pub fn get_template_address(creator: &Address, id: u16) -> (Address, u8) {
    let (pubkey, bump) = Address::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &id.to_le_bytes()],
        &crate::ID,
    );

    (pubkey, bump)
}
