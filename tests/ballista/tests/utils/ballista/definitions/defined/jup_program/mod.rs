use anchor_lang::prelude::AccountMeta;
use ballista_common::{
    logical_components::{Expression, Value, ValueType},
    schema::{ExecutionSettings, TaskDefinition},
    task::{
        action::{
            raw_instruction::RawInstruction,
            token_program_instruction::{TokenProgramInstruction, TokenProgramVersion},
        },
        shared::{TaskAccount, TaskAccounts},
        task_action::TaskAction,
    },
};
use ballista_sdk::find_task_definition_pda;
use num_derive::ToPrimitive;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use strum::EnumIter;
use strum_macros::EnumCount as EnumCountMacro;

use crate::utils::ballista::definitions::instruction_schema::{
    execute_task_with_args_and_fn, InstructionSchema,
};

#[derive(Eq, PartialEq, Debug, Copy, Clone, EnumIter, EnumCountMacro, ToPrimitive)]
#[repr(u8)]
pub enum JupiterSwapInstructionAccount {
    FromTokenAccount = 0,
    ToTokenAccount = 1,
    JupiterProgram = 2,
}

pub struct JupiterSwapInstructionAccountParams {
    pub payer: Pubkey,
    pub from_token_account: Pubkey,
    pub to_token_account: Pubkey,
    pub jupiter_program: Pubkey,
    pub swap_ix_data: Vec<u8>,
    pub swap_jup_accounts: Vec<AccountMeta>,
}

impl InstructionSchema for JupiterSwapInstructionAccount {
    type InstructionAccount = Self;
    type InstructionAccountParams = JupiterSwapInstructionAccountParams;

    fn is_signer(
        _account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        false
    }

    fn is_writable(
        account: &Self::InstructionAccount,
        _params: &Self::InstructionAccountParams,
    ) -> bool {
        matches!(
            account,
            JupiterSwapInstructionAccount::FromTokenAccount
                | JupiterSwapInstructionAccount::ToTokenAccount
        )
    }

    fn get_pubkey(
        account: &Self::InstructionAccount,
        params: &Self::InstructionAccountParams,
    ) -> Pubkey {
        match account {
            JupiterSwapInstructionAccount::FromTokenAccount => params.from_token_account,
            JupiterSwapInstructionAccount::ToTokenAccount => params.to_token_account,
            JupiterSwapInstructionAccount::JupiterProgram => params.jupiter_program,
        }
    }

    fn get_payer(params: &Self::InstructionAccountParams) -> Pubkey {
        params.payer
    }
}

// pub struct MintBubblegumNftAccountMetaParams {
//     pub tree_authority: Pubkey,
//     pub leaf_owner: Pubkey,
//     pub leaf_delegate: Pubkey,
//     pub merkle_tree: Pubkey,
//     pub payer: Pubkey,
//     pub tree_delegate: Pubkey,
// }

pub fn create_jupiter_swap_and_transfer_task_definition() -> TaskDefinition {
    // InputValue(0) -> Jup Swap Raw Bytes
    // InputValue(1) -> Start of Jup Swap Accounts
    // InputValue(2) -> Length of Jup Swap Accounts

    // Byte offset
    let token_account_amount_offset = Expression::Literal(Value::U64(64));

    let from_token_account =
        TaskAccount::FromInput(JupiterSwapInstructionAccount::FromTokenAccount as u8);
    let to_token_account =
        TaskAccount::FromInput(JupiterSwapInstructionAccount::ToTokenAccount as u8);

    // Expression to pull the balance out of token account.
    let from_token_balance_expr = Expression::ValueFromAccountData {
        index: Box::new(from_token_account.clone()),
        offset: Box::new(token_account_amount_offset.clone()),
        value_type: ValueType::U64,
    };
    let to_token_balance_expr = Expression::ValueFromAccountData {
        index: Box::new(to_token_account.clone()),
        offset: Box::new(token_account_amount_offset.clone()),
        value_type: ValueType::U64,
    };

    let jup_swap_task = TaskDefinition {
        execution_settings: ExecutionSettings::default(),
        actions: vec![
            // Execute jupiter swap using instructions from API.
            TaskAction::RawInstruction({
                RawInstruction {
                    program: TaskAccount::FromInput(
                        JupiterSwapInstructionAccount::JupiterProgram as u8,
                    ),
                    data: Expression::InputValue(0),
                    accounts: TaskAccounts::Evaluated {
                        start: Expression::InputValue(1),
                        length: Expression::InputValue(2),
                    },
                }
            }),
            // Transfer exact amount of tokens from input token account to output token account
            TaskAction::TokenProgramInstruction(TokenProgramInstruction::Transfer {
                program_version: TokenProgramVersion::Legacy,
                multisig: None,
                from: TaskAccount::FeePayer,
                from_token_account,
                to_token_account,
                amount: from_token_balance_expr.clone(),
            }),
            // Log amount transferred + amount
            TaskAction::Log(Expression::Literal(Value::String(
                "Transferred".to_string(),
            ))),
            TaskAction::Log(to_token_balance_expr),
        ],
        shared_values: vec![],
        account_groups: vec![],
    };

    jup_swap_task
}

pub fn execute_jupiter_swap_and_transfer(
    instruction: &JupiterSwapInstructionAccountParams,
) -> Instruction {
    let task_definition = find_task_definition_pda(instruction.payer, 0).0;
    execute_task_with_args_and_fn::<JupiterSwapInstructionAccount>(
        task_definition,
        instruction,
        vec![
            Value::Bytes(instruction.swap_ix_data.clone()),
            Value::U8(3),
            Value::U8(instruction.swap_jup_accounts.len() as u8),
        ],
        instruction.swap_jup_accounts.clone(),
    )
}
