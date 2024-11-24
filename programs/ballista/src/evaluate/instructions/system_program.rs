use crate::{
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_task_account},
    invoke_as_signer,
};
use ballista_common::types::execution_state::ExecutionState;
use ballista_common::{
    types::logical_components::Value,
    types::task::action::system_instruction::SystemInstructionAction,
};
use pinocchio::{instruction::AccountMeta, pubkey::find_program_address};
use pinocchio_system::instructions::{CreateAccount, Transfer};

pub fn evaluate(
    system_instruction: &SystemInstructionAction,
    execution_state: &mut ExecutionState,
) -> Result<(), BallistaError> {
    match system_instruction {
        SystemInstructionAction::Transfer { from, to, amount } => {
            let (from_account, seed) = evaluate_task_account(from, execution_state)?;
            let (to_account, _) = evaluate_task_account(to, execution_state)?;

            let amount = evaluate_expression(amount, execution_state)?;
            let amount = match amount.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            execution_state
                .account_info_cache
                .extend_from_slice(&[from_account.into(), to_account.into()]);

            execution_state.account_meta_cache.extend_from_slice(&[
                AccountMeta {
                    pubkey: from_account.key(),
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: to_account.key(),
                    is_signer: false,
                    is_writable: true,
                },
            ]);

            let ix = Transfer {
                from: from_account,
                to: to_account,
                lamports: amount,
            };

            if let Some(seed_index) = seed {
                invoke_as_signer!(execution_state.payer, from_account, seed_index, ix);
            } else {
                ix.invoke().unwrap();
            }

            Ok(())
        }
        SystemInstructionAction::CreateAccount {
            payer,
            account,
            program_owner,
            space,
            lamports,
        } => {
            let (payer, _) = evaluate_task_account(payer, execution_state)?;
            let (account, _) = evaluate_task_account(account, execution_state)?;
            let (owner, _) = evaluate_task_account(program_owner, execution_state)?;

            let space = evaluate_expression(space, execution_state)?;
            let space = match space.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            let lamports = evaluate_expression(lamports, execution_state)?;
            let lamports = match lamports.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            CreateAccount {
                from: payer,
                to: account,
                lamports,
                space,
                owner: owner.key(),
            }
            .invoke()
            .unwrap();

            Ok(())
        }
        _ => panic!("System instruction not implemented"),
    }
}
