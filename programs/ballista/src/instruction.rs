use ballista_common::{
    logical_components::Value,
    schema::{Schema, TaskDefinition},
};
use borsh::{BorshDeserialize, BorshSerialize};
use shank::ShankInstruction;

#[derive(BorshSerialize, BorshDeserialize, ShankInstruction)]
#[rustfmt::skip]
pub(crate) enum BallistaInstruction {
    #[account(0, name = "program_id", desc = "Ballista program")]
    #[account(1, name = "system_program", desc = "System program")]
    #[account(2, name = "payer", desc = "Payer account", signer, writable)]
    #[account(3, name = "schema", desc = "Schema Account", writable)]
    CreateSchema { 
        schema: Schema,
    },
    #[account(0, name = "payer", desc = "Payer account", signer, writable)]
    #[account(1, name = "schema", desc = "Schema Account", writable)]
    #[account(2, name = "system_program", desc = "System program")]
    AddTask {
        task: TaskDefinition,
    },
    #[account(0, name = "payer", desc = "Payer account", signer, writable)]
    #[account(1, name = "schema", desc = "Schema Account", writable)]
    #[account(2, name = "system_program", desc = "System program")]
    RemoveTask {
        task_index: u8,
    },
    #[account(0, name = "payer", desc = "Payer account", signer, writable)]
    #[account(1, name = "schema", desc = "Schema Account", writable)]
    #[account(2, name = "system_program", desc = "System program")]
    SetTask {
        task_index: u8,
        task: TaskDefinition,
    },
    #[account(0, name = "schema", desc = "Schema Account")]
    ExecuteTask {
        task_values: Vec<Value>,
    }

    // #[account(0, name = "program_id", desc = "Lighthouse program")]
    // #[account(1, name = "payer", desc = "Payer account", signer, writable)]
    // #[account(2, name = "memory", desc = "Memory account", writable)]
    // MemoryClose { memory_id: u8, memory_bump: u8 },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertAccountData { log_level: LogLevel, assertion: AccountDataAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertAccountDataMulti { log_level: LogLevel, assertions: AccountDataAssertions },

    // #[account(0, name = "account_a", desc = "Account A where the delta is calculated from")]
    // #[account(1, name = "account_b", desc = "Account B where the delta is calculated to")]
    // AssertAccountDelta { log_level: LogLevel, assertion: AccountDeltaAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertAccountInfo { log_level: LogLevel, assertion: AccountInfoAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertAccountInfoMulti { log_level: LogLevel, assertions: AccountInfoAssertions },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertMintAccount { log_level: LogLevel, assertion: MintAccountAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertMintAccountMulti { log_level: LogLevel, assertions: MintAccountAssertions },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertTokenAccount { log_level: LogLevel, assertion: TokenAccountAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertTokenAccountMulti { log_level: LogLevel, assertions: TokenAccountAssertions },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertStakeAccount { log_level: LogLevel, assertion: StakeAccountAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertStakeAccountMulti { log_level: LogLevel, assertions: StakeAccountAssertions },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertUpgradeableLoaderAccount { log_level: LogLevel, assertion: UpgradeableLoaderStateAssertion },

    // #[account(0, name = "target_account", desc = "Target account to be asserted")]
    // AssertUpgradeableLoaderAccountMulti { log_level: LogLevel, assertions: UpgradeableLoaderStateAssertions },

    // // No accounts
    // AssertSysvarClock { log_level : LogLevel, assertion: SysvarClockAssertion },

    // #[account(0, name = "target_merkle_tree", desc = "Target merkle tree account to be asserted")]
    // #[account(1, name = "root", desc = "The current root of the merkle tree")]
    // #[account(2, name = "spl_account_compression", desc = "SPL account compression program")]
    // AssertMerkleTreeAccount { log_level: LogLevel, assertion: MerkleTreeAssertion },


    // #[account(0, name = "target_account", desc = "Target mpl-bubblegum tree config account to be asserted")]
    // AssertBubblegumTreeConfigAccount { log_level: LogLevel, assertion: BubblegumTreeConfigAssertion },
}
