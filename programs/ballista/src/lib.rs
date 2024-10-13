use ballista_common::schema::TaskDefinition;
use borsh::{BorshDeserialize, BorshSerialize};

use error::BallistaError;
use instruction::BallistaInstruction;
use pinocchio_system::instructions::CreateAccount;
use schema::get_task_definition_address;
use solana_program::declare_id;

// pub mod allocator;
pub mod error;
pub mod evaluate;
pub mod instruction;
pub mod log;
pub mod processor;
pub mod schema;
pub mod task_state;
pub mod validation;

use pinocchio::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    instruction::{Seed, Signer},
    msg,
    pubkey::Pubkey,
    sysvars::{rent::Rent, Sysvar},
};

use pinocchio::entrypoint;

declare_id!("BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2");

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

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

            let user = &accounts[0];
            let task_account = &accounts[1];
            let system_program = &accounts[2];

            if system_program.key() != &pinocchio_system::ID {
                msg!("system program mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            if !user.is_signer() {
                msg!("user account is not a signer");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            let (task_pubkey, bump_seed) = get_task_definition_address(user.key(), task_id)
                .map_err(|e| {
                    msg!("Error getting task definition address: {}", e);
                    BallistaError::InvalidSchemaData
                })?;

            if task_pubkey != *task_account.key() {
                msg!("schema account mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            debug_msg!("passed validation");

            let serialized_task = task.try_to_vec().map_err(|e| {
                msg!("Error serializing task: {}", e);
                BallistaError::InvalidSchemaData
            })?;

            debug_msg!("serialized task");
            let task_id_bytes = task_id.to_le_bytes();
            let bump_seed_bytes = bump_seed.to_le_bytes();

            // find bump seed
            let seeds = [
                Seed::from("task-definition".as_bytes()),
                Seed::from(user.key().as_ref()),
                Seed::from(task_id_bytes.as_ref()),
                Seed::from(bump_seed_bytes.as_ref()),
            ];

            let signer: Signer = seeds.as_slice().into();

            debug_msg!("created signer");

            CreateAccount {
                from: user,
                to: task_account,
                lamports: Rent::get()?.minimum_balance(serialized_task.len()),
                space: serialized_task.len() as u64,
                owner: &crate::ID.to_bytes(),
            }
            .invoke_signed(&[signer])?;

            debug_msg!("created account");

            task_account
                // .task_account
                .try_borrow_mut_data()
                .unwrap()
                .copy_from_slice(&serialized_task);
        }
        BallistaInstruction::ExecuteTask { task_values } => {
            debug_msg!("Instruction: ExecuteTask");

            let schema_account = &accounts[0];
            let payer = &accounts[1];

            if !payer.is_writable() {
                debug_msg!("payer account is not writable");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            if !payer.is_signer() {
                debug_msg!("payer account is not a signer");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            if schema_account.data_is_empty() {
                debug_msg!("schema account is uninitialized");
                return Err(BallistaError::InvalidSchemaData.into());
            }

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
