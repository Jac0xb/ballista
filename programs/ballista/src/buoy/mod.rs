use std::ops::RangeInclusive;

use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

struct AccountMetadata<'info, 'a> {
    schema_id: u8, // or u16/u32 depending on how many schemas you expect
    version: u16,  // Optional: for schema versioning
    info: &'a AccountInfo<'info>,
    data: &'a [u8],
}

enum Value {
    Scalar(ScalarValue),
    Composite(CompositeValue),
    CustomType(u8), // Index to a custom type definition
    Enum(u8),
    UInt(u64), // Added for convenience
}

enum ScalarValue {
    Int(i64), // Reduced to i64 for Solana's typical use cases
    UInt(u64),
    Float(f64),
    Bool(bool),
    String(Vec<u8>), // Using Vec<u8> for fixed-size strings
    Bytes(Vec<u8>),
    Pubkey(Pubkey),
}

enum CompositeValue {
    Option(Box<Value>),
    Vec(Vec<Value>),
}

struct StructDefinition {
    name: u8, // Index in global string array
    fields: Vec<FieldDefinition>,
}

struct CustomTypeDefinition {
    name: u8, // Index in global string array
    fields: Vec<FieldDefinition>,
}

struct EnumDefinition {
    name: u8,          // Index in global string array
    variants: Vec<u8>, // Indices of variant names in global string array
}

struct SchemaDomain {
    enums: Vec<EnumDefinition>,
    custom_types: Vec<CustomTypeDefinition>,
    structs: Vec<StructDefinition>,
    actions: Vec<Action>,
}

struct FieldDefinition {
    name: u8, // Index in global string array
    value_type: SharedType,
    constraints: Vec<Constraint>,
}

enum SharedType {
    Scalar(ScalarType),
    Composite(CompositeType),
    Custom(u8), // Index to a custom type definition
    Enum(u8),   // Index to an enum definition
}

enum ScalarType {
    Int,
    UInt,
    Float,
    Bool,
    String(usize),
    Bytes(usize),
    Pubkey,
}

enum CompositeType {
    Option(Box<SharedType>),
    Vec(Box<SharedType>, usize), // Added size limit
}

enum Constraint {
    Range(RangeInclusive<Value>),
    Length(RangeInclusive<usize>),
    Custom(u8), // Index to a custom validation function
}

//
// Actions System
//
struct Action {
    name: u8, // Index in global string array
    parameters: Vec<Parameter>,
    account_params: Vec<AccountParameter>,
    preconditions: Vec<Condition>,
    effects: Vec<Effect>,
}

struct Parameter {
    name: u8, // Index in global string array
    value_type: SharedType,
}

struct AccountParameter {
    name: u8,        // Index in global string array
    struct_type: u8, // Index of the struct in the SchemaDomain
    validation: AccountValidation,
    is_signer: bool,
    is_writable: bool,
}

enum AccountValidation {
  Any,
  Constant(Pubkey),
  ProgramOwned(Pubkey),
  Seeds(Vec<SeedComponent>),
  DerivedFromSigner(Vec<SeedComponent>),
  MultipleChecks(Vec<AccountValidation>), // New variant for multiple checks
}

enum SeedComponent {
    Constant(Vec<u8>),
    FieldReference(u8, u8), // Struct index, field index
    Parameter(u8),          // Parameter index
}

enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    And,
    Or,
    Xor,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

enum UnaryOperator {
    Negate,
    Not,
}

enum Expression {
    Literal(Value),
    FieldReference(Box<AccountReference>, u8), // Account, field index
    BinaryOp(Box<Expression>, BinaryOperator, Box<Expression>),
    UnaryOp(UnaryOperator, Box<Expression>),
    Conditional(Box<Expression>, Box<Expression>, Box<Expression>),
    ArrayAccess(Box<Expression>, Box<Expression>),
}

enum Condition {
    Expression(Expression),
}

enum Effect {
    SetField(AccountReference, u8, Expression), // Account, field index, value
    ConditionalEffect(Expression, Box<Effect>, Option<Box<Effect>>),
}

enum AccountReference {
    Parameter(u8),                // Index of the account parameter
    Derived(u8, Box<Expression>), // Struct type, seed expression
}

// execution engine
struct ExecutionContext<'info, 'a> {
    schema_domain: &'a SchemaDomain,
    accounts: Vec<AccountMetadata<'info, 'a>>,
}

