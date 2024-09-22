use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program::invoke_signed, pubkey::Pubkey,
    rent::Rent, system_instruction,
};

pub fn create_account<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    new_account: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
    program_owner: &Pubkey,
    rent: &Rent,
    space: u64,
    seeds: Vec<Vec<u8>>,
) -> ProgramResult {
    // let current_lamports = **new_account.try_borrow_lamports().unwrap();
    // if current_lamports == 0 {
    // If there are no lamports in the new account, we create it with the create_account instruction
    // invoke_signed(
    //     &system_instruction::create_account(
    //         payer.key,
    //         new_account.key,
    //         rent.minimum_balance(space as usize),
    //         space,
    //         program_owner,
    //     ),
    //     &[payer.clone(), new_account.clone(), system_program.clone()],
    //     &[seeds
    //         .iter()
    //         .map(|seed| seed.as_slice())
    //         .collect::<Vec<&[u8]>>()
    //         .as_slice()],
    // )

    let instruction = &system_instruction::create_account(
        payer.key,
        new_account.key,
        rent.minimum_balance(space as usize),
        space,
        program_owner,
    );

    invoke_signed(
        instruction,
        &[payer.clone(), new_account.clone(), system_program.clone()],
        &[seeds
            .iter()
            .map(|seed| seed.as_slice())
            .collect::<Vec<&[u8]>>()
            .as_slice()],
    )

    // msg!("instruction {:?}", instruction);

    // let account_metas = instruction
    //     .accounts
    //     .clone()
    //     .into_iter()
    //     .map(|account| AccountMetaC {
    //         pubkey: &account.pubkey,
    //         is_signer: account.is_signer,
    //         is_writable: account.is_writable,
    //     })
    //     .collect::<Vec<_>>();

    // let account_metas = [
    //     AccountMetaC {
    //         pubkey: payer.key(),
    //         is_signer: true,
    //         is_writable: true,
    //     },
    //     AccountMetaC {
    //         pubkey: new_account.key(),
    //         is_signer: true,
    //         is_writable: true,
    //     },
    //     AccountMetaC {
    //         pubkey: system_program.key(),
    //         is_signer: false,
    //         is_writable: false,
    //     },
    // ];

    // let instruction = InstructionC {
    //     program_id: system_program.key(),
    //     accounts: account_metas.as_ptr(),
    //     accounts_len: account_metas.len() as u64,
    //     data: instruction.data.as_ptr(),
    //     data_len: instruction.data.len() as u64,
    // };

    // let infos = [
    //     system_program.to_info_c(),
    //     payer.to_info_c(),
    //     new_account.to_info_c(),
    // ];
    // let seeds: &[&[&[u8]]] = &[&seeds
    //     .iter()
    //     .map(|seed| seed.as_slice())
    //     .collect::<Vec<&[u8]>>()];

    // // Invoke system program
    // #[cfg(target_os = "solana")]
    // unsafe {
    //     solana_program::syscalls::sol_invoke_signed_c(
    //         &instruction as *const InstructionC as *const u8,
    //         infos.as_ptr() as *const u8,
    //         infos.len() as u64,
    //         seeds.as_ptr() as *const u8,
    //         seeds.len() as u64,
    //     );
    // }

    // // // For clippy
    // #[cfg(not(target_os = "solana"))]
    // core::hint::black_box(&(&instruction, &infos, &seeds));

    // Ok(())
    // } else {
    //     // Fund the account for rent exemption.
    //     let required_lamports = rent
    //         .minimum_balance(space as usize)
    //         .max(1)
    //         .saturating_sub(current_lamports);
    //     if required_lamports > 0 {
    //         invoke(
    //             &system_instruction::transfer(payer.key, new_account.key, required_lamports),
    //             &[payer.clone(), new_account.clone(), system_program.clone()],
    //         )?;
    //     }
    //     // Allocate space.
    //     invoke_signed(
    //         &system_instruction::allocate(new_account.key, space),
    //         &[new_account.clone(), system_program.clone()],
    //         &[seeds
    //             .iter()
    //             .map(|seed| seed.as_slice())
    //             .collect::<Vec<&[u8]>>()
    //             .as_slice()],
    //     )?;
    //     // Assign to the specified program
    //     invoke_signed(
    //         &system_instruction::assign(new_account.key, program_owner),
    //         &[new_account.clone(), system_program.clone()],
    //         &[seeds
    //             .iter()
    //             .map(|seed| seed.as_slice())
    //             .collect::<Vec<&[u8]>>()
    //             .as_slice()],
    //     )
    // }
}

// pub fn close<'info>(info: &NoStdAccountInfo, sol_destination: &NoStdAccountInfo) -> Result<()> {
//     // Transfer tokens from the account to the sol_destination.
//     let dest_starting_lamports = sol_destination.lamports();
//     **sol_destination.lamports.borrow_mut() =
//         dest_starting_lamports.checked_add(info.lamports()).unwrap();
//     **info.lamports.borrow_mut() = 0;

//     info.assign(&system_program::ID);
//     info.realloc(0, false).map_err(Into::into)
// }

// fn init_if_needed<'a, 'info>(
//     account_info: &'_ NoStdAccountInfo,
//     payer: &'_ NoStdAccountInfo,
//     space: u64,
//     program_owner: &'_ Pubkey,
//     system_program: &'_ NoStdAccountInfo,
//     seeds: &'_ [Vec<u8>],
// ) -> Result<bool> {
//     if account_info.owner().eq(&system_program.key()) {
//         create_account(
//             payer.as_ref(),
//             account_info,
//             system_program.info,
//             program_owner,
//             &Rent::get()?,
//             space,
//             seeds.to_vec(),
//         )?;

//         Ok(true)
//     } else if keys_equal(account_info.owner, program_owner) {
//         Ok(false)
//     } else {
//         msg!("Unexpected program owner");
//         Err(LighthouseError::AccountValidationFailed.into())
//     }
// }

// fn realloc(
//     account_info: &'_ NoStdAccountInfo,
//     payer: &'_ Signer<'a, 'info>,
//     system_program: &'_ Program<'a, 'info, SystemProgram>,
//     space: u64,
// ) -> Result<()> {
//     let current_lamports = **account_info.try_borrow_lamports()?;
// let required_lamports = Rent::get()?
//     .minimum_balance(space as usize)
//     .max(1)
//     .saturating_sub(current_lamports);
// if required_lamports > 0 {
//     invoke(
//         &system_instruction::transfer(payer.key, account_info.key, required_lamports),
//         &[
//             payer.info_as_owned(),
//             account_info.clone(),
//             system_program.info_as_owned(),
//         ],
//     )?;
// }

//     account_info.realloc(space as usize, false)
// }
