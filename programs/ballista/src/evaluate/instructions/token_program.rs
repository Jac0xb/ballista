use std::ptr;

use crate::{
    error::BallistaError,
    evaluate::{evaluate_expression, evaluate_task_account},
    task_state::TaskState,
};
use ballista_common::{
    logical_components::Value,
    task::action::token_program_instruction::{TokenProgramInstruction, TokenProgramVersion},
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
    sysvar,
};
use spl_token::instruction::TokenInstruction;

pub fn evaluate(
    token_program_instruction: &TokenProgramInstruction,
    task_state: &mut TaskState,
) -> Result<(), BallistaError> {
    let instruction: Instruction = match token_program_instruction {
        TokenProgramInstruction::Transfer {
            program_version,
            from,
            from_token_account,
            to_token_account,
            multisig,
            amount,
        } => {
            let from_account = evaluate_task_account(from, task_state)?;
            let from_token_account = evaluate_task_account(from_token_account, task_state)?;
            let to_token_account = evaluate_task_account(to_token_account, task_state)?;

            let amount = evaluate_expression(amount, task_state)?;
            let amount = match amount.as_ref() {
                Value::U64(value) => *value,
                _ => return Err(BallistaError::InvalidCast),
            };

            task_state.account_info_cache.extend_from_slice(&[
                from_account.to_owned(),
                from_token_account.to_owned(),
                to_token_account.to_owned(),
            ]);

            let program_id = match program_version {
                TokenProgramVersion::Legacy => &spl_token::ID,
                TokenProgramVersion::Token2022 => &spl_token_2022::ID,
            };

            if **from_token_account.try_borrow_lamports().unwrap() == 0 {
                return Err(BallistaError::AccountEmpty);
            }

            if **to_token_account.try_borrow_lamports().unwrap() == 0 {
                return Err(BallistaError::AccountEmpty);
            }

            // check_program_account(program_id)
            //     .map_err(|e| format!("Wrong token program for transferring: {}", e))?;
            let data = TokenInstruction::Transfer { amount }.pack();

            task_state.account_meta_cache.extend_from_slice(&[
                // source account
                AccountMeta {
                    pubkey: *from_token_account.key,
                    is_signer: false,
                    is_writable: true,
                },
                // destination account
                AccountMeta {
                    pubkey: *to_token_account.key,
                    is_signer: false,
                    is_writable: true,
                },
                // authority account
                AccountMeta {
                    pubkey: *from_account.key,
                    is_signer: multisig.as_ref().map(|m| m.is_empty()).unwrap_or(true),
                    is_writable: false,
                },
            ]);

            if let Some(multisig) = multisig {
                for account in multisig {
                    let account = evaluate_task_account(account, task_state)?;
                    task_state.account_info_cache.push(account.to_owned());
                    task_state.account_meta_cache.push(AccountMeta {
                        pubkey: *account.key,
                        is_signer: true,
                        is_writable: false,
                    });
                    task_state.account_info_cache.push(from_account.to_owned());
                }
            }

            Instruction {
                program_id: *program_id,
                accounts: task_state.account_meta_cache.to_vec(),
                data,
            }
        }
        TokenProgramInstruction::InitializeAccount {
            program_version,
            account,
            owner,
            mint,
        } => {
            let account = evaluate_task_account(account, task_state)?;
            let owner = evaluate_task_account(owner, task_state)?;
            let mint = evaluate_task_account(mint, task_state)?;

            let program_id = match program_version {
                TokenProgramVersion::Legacy => &spl_token::ID,
                TokenProgramVersion::Token2022 => &spl_token_2022::ID,
            };

            // check_program_account(program_id)
            //     .map_err(|e| format!("Wrong token program for initializing account: {}", e))?;

            let data = TokenInstruction::InitializeAccount.pack();

            task_state.account_meta_cache.extend_from_slice(&[
                AccountMeta::new(*account.key, false),
                AccountMeta::new_readonly(*mint.key, false),
                AccountMeta::new_readonly(*owner.key, false),
                AccountMeta::new_readonly(sysvar::rent::id(), false),
            ]);

            Instruction {
                program_id: *program_id,
                accounts: task_state.account_meta_cache.to_vec(),
                data,
            }

            // spl_token::instruction::initialize_account(program_id, account.key, mint.key, owner.key)
            //     .map_err(|e| format!("Error executing token program initialize account: {}", e))?
        }
    };

    invoke(&instruction, task_state.account_info_cache.as_slice()).unwrap();

    Ok(())
}