impl<'info, 'a> ExecutionContext<'info, 'a> {
    fn resolve_account(
        &self,
        account_ref: &AccountReference,
    ) -> Result<&AccountMetadata<'info, 'a>, ProgramError> {
        // Implement resolution logic
        panic!("Not implemented");
    }

    fn resolve_account_mut(
        &mut self,
        account_ref: &AccountReference,
    ) -> Result<&mut AccountMetadata<'info, 'a>, ProgramError> {
        // Implement resolution logic
        panic!("Not implemented");
    }

    fn evaluate_expression(&self, expr: &Expression) -> Result<Value, ProgramError> {
        match expr {
            Expression::FieldReference(account_ref, field_index) => {
                let account_meta = self.resolve_account(account_ref)?;

                panic!("Not implemented");
                // Deserialize the specific field from account_meta.data based on schema_id
                // Return the field value
            } // Other cases...
        }
    }

    fn apply_effect(&mut self, effect: &Effect) -> ProgramResult {
        match effect {
            Effect::SetField(account_ref, field_index, expr) => {
                let account_meta = self.resolve_account_mut(account_ref)?;
                let value = self.evaluate_expression(expr)?;

                // Get mutable reference to account data
                let mut account_data = account_meta.info.try_borrow_mut_data()?;

                // Serialize and write the value to the correct field in account_data
                // based on account_meta.schema_id and field_index

                // Update account_meta.data reference
                account_meta.data = &account_data[..];

                Ok(())
            } // Other cases...
        }
    }
}

// Function to create AccountMetadata from AccountInfo
fn create_account_metadata<'info, 'a>(
    account: &'a AccountInfo<'info>,
) -> Result<AccountMetadata<'info, 'a>, ProgramError> {
    let data = account.try_borrow_data()?;
    if data.len() < 3 {
        // Minimum size for schema_id and version
        return Err(ProgramError::AccountDataTooSmall);
    }

    let schema_id = data[0];
    let version = u16::from_le_bytes([data[1], data[2]]);

    Ok(AccountMetadata {
        schema_id,
        version,
        info: account,
        data: &data[3..],
    })
}

// Function to initialize account with metadata
fn initialize_account(account: &AccountInfo, schema_id: u8, version: u16) -> ProgramResult {
    let mut data = account.try_borrow_mut_data()?;
    if data.len() < 3 {
        return Err(ProgramError::AccountDataTooSmall);
    }

    data[0] = schema_id;
    data[1..3].copy_from_slice(&version.to_le_bytes());

    // Initialize the rest of the account data as needed

    Ok(())



let start_battle_action = Action {
  name: 37, // "StartBattle" in global string array
  parameters: vec![],
  account_params: vec![
      AccountParameter {
          name: 38, // "pokemon1" in global string array
          struct_type: 0, // Index of pokemon_def
          validation: AccountValidation::MultipleChecks(vec![
              AccountValidation::Seeds(vec![
                  SeedComponent::Constant(b"pokemon".to_vec()),
                  SeedComponent::FieldReference(0, 0), // 0 is the index for the Pokémon's ID field
              ]),
          ]),
          is_signer: false,
          is_writable: true,
      },
      AccountParameter {
          name: 39, // "pokemon2" in global string array
          struct_type: 0, // Index of pokemon_def
          validation: AccountValidation::MultipleChecks(vec![
              AccountValidation::Seeds(vec![
                  SeedComponent::Constant(b"pokemon".to_vec()),
                  SeedComponent::FieldReference(0, 0), // 0 is the index for the Pokémon's ID field
              ]),
          ]),
          is_signer: false,
          is_writable: true,
      },
  ],
  preconditions: vec![
      // Check if both Pokémon are not in battle
      Condition::Expression(Expression::BinaryOp(
          Box::new(Expression::FieldReference(Box::new(AccountReference::Parameter(0)), 32)),
          BinaryOperator::Equal,
          Box::new(Expression::Literal(Value::UInt(0))) // 0 is the index for "NotInBattle"
      )),
      Condition::Expression(Expression::BinaryOp(
          Box::new(Expression::FieldReference(Box::new(AccountReference::Parameter(1)), 32)),
          BinaryOperator::Equal,
          Box::new(Expression::Literal(Value::UInt(0))) // 0 is the index for "NotInBattle"
      )),
  ],
  effects: vec![
      // ... (same as before)
  ],
};
}


