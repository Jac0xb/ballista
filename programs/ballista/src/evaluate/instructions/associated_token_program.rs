use crate::{error::BallistaError, evaluate::evaluate_task_account, task_state::TaskState};
use ballista_common::task::action::associated_token_program_instruction::AssociatedTokenProgramInstruction;
use pinocchio::msg;
use pinocchio_associated_token::instructions::Create;
use solana_program::instruction::{AccountMeta, Instruction};
use spl_token::instruction::transfer;

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
            let payer = evaluate_task_account(payer, task_state)?;
            let owner = evaluate_task_account(owner, task_state)?;
            let token_account = evaluate_task_account(token_account, task_state)?;
            let mint = evaluate_task_account(mint, task_state)?;
            let token_program_id = evaluate_task_account(token_program_id, task_state)?;
            let system_program_id = evaluate_task_account(system_program_id, task_state)?;

            let create = Create {
                funding_account: payer,
                associated_account: token_account,
                wallet_address: owner,
                token_mint: mint,
                system_program: system_program_id,
                token_program: token_program_id,
            };
            // .invoke()
            // .unwrap();

            // msg!(
            //     "Payer address: {:?}",
            //     bs58::encode(payer.key()).into_string()
            // );
            // msg!(
            //     "Owner address: {:?}",
            //     bs58::encode(owner.key()).into_string()
            // );
            // msg!(
            //     "Token account address: {:?}",
            //     bs58::encode(token_account.key()).into_string()
            // );
            // msg!("Mint address: {:?}", bs58::encode(mint.key()).into_string());
            // msg!(
            //     "Token program address: {:?}",
            //     bs58::encode(token_program_id.key()).into_string()
            // );

            create.invoke().unwrap();

            // task_state.account_meta_cache.extend_from_slice(&[
            //     AccountMeta::new(*payer.key, true),
            //     AccountMeta::new(*token_account.key, false),
            //     AccountMeta::new_readonly(*owner.key, false),
            //     AccountMeta::new_readonly(*mint.key, false),
            //     AccountMeta::new_readonly(solana_program::system_program::id(), false),
            //     AccountMeta::new_readonly(*token_program_id.key, false),
            // ]);

            // task_state.account_info_cache.extend_from_slice(&[
            //     payer.to_owned(),
            //     token_account.to_owned(),
            //     owner.to_owned(),
            //     mint.to_owned(),
            //     system_program_id.to_owned(),
            //     token_program_id.to_owned(),
            // ]);

            // Instruction {
            //     program_id: spl_associated_token_account::ID,
            //     accounts: task_state.account_meta_cache.to_vec(),
            //     data: vec![0],
            // }
        }
    };

    // solana_invoke::invoke(&instruction, task_state.account_info_cache.as_slice()).unwrap();

    Ok(())
}
