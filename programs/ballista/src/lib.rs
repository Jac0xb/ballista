use ballista_common::schema::TaskDefinition;
use borsh::{BorshDeserialize, BorshSerialize};
use error::BallistaError;
use geppetto::AccountInfoValidation;
use instruction::BallistaInstruction;
use pda::{get_task_definition_address, TASK_DEFINITION_SEED};

pub mod error;
pub mod evaluate;
pub mod instruction;
pub mod log;
pub mod pda;
pub mod processor;
pub mod task_state;

use pinocchio::{
    account_info::AccountInfo, instruction::Seed, msg, program_error::ProgramError, pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_pubkey::declare_id;

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

            let [user, task_account, system_program] = accounts else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };

            let (task_pda, bump_seed) = get_task_definition_address(user.key(), task_id);

            // [account(program === pinocchio_system::ID)]
            system_program.assert_program(&pinocchio_system::ID)?;

            // [account(writable, signer)]
            user.assert_signer()?.assert_writable()?;

            // [account(writable, unowned, empty, key === task_pda)]
            task_account
                .assert_writable()?
                .assert_owner(&pinocchio_system::ID)?
                .assert_empty()?
                .assert_key(&task_pda)?;

            debug_msg!("passed validation");

            let serialized_task = task.try_to_vec().map_err(|e| {
                msg!("Error serializing task: {}", e);
                BallistaError::InvalidSchemaData
            })?;

            let task_id_bytes = task_id.to_le_bytes();
            let seeds = [
                Seed::from(TASK_DEFINITION_SEED),
                Seed::from(user.key().as_ref()),
                Seed::from(task_id_bytes.as_ref()),
            ];

            geppetto::allocate_account_with_bump(
                task_account,
                system_program,
                user,
                serialized_task.len(),
                &crate::ID,
                &seeds,
                bump_seed,
            )?;

            debug_msg!("created account");

            task_account
                .try_borrow_mut_data()
                .unwrap()
                .copy_from_slice(&serialized_task);
        }
        BallistaInstruction::ExecuteTask { task_values } => {
            debug_msg!("Instruction: ExecuteTask");

            let schema_account = &accounts[0];
            let payer = &accounts[1];

            // [account(writable, signer)]
            payer.assert_writable()?.assert_signer()?;

            // [account(!empty)]
            schema_account.assert_not_empty()?;

            let task_definition =
                TaskDefinition::try_from_slice(&schema_account.try_borrow_data().unwrap())
                    .or(Err(BallistaError::InvalidSchemaData))?;

            processor::execute_action::process(
                &task_definition,
                &task_values,
                payer,
                &accounts[2..],
            )
            .unwrap();
        }
    }

    Ok(())
}
