use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_program_task_account, evaluate_task_account},
    task_state::TaskState,
};
use ballista_common::task::action::defined_instruction::DefinedInstruction;
use solana_program::instruction::{AccountMeta, Instruction};

pub fn evaluate(
    defined_instruction: &DefinedInstruction,
    task_state: &mut TaskState<'_, '_>,
    instruction_data_cache: &mut Vec<u8>,
) -> Result<(), BallistaError> {
    debug_msg!("evaluate task accounts");
    for task_account in defined_instruction.accounts.iter() {
        let account = evaluate_task_account(&task_account.task_account, task_state)?;
        task_state.account_meta_cache.push(AccountMeta {
            pubkey: *account.key,
            is_signer: account.is_signer,
            is_writable: account.is_writable,
        });
        task_state.account_info_cache.push(account.clone());
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
        program_id: *program_account.key,
        accounts: unsafe {
            Vec::from_raw_parts(
                task_state.account_meta_cache.as_ptr() as *mut AccountMeta,
                task_state.account_meta_cache.len(),
                task_state.account_meta_cache.capacity(),
            )
        },
        data: unsafe {
            Vec::from_raw_parts(
                instruction_data_cache.as_ptr() as *mut u8,
                instruction_data_cache.len(),
                instruction_data_cache.capacity(),
            )
        },
    };

    // log_instruction(&instruction, task_state);

    debug_msg!("invoking instruction");
    solana_invoke::invoke(&instruction, &task_state.account_info_cache).unwrap();

    // // For clippy
    #[cfg(not(target_os = "solana"))]
    core::hint::black_box(&(&instruction, &task_state.account_info_cache));

    debug_msg!("finished invoking instruction");

    Ok(())
}
