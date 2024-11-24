use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_program_task_account},
};
use ballista_common::types::execution_state::ExecutionState;
use ballista_common::types::{
    logical_components::Value,
    task::{action::raw_instruction::RawInstruction, task_account::TaskAccounts},
};
use pinocchio::{
    instruction::{AccountMeta, Instruction},
    program::invoke_unchecked,
};

pub fn evaluate(
    raw_instruction: &RawInstruction,
    execution_state: &mut ExecutionState<'_>,
) -> Result<(), BallistaError> {
    debug_msg!("evaluate task accounts");

    let accounts = match &raw_instruction.accounts {
        TaskAccounts::Evaluated { start, length } => {
            let start_value = evaluate_expression(start, execution_state)?;
            let length_value = evaluate_expression(length, execution_state)?;

            let index = match start_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            let length = match length_value.as_ref() {
                Value::U8(value) => *value as usize,
                _ => return Err(BallistaError::InvalidCast),
            };

            if index > execution_state.input_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            if index + length > execution_state.input_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            &execution_state.input_accounts[index..index + length]
        }
        TaskAccounts::FromInput { start, length } => {
            if *start as usize > execution_state.input_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            if (*start as usize) + (*length as usize) > execution_state.input_accounts.len() {
                return Err(BallistaError::InvalidAccountRange);
            }

            &execution_state.input_accounts[*start as usize..*length as usize]
        }
    };

    for account in accounts {
        execution_state.account_meta_cache.push(AccountMeta {
            pubkey: account.key(),
            is_signer: account.is_signer(),
            is_writable: account.is_writable(),
        });
        execution_state.account_info_cache.push(account.into());
    }

    let program_id = evaluate_program_task_account(&raw_instruction.program, execution_state)?;
    let data_value = evaluate_expression(&raw_instruction.data, execution_state)?;
    let data: &Vec<u8> = if let Value::Bytes(bytes) = data_value.as_ref() {
        bytes
    } else {
        return Err(BallistaError::InvalidInstructionData);
    };

    let instruction = Instruction {
        program_id: program_id.key(),
        accounts: execution_state.account_meta_cache.as_slice(),
        data: data.as_slice(),
    };

    debug_msg!("invoking instruction");
    unsafe {
        invoke_unchecked(&instruction, execution_state.account_info_cache.as_slice());
    }

    // // For clippy
    #[cfg(not(target_os = "solana"))]
    core::hint::black_box(&(&instruction, &execution_state.account_info_cache));

    debug_msg!("finished invoking instruction");

    Ok(())
}
