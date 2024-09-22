use crate::{
    evaluate::{evaluate_expression, evaluate_program_task_account, evaluate_task_accounts},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value,
    schema::{instruction::ArgumentDefinition, Schema},
    task::action::schema_instruction::{SchemaInstruction, TaskArgument},
};
use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
};

pub fn evaluate<'info>(
    schema: &Schema,
    schema_instruction: &SchemaInstruction,
    task_state: &TaskState<'info, '_>,
) -> Result<(), String> {
    let program_account = evaluate_program_task_account(&schema_instruction.program, task_state)?;

    let mut instruction_accounts = vec![];
    for task_account in schema_instruction.accounts.iter() {
        evaluate_task_accounts(task_account, task_state, &mut instruction_accounts)?;
    }

    let mut task_arguments: Vec<Value> = vec![];
    for arg in schema_instruction.arguments.iter() {
        let input_value = match arg {
            TaskArgument::Expression(expression) => evaluate_expression(expression, task_state)?,
            TaskArgument::Literal(value) => value.clone(),
        };

        task_arguments.push(input_value);
    }

    let instruction_schema = schema
        .instructions
        .get(schema_instruction.instruction_id as usize)
        .ok_or_else(|| {
            format!(
                "Instruction at index {} not found",
                schema_instruction.instruction_id
            )
        })?;

    let mut instruction_arguments: Vec<&Value> = vec![];
    let mut input_index = 0;
    for schema_arg in instruction_schema.arguments.iter() {
        match schema_arg {
            ArgumentDefinition::Constant { value, .. } => {
                instruction_arguments.push(value);
            }
            ArgumentDefinition::Input { value_type } => {
                // TODO: NEED TO ADD TYPE CHECKING
                // if let Some(value_type) = value_type {
                //     if task_arguments[input_index].value_type() != value_type {
                //         return Err(format!(
                //             "Argument at index {} has wrong type, expected {}",
                //             input_index, value_type
                //         ));
                //     }
                // }

                let value = task_arguments.get(input_index).unwrap_or_else(|| {
                    panic!("Argument at index {} not found", input_index);
                });

                instruction_arguments.push(value);
                input_index += 1;
            }
        }
    }

    let instruction_account_metas = instruction_accounts
        .iter()
        .map(|account: &&AccountInfo<'info>| AccountMeta {
            pubkey: *account.key,
            is_signer: account.is_signer,
            is_writable: account.is_writable,
        })
        .collect::<Vec<_>>();

    let serialized_instruction_arguments = instruction_arguments
        .iter()
        .flat_map(|arg| arg.as_bytes(instruction_schema.serialization))
        .collect();

    let infos: Vec<AccountInfo<'info>> = instruction_accounts
        .iter()
        .map(|account| account.to_owned().clone())
        .collect();

    let instruction = Instruction {
        program_id: *program_account.key,
        accounts: instruction_account_metas,
        data: serialized_instruction_arguments,
    };

    solana_invoke::invoke(&instruction, &infos).unwrap();

    // // For clippy
    #[cfg(not(target_os = "solana"))]
    core::hint::black_box(&(&instruction, &infos));

    Ok(())
}
