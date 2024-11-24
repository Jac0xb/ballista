#[macro_export]
macro_rules! debug_msg {
    ($($arg:tt)*) => {
        if true {
            pinocchio::msg!($($arg)*)
        }
    };
}

// pub fn log_instruction(instruction: &Instruction, execution_state: &ExecutionState) {
//     msg!("Instruction {:?}", instruction.data);

//     msg!(
//         "{:<4} {:<44} {:<44} {:<10} {:<10} {:<10}",
//         "IxId",
//         "Key",
//         "Owner",
//         "Writable",
//         "Signer",
//         "Invoked"
//     );

//     for (i, account) in instruction.accounts.iter().enumerate() {
//         let account_info = execution_state
//             .input_accounts
//             .iter()
//             .find(|a| a.key() == account.pubkey)
//             .unwrap();

//         let is_writable = account.is_writable;
//         let is_signer = account.is_signer;
//         let is_invoked = account_info.executable();

//         msg!(
//             "[{:<2}] {:<44} {:<44} {:<10} {:<10} {:<10}",
//             i,
//             bs58::encode(account.pubkey).into_string(),
//             bs58::encode(account_info.owner()).into_string(),
//             is_writable,
//             is_signer,
//             is_invoked
//         );
//     }
// }
