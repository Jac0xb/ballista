use crate::{
    debug_msg,
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_task_account},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value,
    task::action::token_program_instruction::{TokenProgramInstruction, TokenProgramVersion},
};
use pinocchio_token::instructions::{InitilizeAccount3, Transfer};

pub fn evaluate(
    token_program_instruction: &TokenProgramInstruction,
    task_state: &mut TaskState,
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
            let (from_account, _) = evaluate_task_account(from, task_state)?;
            let (from_token_account, _) = evaluate_task_account(from_token_account, task_state)?;
            let (to_token_account, _) = evaluate_task_account(to_token_account, task_state)?;

            let amount = evaluate_expression(amount, task_state)?;
            let amount = match amount.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            let program_id = match program_version {
                TokenProgramVersion::Legacy => &spl_token::ID,
                TokenProgramVersion::Token2022 => &spl_token_2022::ID,
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
            let (account, _) = evaluate_task_account(account, task_state)?;
            let (owner, _) = evaluate_task_account(owner, task_state)?;
            let (mint, _) = evaluate_task_account(mint, task_state)?;

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
