use ballista_common::schema::{Schema, TaskDefinition};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, msg, program::invoke, pubkey::Pubkey, rent::Rent,
    system_instruction, sysvar::Sysvar,
};

use crate::{error::BallistaError, ID};

#[derive(BorshSerialize, BorshDeserialize)]
pub enum UpdateSchemaArgs {
    AddTask {
        task: TaskDefinition,
    },
    RemoveTask {
        task_index: u8,
    },
    SetTask {
        task_index: u8,
        task: TaskDefinition,
    },
}

pub fn get_schema_address(owner: &Pubkey, id: u8) -> Result<(Pubkey, u8), BallistaError> {
    let (pubkey, bump) =
        Pubkey::find_program_address(&["schema".as_bytes(), owner.as_ref(), &[id]], &ID);

    Ok((pubkey, bump))
}

pub fn update_schema_account<'info>(
    payer: &AccountInfo<'info>,
    schema_account: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    update_fn: impl FnOnce(&mut Schema) -> Result<(), BallistaError>,
) -> Result<(), BallistaError> {
    let serialized_schema = schema_account.try_borrow_data().unwrap();
    let previous_serialized_len = serialized_schema.len();
    let current_lamports = **schema_account.try_borrow_lamports().unwrap();

    if previous_serialized_len == 0 {
        msg!("schema account is uninitialized");
        return Err(BallistaError::InvalidSchemaData);
    }

    let mut schema = Schema::try_from_slice(&serialized_schema).map_err(|_| {
        msg!("Invalid schema data");
        BallistaError::InvalidSchemaData
    })?;

    update_fn(&mut schema)?;
    drop(serialized_schema);

    let new_serialized_schema = &schema.try_to_vec().unwrap();
    let new_serialized_len = new_serialized_schema.len();

    if previous_serialized_len < new_serialized_len {
        let required_lamports = Rent::get()
            .unwrap()
            .minimum_balance(new_serialized_len)
            .max(1)
            .saturating_sub(current_lamports);

        solana_invoke::invoke(
            &system_instruction::transfer(payer.key, schema_account.key, required_lamports),
            &[
                payer.clone(),
                schema_account.clone(),
                system_program.clone(),
            ],
        )
        .map_err(|e| {
            msg!("Error transferring lamports: {}", e);
            BallistaError::InvalidSchemaData
        })?;
    }

    schema_account
        .realloc(new_serialized_len, true)
        .map_err(|e| {
            msg!("Error reallocating schema account: {}", e);
            BallistaError::InvalidSchemaData
        })?;

    schema_account
        .try_borrow_mut_data()
        .unwrap()
        .copy_from_slice(new_serialized_schema);

    Ok(())
}
