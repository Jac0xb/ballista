use ballista_common::{
    accounts::task_definition::TaskDefinition, types::logical_components::Value,
};
use borsh::{BorshDeserialize, BorshSerialize};
use shank::ShankInstruction;

#[derive(BorshSerialize, BorshDeserialize, ShankInstruction)]
#[rustfmt::skip]
pub(crate) enum BallistaInstruction {
    #[account(0, name = "payer", desc = "Payer account", signer, writable)]
    #[account(1, name = "task_definition", desc = "Task definition account", writable)]
    #[account(2, name = "system_program", desc = "System program")]
    CreateTask {
        task: TaskDefinition,
        task_id: u16,
    },

    #[account(0, name = "payer", desc = "Payer account", signer, writable)]
    #[account(1, name = "task_definition", desc = "Task definition account")]
    ExecuteTask {
        input_values: Vec<Value>,
    },

    #[account(0, name = "payer", desc = "Payer account", signer, writable)]
    #[account(1, name = "task_definition", desc = "Task definition account")]
    ExecuteTaskNoInputs {},
}
