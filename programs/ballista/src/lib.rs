use ballista_common::accounts::task_definition::TaskDefinition;
use borsh::BorshDeserialize;
use error::BallistaError;
use geppetto::{AccountInfoValidation, AsAccount};
use instruction::BallistaInstruction;

pub mod error;
pub mod evaluate;
pub mod instruction;
pub mod processor;
pub mod utils;

use pinocchio::{
    account_info::AccountInfo, instruction::Seed, program_error::ProgramError, pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_pubkey::declare_id;
use utils::pda::{get_task_definition_address, TASK_DEFINITION_SEED};

declare_id!("BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2");

#[cfg(not(feature = "no-entrypoint"))]
mod init {
    use crate::process_instruction;
    use pinocchio::entrypoint;
    entrypoint!(process_instruction);
}

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let mut remaining_instruction_data = instruction_data;
    let instruction =
        BallistaInstruction::deserialize(&mut remaining_instruction_data).map_err(|e| {
            debug_msg!("Error deserializing instruction: {}", e);
            BallistaError::InvalidInstructionData
        })?;

    match instruction {
        BallistaInstruction::CreateTask { task, task_id } => {
            debug_msg!("Instruction: CreateTask");

            let [payer, task_account, system_program] = accounts else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };

            let (task_pda, _) = get_task_definition_address(payer.key(), task_id);

            // [account(program === pinocchio_system::ID)]
            system_program.assert_program(&pinocchio_system::ID)?;

            // [account(writable, signer)]
            payer.assert_signer()?.assert_writable()?;

            // [account(writable, unowned, empty, key === task_pda)]
            task_account
                .assert_writable()?
                .assert_owner(&pinocchio_system::ID)?
                .assert_empty()?
                .assert_key(&task_pda)?;

            debug_msg!("passed validation");

            let task_id_bytes = task_id.to_le_bytes();
            let seeds = [
                Seed::from(TASK_DEFINITION_SEED),
                Seed::from(payer.key().as_ref()),
                Seed::from(task_id_bytes.as_ref()),
            ];

            debug_msg!("created account");

            task_account.create_account::<TaskDefinition>(
                &task,
                system_program,
                payer,
                &crate::ID,
                &seeds,
            )?;
        }
        BallistaInstruction::ExecuteTask { input_values } => {
            debug_msg!("Instruction: ExecuteTask");

            let [payer, task_account, ref remaining_accounts @ ..] = accounts else {
                debug_msg!("Not enough account keys {}", accounts.len());
                return Err(ProgramError::NotEnoughAccountKeys);
            };

            // [account(writable, signer)]
            payer.assert_writable()?.assert_signer()?;

            let task_definition = task_account.as_account::<TaskDefinition>(&crate::ID)?;

            processor::execute(&task_definition, &input_values, payer, remaining_accounts).unwrap();
        }
        BallistaInstruction::ExecuteTaskNoInputs {} => {
            debug_msg!("Instruction: ExecuteTaskNoInputs");

            let [payer, task_account, ref remaining_accounts @ ..] = accounts else {
                debug_msg!("Not enough account keys {}", accounts.len());
                return Err(ProgramError::NotEnoughAccountKeys);
            };

            // [account(writable, signer)]
            payer.assert_writable()?.assert_signer()?;

            let task_definition = task_account.as_account::<TaskDefinition>(&crate::ID)?;

            processor::execute(&task_definition, &[], payer, remaining_accounts).unwrap();
        }
    }

    Ok(())
}
