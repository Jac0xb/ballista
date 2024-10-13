use anchor_lang::prelude::AccountMeta;
use anchor_lang::system_program;
use ballista_common::logical_components::{ArithmeticBehavior, Condition, Expression, Value};
use ballista_common::schema::{ExecutionSettings, TaskDefinition, Validation};
use ballista_common::task::action::associated_token_program_instruction::AssociatedTokenProgramInstruction;
use ballista_common::task::action::set_cache::SetCacheType;
use ballista_common::task::action::token_program_instruction::{
    TokenProgramInstruction, TokenProgramVersion,
};
use ballista_common::task::shared::TaskAccount;
use ballista_common::task::task_action::TaskAction;
use ballista_sdk::generated::instructions::{ExecuteTask, ExecuteTaskInstructionArgs};
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::EncodableKeypair;
use spl_associated_token_account::get_associated_token_address;

use crate::utils::ballista::definitions::utils::actions_for_loop;

pub fn create_batch_token_transfer_def(
    transfer_amount: u64,
    loop_length: Expression,
) -> TaskDefinition {
    let cache_offset =
        |offset: u8| Expression::cached_value(0).checked_add(&Value::U8(6 + offset).expr());

    TaskDefinition {
        execution_settings: ExecutionSettings::default(),
        account_groups: vec![],
        actions: actions_for_loop(
            // vec![
            //     TaskAction::SetCache(SetCacheType::Expression {
            //         index: 0,
            //     expression: Value::U8(0).expr(),
            // }),
            // TaskAction::loop_action(
            //     Condition::less_than(
            //         Expression::cached_value(0),
            //         Expression::InputValue(0).checked_multiply(Value::U8(2).expr()),
            //     ),
            vec![
                TaskAction::Conditional {
                    condition: Condition::Validation(Validation::IsEmpty(TaskAccount::Evaluated(
                        cache_offset(1),
                    ))),
                    true_action: Box::new(TaskAction::AssociatedTokenProgramInstruction(
                        AssociatedTokenProgramInstruction::InitializeAccount {
                            system_program_id: TaskAccount::FromInput(0),
                            token_program_id: TaskAccount::FromInput(1),
                            payer: TaskAccount::FromInput(3),
                            mint: TaskAccount::FromInput(5),
                            owner: TaskAccount::Evaluated(cache_offset(0)),
                            token_account: TaskAccount::Evaluated(cache_offset(1)),
                        },
                    )),
                },
                TaskAction::TokenProgramInstruction(TokenProgramInstruction::Transfer {
                    program_version: TokenProgramVersion::Legacy,
                    from: TaskAccount::FromInput(3),
                    from_token_account: TaskAccount::FromInput(4),
                    to_token_account: TaskAccount::Evaluated(cache_offset(1)),
                    amount: Expression::StaticValue(0),
                    multisig: None,
                }),
                TaskAction::Log(Expression::CachedValue(0).divide(
                    Expression::literal(Value::U8(2)),
                    ArithmeticBehavior::Checked,
                )),
                TaskAction::SetCache(SetCacheType::Expression {
                    index: 0,
                    expression: Expression::cached_value(0).checked_add(&Value::U8(2).expr()),
                }),
            ],
            &loop_length,
            2,
        ),
        shared_values: vec![Value::U64(transfer_amount)],
    }
}

pub fn create_token_transfer(
    task: Pubkey,
    user: &Keypair,
    user_ata: Pubkey,
    mint: &Pubkey,
    destinations: &[Pubkey],
) -> Instruction {
    println!("Payer {:?}", user.encodable_pubkey());
    println!("User ATA {:?}", user_ata);
    println!("Mint {:?}", mint);

    let mut remaining_accounts = vec![
        AccountMeta::new_readonly(system_program::ID, false), // 0
        AccountMeta::new_readonly(spl_token::ID, false),      // 1
        AccountMeta::new_readonly(spl_associated_token_account::ID, false), // 2
        AccountMeta::new(user.encodable_pubkey(), true),      // 3
        AccountMeta::new(user_ata, false),                    // 4
        AccountMeta::new_readonly(*mint, false),              // 5
    ];

    for destination in destinations {
        println!("Destination {:?}", *destination);
        println!(
            "Associated token address {:?}",
            get_associated_token_address(destination, mint)
        );

        remaining_accounts.push(AccountMeta::new(*destination, false));
        remaining_accounts.push(AccountMeta::new(
            get_associated_token_address(destination, mint),
            false,
        ));
    }

    ballista_sdk::generated::instructions::ExecuteTask::instruction_with_remaining_accounts(
        &ExecuteTask {
            task,
            payer: user.encodable_pubkey(),
        },
        ExecuteTaskInstructionArgs {
            task_values: vec![Value::U8(destinations.len() as u8)],
        },
        &remaining_accounts,
    )
}

pub fn get_associated_token_address_and_bump_seed_internal(
    wallet_address: &Pubkey,
    token_mint_address: &Pubkey,
    program_id: &Pubkey,
    token_program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            &wallet_address.to_bytes(),
            &token_program_id.to_bytes(),
            &token_mint_address.to_bytes(),
        ],
        program_id,
    )
}
