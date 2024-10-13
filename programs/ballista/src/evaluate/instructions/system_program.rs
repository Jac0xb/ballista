use std::{ops::Deref, ptr};

use crate::{
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_task_account},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value, task::action::system_instruction::SystemInstructionAction,
};
use pinocchio::instruction::{AccountMeta, Instruction};
use pinocchio_system::instructions::{CreateAccount, Transfer};
use solana_program::{system_instruction::SystemInstruction, system_program};

pub fn evaluate(
    system_instruction: &SystemInstructionAction,
    task_state: &mut TaskState,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    match system_instruction {
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
                .extend_from_slice(&[from_account.into(), to_account.into()]);

            task_state.account_meta_cache.extend_from_slice(&[
                AccountMeta {
                    pubkey: from_account.key(),
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: to_account.key(),
                    is_signer: false,
                    is_writable: true,
                },
            ]);

            // unsafe {
            //     let mut instruction_data: Vec<u8> = Vec::from_raw_parts(
            //         instruction_data_cache.as_ptr() as *mut u8,
            //         instruction_data_cache.len(),
            //         instruction_data_cache.capacity(),
            //     );

            //     bincode::serialize_into(
            //         &mut instruction_data,
            //         &SystemInstruction::Transfer { lamports: amount },
            //     )
            //     .unwrap();

            Transfer {
                from: &from_account,
                to: &to_account,
                lamports: amount,
            }
            .invoke()
            .map_err(|_| BallistaError::Todo)?;

            Ok(())

            // Instruction {
            //     program_id: &system_program::id().to_bytes(),
            //     accounts: &[
            //         AccountMeta {
            //             pubkey: from_account.key(),
            //             is_signer: true,
            //             is_writable: true,
            //         },
            //         AccountMeta {
            //             pubkey: to_account.key(),
            //             is_signer: false,
            //             is_writable: true,
            //         },
            //     ],
            //     data: instruction_data.as_slice(),
            // };
            // }
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

            task_state.account_info_cache.push(account.into());
            task_state.account_info_cache.push(owner.into());

            // task_state
            //     .account_meta_cache
            //     .push(AccountMeta::new(*payer.key, true));
            // task_state
            //     .account_meta_cache
            //     .push(AccountMeta::new(*account.key, true));
            // task_state
            //     .account_meta_cache
            //     .push(AccountMeta::new(*owner.key, true));

            CreateAccount {
                from: &payer,
                to: &account,
                lamports,
                space,
                owner: owner.key(),
            }
            .invoke();

            Ok(())

            // unsafe {
            // unsafe {
            // let accounts: Vec<AccountMeta> = Vec::from_raw_parts(
            //     task_state.account_meta_cache.as_ptr() as *mut AccountMeta,
            //     task_state.account_meta_cache.len(),
            //     task_state.account_meta_cache.capacity(),
            // );

            // let mut instruction_data: Vec<u8> = Vec::from_raw_parts(
            //     instruction_data_cache.as_ptr() as *mut u8,
            //     instruction_data_cache.len(),
            //     instruction_data_cache.capacity(),
            // );

            // bincode::serialize_into(
            //     &mut instruction_data,
            //     &SystemInstruction::CreateAccount {
            //         lamports,
            //         space,
            //         owner: owner.key(),
            //     },
            // )
            // .unwrap();

            // Instruction {
            //     program_id: &system_program::id().to_bytes(),
            //     accounts: &[],
            //     data: instruction_data.as_slice(),
            // }
            // }
        }
        _ => panic!("System instruction not implemented"),
    }

    // solana_invoke::invoke(&instruction, task_state.account_info_cache.as_slice()).unwrap();
}
