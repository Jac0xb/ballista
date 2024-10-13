use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_program_task_account, evaluate_task_account},
    task_state::TaskState,
};
use ballista_common::task::action::defined_instruction::DefinedInstruction;
use pinocchio::{
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_unchecked},
};

pub fn evaluate(
    defined_instruction: &DefinedInstruction,
    task_state: &mut TaskState<'_>,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    debug_msg!("evaluate task accounts");
    for task_account in defined_instruction.accounts.iter() {
        let account = evaluate_task_account(&task_account.task_account, task_state)?;
        task_state.account_meta_cache.push(AccountMeta {
            pubkey: account.key(),
            is_signer: account.is_signer(),
            is_writable: account.is_writable(),
        });
        task_state.account_info_cache.push(account.into());
    }

    debug_msg!("evaluate arguments");
    for schema_arg in defined_instruction.arguments.iter() {
        let value = evaluate_expression(&schema_arg.value, task_state).unwrap();
        // .into_owned(); // CLONING IS CHEATING

        debug_msg!("evaluated value");

        // let value = value.into_owned();

        value.as_bytes(
            defined_instruction.serialization_type,
            instruction_data_cache,
        );
    }

    // let infos = task_state.account_info_cache.to_vec();
    // TODO: this is not entire correct
    // for account in infos.iter() {}

    // let infos = task_state.account_info_cache.to_vec();

    debug_msg!("evaluating program account");
    let program_account = evaluate_program_task_account(&defined_instruction.program, task_state)?;
    let instruction = Instruction {
        program_id: program_account.key(),
        accounts: task_state.account_meta_cache.as_slice(),
        data: instruction_data_cache.as_slice(),
    };

    // log_instruction(&instruction, task_state);

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
