use ballista_common::{
    accounts::task_definition::{ExecutionSettings, TaskDefinition},
    types::{
        logical_components::{ArithmeticBehavior, Expression, Value},
        task::{
            command::{
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
pub enum BatchTransferExistingInstructionAccount {
    TokenProgram = 0,
    From = 1,
    FromTokenAccount = 2,
}

impl InstructionSchema for BatchTransferExistingInstructionAccount {
    type InstructionAccount = Self;
    type InstructionAccountParams = AccountMetaParams;

    fn get_pubkey(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> Pubkey {
        match account {
            BatchTransferExistingInstructionAccount::TokenProgram => spl_token::ID,
            BatchTransferExistingInstructionAccount::From => params.user,
            BatchTransferExistingInstructionAccount::FromTokenAccount => {
                get_associated_token_address(&params.user, &params.mint)
            }
        }
    }

    fn is_signer(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        *account == BatchTransferExistingInstructionAccount::From
    }

    fn is_writable(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(
            account,
            BatchTransferExistingInstructionAccount::FromTokenAccount
                | BatchTransferExistingInstructionAccount::From
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

pub fn create_batch_token_transfer_existing_def(
    transfer_amount: u64,
    loop_length: Expression,
) -> TaskDefinition {
    let from_input = |a: BatchTransferExistingInstructionAccount| TaskAccount::FromInput(a as u8);
    let from = from_input(BatchTransferExistingInstructionAccount::From);
    let from_token_account = from_input(BatchTransferExistingInstructionAccount::FromTokenAccount);

    let cache_offset = |offset: u8| {
        Expression::cached_value(0).checked_add(
            &Value::U8(BatchTransferExistingInstructionAccount::COUNT as u8 + offset).expr(),
        )
    };

    let current_destination_token_account = TaskAccount::Evaluated(cache_offset(0));

    TaskDefinition {
        execution_settings: ExecutionSettings::default(),
        account_groups: vec![],
        actions: actions_for_loop(
            vec![
                Command::InvokeTokenProgram(TokenProgramInstruction::Transfer {
                    program_version: TokenProgramVersion::Legacy,
                    from,
                    from_token_account,
                    to_token_account: current_destination_token_account.clone(),
                    amount: Expression::StaticValue(0),
                    multisig: None,
                }),
                Command::Log(Expression::CachedValue(0)),
            ],
            &loop_length,
            1,
        ),
        shared_values: vec![Value::U64(transfer_amount)],
    }
}

pub fn execute_batch_token_transfer_existing_tx(
    task_definition: Pubkey,
    user: &Pubkey,
    mint: &Pubkey,
    destinations: &[Pubkey],
) -> Instruction {
    let mut remaining_accounts =
        build_remaining_accounts::<BatchTransferExistingInstructionAccount>(&AccountMetaParams {
            user: *user,
            mint: *mint,
        });

    for destination in destinations {
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
