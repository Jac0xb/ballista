use std::vec;

use ballista_common::schema::Schema;
use borsh::{BorshDeserialize, BorshSerialize};

use error::BallistaError;
use instruction::BallistaInstruction;
use schema::update_schema::{get_schema_address, update_schema_account};
use solana_program::{account_info::AccountInfo, declare_id, msg, rent::Rent, sysvar::Sysvar};
use solana_program::{entrypoint::ProgramResult, pubkey::Pubkey};
use validation::account::create_account;

pub mod allocator;
pub mod error;
pub mod evaluate;
pub mod instruction;
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
            msg!("Error deserializing instruction: {}", e);
            BallistaError::InvalidInstructionData
        })?;

    match instruction {
        BallistaInstruction::CreateSchema { schema } => {
            let program = &accounts[0];
            let system_program = &accounts[1];
            let user = &accounts[2];
            let schema_account = &accounts[3];

            // find bump seed
            let mut seeds = vec![
                "schema".as_bytes().to_vec(),
                accounts[2].key.as_ref().to_vec(),
                vec![0u8],
            ];

            let (schema_pubkey, bump_seed) = get_schema_address(user.key, 0)?;

            if schema_pubkey != *schema_account.key {
                msg!("schema account mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            let serialized_schema = schema.try_to_vec().unwrap();

            seeds.push(bump_seed.to_le_bytes().to_vec());

            create_account(
                user,
                schema_account,
                system_program,
                program.key,
                &Rent::get()?,
                serialized_schema.len() as u64,
                seeds,
            )
            .unwrap();

            schema_account
                .try_borrow_mut_data()
                .unwrap()
                .copy_from_slice(&serialized_schema);
        }
        BallistaInstruction::AddTask { task } => {
            let user_account = &accounts[0];
            let schema_account = &accounts[1];
            let system_program = &accounts[2];

            let (schema_pubkey, _) = get_schema_address(user_account.key, 0)?;

            if schema_pubkey != *schema_account.key {
                msg!("schema account mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            update_schema_account(user_account, schema_account, system_program, |schema| {
                schema.tasks.push(task);
                Ok(())
            })?;
        }
        BallistaInstruction::RemoveTask { task_index } => {
            let user_account = &accounts[0];
            let schema_account = &accounts[1];
            let system_program = &accounts[2];

            let (schema_pubkey, _) = get_schema_address(user_account.key, 0)?;

            if schema_pubkey != *schema_account.key {
                msg!("schema account mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            update_schema_account(user_account, schema_account, system_program, |schema| {
                schema.tasks.remove(task_index as usize);
                Ok(())
            })?;
        }
        BallistaInstruction::SetTask { task_index, task } => {
            let user_account = &accounts[0];
            let schema_account = &accounts[1];
            let system_program = &accounts[2];
            let (schema_pubkey, _) = get_schema_address(user_account.key, 0)?;

            if schema_pubkey != *schema_account.key {
                msg!("schema account mismatch");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            update_schema_account(user_account, schema_account, system_program, |schema| {
                schema.tasks[task_index as usize] = task;
                Ok(())
            })?;
        }
        BallistaInstruction::ExecuteTask { task_values } => {
            let schema_account = &accounts[0];

            if schema_account.data_is_empty() {
                msg!("schema account is uninitialized");
                return Err(BallistaError::InvalidSchemaData.into());
            }

            let schema = Schema::try_from_slice(&schema_account.data.borrow())
                .or(Err(BallistaError::InvalidSchemaData))?;

            processor::execute_action::process(&schema, 0, &task_values, &accounts[1..]).unwrap();
        }
    }

    Ok(())
}
