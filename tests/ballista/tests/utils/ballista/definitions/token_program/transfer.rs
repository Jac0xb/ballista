use ballista_common::logical_components::{Condition, Expression, Value};
use ballista_common::schema::{ExecutionSettings, TaskDefinition, Validation};
use ballista_common::task::action::associated_token_program_instruction::AssociatedTokenProgramInstruction;
use ballista_common::task::action::set_cache::SetCacheType;
use ballista_common::task::action::token_program_instruction::{
    TokenProgramInstruction, TokenProgramVersion,
};
use ballista_common::task::shared::TaskAccount;
use ballista_common::task::task_action::TaskAction;

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
                            token_program_id: TaskAccount::FromInput(1),
                            system_program_id: TaskAccount::FromInput(0),
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
                TaskAction::SetCache(SetCacheType::Expression {
                    index: 0,
                    expression: Expression::cached_value(0).checked_add(&Value::U8(2).expr()),
                }),
            ],
            &loop_length,
        ),
        shared_values: vec![Value::U64(transfer_amount)],
    }
}
