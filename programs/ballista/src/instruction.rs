use ballista_common::{logical_components::Value, schema::TaskDefinition};
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

    #[account(0, name = "task", desc = "Task Account")]
    #[account(1, name = "payer", desc = "Payer account", signer, writable)]
    ExecuteTask {
        task_values: Vec<Value>,
    }
}
