use ballista_common::{
    accounts::task_definition::{ExecutionSettings, TaskDefinition},
    types::{
        logical_components::{ArithmeticBehavior, Condition, Expression, Validation, Value},
        task::{
            command::{
                associated_token_program_instruction::AssociatedTokenProgramInstruction,
                token_program_instruction::{TokenProgramInstruction, TokenProgramVersion},
                Command,
            },
            task_account::TaskAccount,
        },
    },
};
use ballista_sdk::generated::instructions::{ExecuteTask, ExecuteTaskInstructionArgs};
use num_derive::ToPrimitive;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use spl_associated_token_account::get_associated_token_address;

use strum::EnumCount;
use strum_macros::EnumIter;

use crate::utils::ballista::definitions::{
    instruction_schema::{build_remaining_accounts, InstructionSchema},
    utils::actions_for_loop,
};

#[derive(Eq, PartialEq, Debug, Copy, Clone, EnumIter, EnumCount, ToPrimitive)]
#[repr(u8)]
pub enum BatchTransferInstructionAccount {
    SystemProgram = 0,
    TokenProgram = 1,
    AssociatedTokenProgram = 2,
    From = 3,
    Mint = 4,
    FromTokenAccount = 5,
}

impl InstructionSchema for BatchTransferInstructionAccount {
    type InstructionAccount = Self;
    type InstructionAccountParams = AccountMetaParams;

    fn get_pubkey(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> Pubkey {
        match account {
            BatchTransferInstructionAccount::SystemProgram => system_program::ID,
            BatchTransferInstructionAccount::TokenProgram => spl_token::ID,
            BatchTransferInstructionAccount::AssociatedTokenProgram => {
                spl_associated_token_account::ID
            }
            BatchTransferInstructionAccount::From => params.user,
            BatchTransferInstructionAccount::Mint => params.mint,
            BatchTransferInstructionAccount::FromTokenAccount => {
                get_associated_token_address(&params.user, &params.mint)
            }
        }
    }

    fn is_signer(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        *account == BatchTransferInstructionAccount::From
    }

    fn is_writable(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(
            account,
            BatchTransferInstructionAccount::FromTokenAccount
                | BatchTransferInstructionAccount::From
        )
    }

    fn get_payer(params: &Self::InstructionAccountParams) -> Pubkey {
        params.user
    }
}

pub struct AccountMetaParams {
    pub user: Pubkey,
    pub mint: Pubkey,
}

pub fn create_batch_token_transfer_def(
    transfer_amount: u64,
    loop_length: Expression,
) -> TaskDefinition {
    let from_input = |a: BatchTransferInstructionAccount| TaskAccount::FromInput(a as u8);
    let system_program_id = from_input(BatchTransferInstructionAccount::SystemProgram);
    let token_program_id = from_input(BatchTransferInstructionAccount::TokenProgram);
    let from = from_input(BatchTransferInstructionAccount::From);
    let mint = from_input(BatchTransferInstructionAccount::Mint);
    let from_token_account = from_input(BatchTransferInstructionAccount::FromTokenAccount);

    let cache_offset = |offset: u8| {
        Expression::cached_value(0)
            .checked_add(&Value::U8(BatchTransferInstructionAccount::COUNT as u8 + offset).expr())
    };

    let current_destination_owner = TaskAccount::Evaluated(cache_offset(0));
    let current_destination_token_account = TaskAccount::Evaluated(cache_offset(1));

    TaskDefinition {
        execution_settings: ExecutionSettings::default(),
        account_groups: vec![],
        actions: actions_for_loop(
            vec![
                Command::Conditional {
                    condition: Condition::Validation(Validation::IsEmpty(
                        current_destination_token_account.clone(),
                    )),
                    true_action: Box::new(Command::InvokeAssociatedTokenProgram(
                        AssociatedTokenProgramInstruction::InitializeAccount {
                            system_program_id,
                            token_program_id,
                            payer: from.clone(),
                            mint,
                            owner: current_destination_owner,
                            token_account: current_destination_token_account.clone(),
                        },
                    )),
                },
                Command::InvokeTokenProgram(TokenProgramInstruction::Transfer {
                    program_version: TokenProgramVersion::Legacy,
                    from,
                    from_token_account,
                    to_token_account: current_destination_token_account.clone(),
                    amount: Expression::StaticValue(0),
                    multisig: None,
                }),
                Command::Log(Expression::CachedValue(0).divide(
                    Expression::literal(Value::U8(2)),
                    ArithmeticBehavior::Checked,
                )),
            ],
            &loop_length.checked_multiply(Expression::Literal(Value::U8(2))),
            2,
        ),
        shared_values: vec![Value::U64(transfer_amount)],
    }
}

pub fn execute_batch_token_transfer_tx(
    task_definition: Pubkey,
    user: &Pubkey,
    mint: &Pubkey,
    destinations: &[Pubkey],
) -> Instruction {
    let mut remaining_accounts =
        build_remaining_accounts::<BatchTransferInstructionAccount>(&AccountMetaParams {
            user: *user,
            mint: *mint,
        });

    for destination in destinations {
        remaining_accounts.push(AccountMeta::new_readonly(*destination, false));
        remaining_accounts.push(AccountMeta::new(
            get_associated_token_address(destination, mint),
            false,
        ));
    }

    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        &ExecuteTask {
            task_definition,
            payer: *user,
        },
        ExecuteTaskInstructionArgs {
            input_values: vec![Value::U8(destinations.len() as u8)],
        },
        &remaining_accounts,
    )
}
