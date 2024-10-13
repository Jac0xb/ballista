use ballista_common::{
    logical_components::{Condition, Expression, Value},
    task::{action::set_cache::SetCacheType, task_action::TaskAction},
};

pub fn actions_for_loop(
    mut actions: Vec<TaskAction>,
    length: &Expression,
    increment_by: u8,
) -> Vec<TaskAction> {
    actions.push(TaskAction::SetCache(SetCacheType::Expression {
        index: 0,
        expression: Expression::cached_value(0).checked_add(&Value::U8(increment_by).expr()),
    }));

    vec![
        TaskAction::SetCache(SetCacheType::Expression {
            index: 0,
            expression: Value::U8(0).expr(),
        }),
        TaskAction::loop_action(
            Condition::less_than(Expression::cached_value(0), length.clone()),
            actions,
        ),
    ]
}
