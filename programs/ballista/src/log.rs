// log macro

// pub static LOG_ENABLED: bool = false;

use solana_program::{instruction::Instruction, msg};

use crate::task_state::TaskState;

#[macro_export]
macro_rules! debug_msg {
    ($($arg:tt)*) => {
        if false {
            solana_program::msg!($($arg)*)
        }
    };
}

pub fn log_instruction(instruction: &Instruction, task_state: &TaskState) {
    msg!("Instruction {:?}", instruction.data);

    msg!(
        "{:<4} {:<44} {:<44} {:<10} {:<10} {:<10}",
        "IxId",
        "Key",
        "Owner",
        "Writable",
        "Signer",
        "Invoked"
    );

    for (i, account) in instruction.accounts.iter().enumerate() {
        let account_info = task_state.account_info_cache.get(i).unwrap();

        let is_writable = account.is_writable;
        let is_signer = account.is_signer;
        let is_invoked = account_info.executable;

        msg!(
            "[{:<2}] {:<44} {:<44} {:<10} {:<10} {:<10}",
            i,
            account.pubkey.to_string(),
            account_info.owner.to_string(),
            is_writable,
            is_signer,
            is_invoked
        );
    }
}
