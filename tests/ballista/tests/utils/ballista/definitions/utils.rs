use ballista_common::types::{
    logical_components::{Condition, Expression, Value},
    task::{command::set_cache::SetCacheType, command::Command},
};

pub fn actions_for_loop(
    mut actions: Vec<Command>,
    length: &Expression,
    increment_by: u8,
) -> Vec<Command> {
    actions.push(Command::SetCache(SetCacheType::Expression {
        index: 0,
        expression: Expression::cached_value(0).checked_add(&Value::U8(increment_by).expr()),
    }));

    vec![
        Command::SetCache(SetCacheType::Expression {
            index: 0,
            expression: Value::U8(0).expr(),
        }),
        Command::loop_action(
            Condition::less_than(Expression::cached_value(0), length.clone()),
            actions,
        ),
    ]
}
