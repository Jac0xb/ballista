use std::{ops::Deref, ptr};

use crate::{
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_task_account},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value, task::action::system_instruction::SystemInstructionAction,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    system_instruction::SystemInstruction,
    system_program,
};

pub fn evaluate(
    system_instruction: &SystemInstructionAction,
    task_state: &mut TaskState,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    let instruction: Instruction = match system_instruction {
        SystemInstructionAction::Transfer { from, to, amount } => {
            let from_account = evaluate_task_account(from, task_state)?;
            let to_account = evaluate_task_account(to, task_state)?;

            let amount = evaluate_expression(amount, task_state)?;
            let amount = match amount.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            task_state
                .account_info_cache
                .extend_from_slice(&[from_account.to_owned(), to_account.to_owned()]);

            task_state.account_meta_cache.extend_from_slice(&[
                AccountMeta {
                    pubkey: *from_account.key,
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: *to_account.key,
                    is_signer: false,
                    is_writable: true,
                },
            ]);

            unsafe {
                let mut instruction_data: Vec<u8> = Vec::from_raw_parts(
                    instruction_data_cache.as_ptr() as *mut u8,
                    instruction_data_cache.len(),
                    instruction_data_cache.capacity(),
                );

                bincode::serialize_into(
                    &mut instruction_data,
                    &SystemInstruction::Transfer { lamports: amount },
                )
                .unwrap();

                Instruction {
                    program_id: system_program::id(),
                    accounts: vec![
                        AccountMeta::new(*from_account.key, true),
                        AccountMeta::new(*to_account.key, false),
                    ],
                    data: instruction_data,
                }
            }
        }
        SystemInstructionAction::CreateAccount {
            payer,
            account,
            program_owner,
            space,
            lamports,
        } => {
            let payer = evaluate_task_account(payer, task_state)?;
            let account = evaluate_task_account(account, task_state)?;
            let owner = evaluate_task_account(program_owner, task_state)?;

            let space = evaluate_expression(space, task_state)?;
            let space = match space.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            let lamports = evaluate_expression(lamports, task_state)?;
            let lamports = match lamports.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            task_state.account_info_cache.push(account.to_owned());
            task_state.account_info_cache.push(owner.to_owned());

            // task_state
            //     .account_meta_cache
            //     .push(AccountMeta::new(*payer.key, true));
            // task_state
            //     .account_meta_cache
            //     .push(AccountMeta::new(*account.key, true));
            // task_state
            //     .account_meta_cache
            //     .push(AccountMeta::new(*owner.key, true));

            unsafe {
                // let accounts: Vec<AccountMeta> = Vec::from_raw_parts(
                //     task_state.account_meta_cache.as_ptr() as *mut AccountMeta,
                //     task_state.account_meta_cache.len(),
                //     task_state.account_meta_cache.capacity(),
                // );

                let mut instruction_data: Vec<u8> = Vec::from_raw_parts(
                    instruction_data_cache.as_ptr() as *mut u8,
                    instruction_data_cache.len(),
                    instruction_data_cache.capacity(),
                );

                bincode::serialize_into(
                    &mut instruction_data,
                    &SystemInstruction::CreateAccount {
                        lamports,
                        space,
                        owner: *owner.key,
                    },
                )
                .unwrap();

                Instruction {
                    program_id: system_program::id(),
                    accounts: vec![],
                    data: instruction_data,
                }
            }
        }
        _ => panic!("System instruction not implemented"),
    };

    solana_invoke::invoke(&instruction, task_state.account_info_cache.as_slice()).unwrap();

    Ok(())
}
