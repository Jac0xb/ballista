use borsh::{BorshDeserialize, BorshSerialize};
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize)]
struct Value {
    int_value: i32,
}

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize)]
enum ValueType {
    Integer,
    Float,
    String,
}

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize)]
struct ArithmeticBehavior {
    overflow: bool,
}

#[derive(Clone, Debug, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
enum Condition {
    AlwaysTrue,
    AlwaysFalse,
    Equals(Box<Expression>, Box<Expression>),
}

#[derive(Clone, Debug, PartialEq, BorshDeserializeBoxed, BorshSerializeBoxed)]
enum Expression {
    Literal(Value),
    InputValue(u8),
    StaticValue(u8),
    CachedValue(u8),
    SafeCast(Box<Expression>, ValueType),
    Multiply(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Divide(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Add(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Subtract(Box<Expression>, Box<Expression>, ArithmeticBehavior),
    Conditional(Box<Condition>, Box<Expression>, Box<Expression>),
}

#[test]
fn test_simple_expression() {
    let expression = Expression::Literal(Value { int_value: 42 });

    let mut serialized = Vec::new();
    expression
        .serialize(&mut serialized)
        .expect("Serialization failed");

    let deserialized =
        Expression::deserialize_reader(&mut &serialized[..]).expect("Deserialization failed");

    assert_eq!(expression, deserialized);
}

#[test]
fn test_nested_expression() {
    let expression = Expression::Add(
        Box::new(Expression::Multiply(
            Box::new(Expression::InputValue(10)),
            Box::new(Expression::InputValue(20)),
            ArithmeticBehavior { overflow: false },
        )),
        Box::new(Expression::Literal(Value { int_value: 5 })),
        ArithmeticBehavior { overflow: false },
    );

    let mut serialized = Vec::new();
    expression
        .serialize(&mut serialized)
        .expect("Serialization failed");

    let deserialized =
        Expression::deserialize_reader(&mut &serialized[..]).expect("Deserialization failed");

    assert_eq!(expression, deserialized);
}

#[test]
fn test_conditional_expression() {
    let condition = Condition::Equals(
        Box::new(Expression::InputValue(1)),
        Box::new(Expression::InputValue(1)),
    );

    let expression = Expression::Conditional(
        Box::new(condition),
        Box::new(Expression::Literal(Value { int_value: 100 })),
        Box::new(Expression::Literal(Value { int_value: 0 })),
    );

    let mut serialized = Vec::new();
    expression
        .serialize(&mut serialized)
        .expect("Serialization failed");

    let deserialized =
        Expression::deserialize_reader(&mut &serialized[..]).expect("Deserialization failed");

    assert_eq!(expression, deserialized);
}

#[test]
fn test_all_variants() {
    let expressions = vec![
        Expression::Literal(Value { int_value: -42 }),
        Expression::InputValue(255),
        Expression::StaticValue(128),
        Expression::CachedValue(64),
        Expression::SafeCast(Box::new(Expression::InputValue(10)), ValueType::Float),
        Expression::Divide(
            Box::new(Expression::Literal(Value { int_value: 100 })),
            Box::new(Expression::InputValue(4)),
            ArithmeticBehavior { overflow: false },
        ),
    ];

    for expr in expressions {
        let mut serialized = Vec::new();
        expr.serialize(&mut serialized)
            .expect("Serialization failed");

        let deserialized =
            Expression::deserialize_reader(&mut &serialized[..]).expect("Deserialization failed");

        assert_eq!(expr, deserialized);
    }
}

#[test]
fn test_complex_recursive_expression() {
    let expression = Expression::Add(
        Box::new(Expression::Subtract(
            Box::new(Expression::Multiply(
                Box::new(Expression::InputValue(2)),
                Box::new(Expression::InputValue(3)),
                ArithmeticBehavior { overflow: false },
            )),
            Box::new(Expression::Divide(
                Box::new(Expression::InputValue(10)),
                Box::new(Expression::InputValue(2)),
                ArithmeticBehavior { overflow: false },
            )),
            ArithmeticBehavior { overflow: false },
        )),
        Box::new(Expression::Literal(Value { int_value: 5 })),
        ArithmeticBehavior { overflow: false },
    );

    let mut serialized = Vec::new();
    expression
        .serialize(&mut serialized)
        .expect("Serialization failed");

    let deserialized =
        Expression::deserialize_reader(&mut &serialized[..]).expect("Deserialization failed");

    assert_eq!(expression, deserialized);
}

#[test]
fn test_deserialization_error() {
    let invalid_data = [255, 0, 0, 0];

    let result = Expression::deserialize_reader(&mut &invalid_data[..]);

    assert!(result.is_err());
}
