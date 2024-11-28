use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_program_task_account, evaluate_task_account},
    invoke_with_seeds,
};

use ballista_common::types::execution_state::ExecutionState;
use ballista_common::types::task::command::defined_instruction::DefinedInstruction;
use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program::invoke_unchecked,
};

pub fn evaluate(
    defined_instruction: &DefinedInstruction,
    execution_state: &mut ExecutionState<'_>,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    debug_msg!("evaluate task accounts");

    let mut user_pda: Option<(&AccountInfo, u32)> = None;

    for task_account in defined_instruction.accounts.iter() {
        let (account, seeds) = evaluate_task_account(&task_account.task_account, execution_state)?;
        execution_state.account_meta_cache.push(AccountMeta {
            pubkey: account.key(),
            is_signer: task_account.signer,
            is_writable: task_account.writable,
        });
        execution_state.account_info_cache.push(account.into());

        if let Some(seed) = seeds {
            if let Some((curr_pda, _)) = user_pda {
                if curr_pda.key() != account.key() {
                    panic!("Multiple seeds are not supported");
                }

                continue;
            }

            user_pda = Some((account, seed));
        }
    }

    debug_msg!("evaluate arguments");
    for schema_arg in defined_instruction.arguments.iter() {
        let value = evaluate_expression(&schema_arg.value, execution_state).unwrap();

        value.as_bytes(
            defined_instruction.serialization_type,
            instruction_data_cache,
        );
    }

    debug_msg!("evaluating program account");
    let program_account =
        evaluate_program_task_account(&defined_instruction.program, execution_state)?;
    let instruction = Instruction {
        program_id: program_account.key(),
        accounts: execution_state.account_meta_cache.as_slice(),
        data: instruction_data_cache.as_slice(),
    };

    if let Some((user_pda, user_pda_index)) = user_pda {
        invoke_with_seeds!(
            &instruction,
            execution_state.account_info_cache.as_slice(),
            execution_state.payer,
            user_pda,
            user_pda_index
        );
    } else {
        debug_msg!("invoking instruction");
        unsafe {
            invoke_unchecked(&instruction, execution_state.account_info_cache.as_slice());
        }
    }

    #[cfg(not(target_os = "solana"))]
    core::hint::black_box(&(&instruction, &execution_state.account_info_cache));

    debug_msg!("finished invoking instruction");

    Ok(())
}
