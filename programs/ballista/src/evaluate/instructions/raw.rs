use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_program_task_account},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value,
    task::{action::raw_instruction::RawInstruction, shared::TaskAccounts},
};
use pinocchio::{
    instruction::{AccountMeta, Instruction},
    program::invoke_unchecked,
};

pub fn evaluate(
    raw_instruction: &RawInstruction,
    task_state: &mut TaskState<'_>,
) -> Result<(), BallistaError> {
    debug_msg!("evaluate task accounts");

    let accounts = match &raw_instruction.accounts {
        TaskAccounts::Evaluated { start, length } => {
            let start_value = evaluate_expression(start, task_state)?;
            let length_value = evaluate_expression(length, task_state)?;

            let index = match start_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            let length = match length_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            if index > task_state.all_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            if index + length > task_state.all_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            &task_state.all_accounts[index..index + length]
        }
        TaskAccounts::FromInput { start, length } => {
            if *start as usize > task_state.all_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            if (*start as usize) + (*length as usize) > task_state.all_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            &task_state.all_accounts[*start as usize..*length as usize]
        }
    };

    for account in accounts {
        task_state.account_meta_cache.push(AccountMeta {
            pubkey: account.key(),
            is_signer: account.is_signer(),
            is_writable: account.is_writable(),
        });
        task_state.account_info_cache.push(account.into());
    }

    let program_id = evaluate_program_task_account(&raw_instruction.program, task_state)?;
    let data_value = evaluate_expression(&raw_instruction.data, task_state)?;
    let data: &Vec<u8> = if let Value::Bytes(bytes) = data_value.as_ref() {
        bytes
    } else {
        return Err(BallistaError::InvalidInstructionData);
    };

    let instruction = Instruction {
        program_id: program_id.key(),
        accounts: task_state.account_meta_cache.as_slice(),
        data: data.as_slice(),
    };

    debug_msg!("invoking instruction");
    unsafe {
        invoke_unchecked(&instruction, task_state.account_info_cache.as_slice());
    }

    // // For clippy
    #[cfg(not(target_os = "solana"))]
    core::hint::black_box(&(&instruction, &task_state.account_info_cache));

    debug_msg!("finished invoking instruction");

    Ok(())
}
