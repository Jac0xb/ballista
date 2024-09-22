pub fn transfer() {
    let global_schema = Box::new(GlobalSchema {
        instructions: vec![InstructionDefinition {
            serialization: bask::SerializationType::Borsh,
            arguments: vec![
                ArgumentDefinition::Constant {
                    value: Value::Bytes(vec![2, 0, 0, 0]),
                },
                ArgumentDefinition::Input {
                    value_type: ValueType::U64,
                },
            ],
            accounts: vec![
                AccountDefinition {
                    name: "from".to_string(),
                    signer: true,
                    writable: true,
                    validate: None,
                },
                AccountDefinition {
                    name: "to".to_string(),
                    signer: false,
                    writable: true,
                    validate: None,
                },
            ],
        }],
        tasks: vec![TaskDefinition {
            shared_account_len: 3,
            instructions: vec![InstructionInstance {
                program: AccountSource::Shared(0),
                schema_id: 0,
                accounts: vec![
                    AccountInstance {
                        key: AccountSource::Shared(1),
                    },
                    AccountInstance {
                        key: AccountSource::Shared(2),
                    },
                ],
                arguments: vec![TaskArgument {
                    value_type: ValueType::U8,
                    argument: ArgumentSource::Shared(0),
                    transform: Some(bask::Expression::SafeCast(
                        Box::new(Expression::InputArgument),
                        ValueType::U64,
                    )),
                }],
            }],
            shared_values: vec![Value::U64(100_000_000)],
        }],
    });
}
