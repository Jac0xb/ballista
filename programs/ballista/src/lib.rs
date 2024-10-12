use std::vec;

use ballista_common::schema::TaskDefinition;
use borsh::{BorshDeserialize, BorshSerialize};

use error::BallistaError;
use instruction::BallistaInstruction;
use schema::get_task_definition_address;
use solana_program::msg;
use solana_program::{account_info::AccountInfo, declare_id, rent::Rent, sysvar::Sysvar};
use solana_program::{entrypoint::ProgramResult, pubkey::Pubkey};
use validation::account::create_account;

pub mod allocator;
pub mod error;
pub mod evaluate;
pub mod instruction;
pub mod log;
pub mod processor;
pub mod schema;
pub mod task_state;
pub mod validation;

declare_id!("BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2");

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
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

            let user = &accounts[0];
            let task_account = &accounts[1];
            let system_program = &accounts[2];

            if system_program.key != &solana_program::system_program::ID {
                msg!("system program mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            if !user.is_signer {
                msg!("user account is not a signer");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            // find bump seed
            let mut seeds = vec![
                "task-definition".as_bytes().to_vec(),
                user.key.as_ref().to_vec(),
                task_id.to_le_bytes().to_vec(),
            ];

            let (task_pubkey, bump_seed) =
                get_task_definition_address(user.key, task_id).map_err(|e| {
                    msg!("Error getting task definition address: {}", e);
                    BallistaError::InvalidSchemaData
                })?;

            if task_pubkey != *task_account.key {
                msg!("schema account mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            let serialized_task = task.try_to_vec().map_err(|e| {
                msg!("Error serializing task: {}", e);
                BallistaError::InvalidSchemaData
            })?;

            seeds.push(bump_seed.to_le_bytes().to_vec());

            create_account(
                user,
                task_account,
                system_program,
                &crate::ID,
                &Rent::get()?,
                serialized_task.len() as u64,
                seeds,
            )
            .unwrap();

            task_account
                .try_borrow_mut_data()
                .unwrap()
                .copy_from_slice(&serialized_task);
        }
        BallistaInstruction::ExecuteTask { task_values } => {
            debug_msg!("Instruction: ExecuteTask");

            let schema_account = &accounts[0];
            let payer = &accounts[1];

            if !payer.is_writable {
                debug_msg!("payer account is not writable");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            if !payer.is_signer {
                debug_msg!("payer account is not a signer");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            if schema_account.data_is_empty() {
                debug_msg!("schema account is uninitialized");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            let task_definition = TaskDefinition::try_from_slice(&schema_account.data.borrow())
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
