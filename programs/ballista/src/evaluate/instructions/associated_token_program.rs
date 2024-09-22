use crate::{evaluate::evaluate_task_account, task_state::TaskState};
use ballista_common::task::action::associated_token_program_instruction::AssociatedTokenProgramInstruction;
use solana_program::instruction::{AccountMeta, Instruction};

pub fn evaluate(
    program_instruction: &AssociatedTokenProgramInstruction,
    task_state: &mut TaskState,
) -> Result<(), String> {
    // let mut account_infos = vec![];

    let instruction: Instruction = match program_instruction {
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

            task_state.account_meta_cache.extend_from_slice(&[
                AccountMeta::new(*payer.key, true),
                AccountMeta::new(*token_account.key, false),
                AccountMeta::new_readonly(*owner.key, false),
                AccountMeta::new_readonly(*mint.key, false),
                AccountMeta::new_readonly(solana_program::system_program::id(), false),
                AccountMeta::new_readonly(*token_program_id.key, false),
            ]);

            task_state.account_info_cache.extend_from_slice(&[
                payer.to_owned(),
                token_account.to_owned(),
                owner.to_owned(),
                mint.to_owned(),
                system_program_id.to_owned(),
                token_program_id.to_owned(),
            ]);

            Instruction {
                program_id: spl_associated_token_account::ID,
                accounts: task_state.account_meta_cache.to_vec(),
                data: vec![0],
            }
        }
    };

    solana_invoke::invoke(&instruction, task_state.account_info_cache.as_slice()).unwrap();

    Ok(())
}
