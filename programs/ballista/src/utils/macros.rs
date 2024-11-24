#[macro_export]
macro_rules! invoke_as_signer {
    ($user:expr, $user_pda:expr, $user_seed_index:expr, $ix:expr) => {
        let user_seed_index_bytes = $user_seed_index.to_le_bytes();
        let user_seed_bytes = user_seed_index_bytes.as_ref();
        let mut seeds = [
            "henchman".as_bytes(),
            $user.key().as_ref(),
            user_seed_bytes,
            &[0],
        ];

        if !$user.is_signer() {
            panic!("User is not a signer");
        }

        let (pubkey, bump) = find_program_address(&seeds[0..3], &$crate::ID);

        if pubkey != $user_pda.key().as_ref() {
            panic!("Mismatch PDA");
        }

        let bump_bytes = bump.to_le_bytes();
        seeds[3] = &bump_bytes;

        let seeds: [pinocchio::instruction::Seed; 4] = seeds.map(|s| s.into());

        let signer = pinocchio::instruction::Signer::from(seeds.as_slice());

        $ix.invoke_signed(&[signer]).unwrap();
    };
}

#[macro_export]
macro_rules! invoke_with_seeds {
    ($ix:expr, $accounts:expr, $user:expr, $user_pda:expr, $user_seed_index:expr) => {
        let user_seed_index_bytes = $user_seed_index.to_le_bytes();
        let user_seed_bytes = user_seed_index_bytes.as_ref();
        let mut seeds = [
            "henchman".as_bytes(),
            $user.key().as_ref(),
            user_seed_bytes,
            &[0],
        ];

        if !$user.is_signer() {
            panic!("User is not a signer");
        }

        let (pubkey, bump) = pinocchio::pubkey::find_program_address(&seeds[0..3], &$crate::ID);

        if pubkey != $user_pda.key().as_ref() {
            panic!("Mismatch PDA");
        }

        let bump_bytes = bump.to_le_bytes();
        seeds[3] = &bump_bytes;

        let seeds: [pinocchio::instruction::Seed; 4] = seeds.map(|s| s.into());

        let signer = pinocchio::instruction::Signer::from(seeds.as_slice());

        pinocchio::msg!("{:?}", &$ix);

        unsafe {
            pinocchio::program::invoke_signed_unchecked($ix, $accounts, &[signer]);
        }
    };
}
