use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_task_account},
};
use ballista_common::types::execution_state::ExecutionState;
use ballista_common::types::{
    logical_components::Value,
    task::command::token_program_instruction::{TokenProgramInstruction, TokenProgramVersion},
};
use pinocchio_token::instructions::{InitilizeAccount3, Transfer};

pub fn evaluate(
    token_program_instruction: &TokenProgramInstruction,
    execution_state: &mut ExecutionState,
) -> Result<(), BallistaError> {
    match token_program_instruction {
        TokenProgramInstruction::Transfer {
            program_version,
            from,
            from_token_account,
            to_token_account,
            multisig,
            amount,
        } => {
            let (from_account, _) = evaluate_task_account(from, execution_state)?;
            let (from_token_account, _) =
                evaluate_task_account(from_token_account, execution_state)?;
            let (to_token_account, _) = evaluate_task_account(to_token_account, execution_state)?;

            let amount = evaluate_expression(amount, execution_state)?;
            let amount = match amount.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            // TODO: Need to implement support for token2022
            let program_id = match program_version {
                TokenProgramVersion::Legacy => &pinocchio_token::ID,
                TokenProgramVersion::Token2022 => &pinocchio_token::ID,
            };

            if *from_token_account.try_borrow_lamports().unwrap() == 0 {
                debug_msg!("Source token account has no lamports");
                return Err(BallistaError::AccountEmpty);
            }

            if *to_token_account.try_borrow_lamports().unwrap() == 0 {
                debug_msg!("Destination token account has no lamports");
                return Err(BallistaError::AccountEmpty);
            }

            Transfer {
                from: from_token_account,
                to: to_token_account,
                authority: from_account,
                amount,
            }
            .invoke()
            .unwrap();
        }
        TokenProgramInstruction::InitializeAccount {
            program_version,
            account,
            owner,
            mint,
        } => {
            let (account, _) = evaluate_task_account(account, execution_state)?;
            let (owner, _) = evaluate_task_account(owner, execution_state)?;
            let (mint, _) = evaluate_task_account(mint, execution_state)?;

            InitilizeAccount3 {
                token: account,
                mint,
                owner: owner.key(),
            }
            .invoke()
            .unwrap();
        }
    };

    Ok(())
}
