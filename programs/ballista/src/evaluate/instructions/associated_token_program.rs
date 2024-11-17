use crate::{error::BallistaError, evaluate::evaluate_task_account, task_state::TaskState, ID};
use ballista_common::task::action::associated_token_program_instruction::AssociatedTokenProgramInstruction;
use pinocchio::{
    instruction::{Seed, Signer},
    pubkey::find_program_address,
};
use pinocchio_associated_token::instructions::Create;

pub fn evaluate(
    program_instruction: &AssociatedTokenProgramInstruction,
    task_state: &mut TaskState,
) -> Result<(), BallistaError> {
    // let mut account_infos = vec![];

    match program_instruction {
        AssociatedTokenProgramInstruction::InitializeAccount {
            payer,
            owner,
            token_account,
            mint,
            token_program_id,
            system_program_id,
        } => {
            let (payer, payer_seed) = evaluate_task_account(payer, task_state)?;
            let (owner, _) = evaluate_task_account(owner, task_state)?;
            let (token_account, _) = evaluate_task_account(token_account, task_state)?;
            let (mint, _) = evaluate_task_account(mint, task_state)?;
            let (token_program_id, _) = evaluate_task_account(token_program_id, task_state)?;
            let (system_program_id, _) = evaluate_task_account(system_program_id, task_state)?;

            let create = Create {
                funding_account: payer,
                associated_account: token_account,
                wallet_address: owner,
                token_mint: mint,
                system_program: system_program_id,
                token_program: token_program_id,
            };

            if let Some(payer_seed) = payer_seed {
                let payer_seed_bytes = payer_seed.to_le_bytes();
                let mut seeds = [
                    "henchman".as_bytes(),
                    owner.key().as_ref(),
                    payer_seed_bytes.as_ref(),
                    &[0],
                ];

                let (pubkey, bump) = find_program_address(&seeds[0..3], &ID);

                if pubkey != payer.key().as_ref() {
                    // TODO: Make better error
                    return Err(BallistaError::InvalidPDA);
                }

                let bump_bytes = bump.to_le_bytes();
                seeds[3] = &bump_bytes;

                let seeds: [Seed; 4] = seeds.map(|s| s.into());

                let signer = Signer::from(seeds.as_slice());

                create.invoke_signed(&[signer]).unwrap();
            } else {
                create.invoke().unwrap();
            }
        }
    };

    Ok(())
}

// generate bump
