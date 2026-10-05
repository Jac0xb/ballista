//! Building a run from a compiled template, by name: the TypeScript SDK's `buildRunInstruction`
//! and `encodeRunInputs`.

use core::fmt;

use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use super::compile::CompiledTemplate;
use super::model::{Account, Type};

/// The most bytes a run's encoded inputs may take.
const MAX_INPUT_BYTES: usize = 1_024;
/// The most runtime accounts a run may pass, template account excluded.
const MAX_RUNTIME_ACCOUNTS: usize = 120;

/// Why a run could not be built: a missing or unknown name, a value of the wrong type, a row
/// count outside the template's range, or an account that does not match its pinned address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunError {
    message: String,
}

impl RunError {
    fn new(message: impl Into<String>) -> Self {
        RunError {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RunError {}

/// A run input's value: an integer, a `bool`, or bytes (a `pubkey` is its 32 bytes). Checked
/// against the declared type when the run is built.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunValue {
    Integer(Integer),
    Bool(bool),
    Bytes(Vec<u8>),
}

/// An integer of any Rust width, kept exactly until it is checked against the declared type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Integer {
    negative: bool,
    magnitude: u128,
}

macro_rules! unsigned {
    ($($ty:ty),*) => {$(
        impl From<$ty> for RunValue {
            fn from(value: $ty) -> Self {
                RunValue::Integer(Integer { negative: false, magnitude: value as u128 })
            }
        }
    )*};
}
macro_rules! signed {
    ($($ty:ty),*) => {$(
        impl From<$ty> for RunValue {
            fn from(value: $ty) -> Self {
                RunValue::Integer(Integer {
                    negative: value < 0,
                    magnitude: (value as i128).unsigned_abs(),
                })
            }
        }
    )*};
}
unsigned!(u8, u16, u32, u64, u128, usize);
signed!(i8, i16, i32, i64, i128, isize);

impl From<bool> for RunValue {
    fn from(value: bool) -> Self {
        RunValue::Bool(value)
    }
}

impl From<Pubkey> for RunValue {
    fn from(value: Pubkey) -> Self {
        RunValue::Bytes(value.to_bytes().to_vec())
    }
}

impl From<&Pubkey> for RunValue {
    fn from(value: &Pubkey) -> Self {
        RunValue::Bytes(value.to_bytes().to_vec())
    }
}

impl From<&[u8]> for RunValue {
    fn from(value: &[u8]) -> Self {
        RunValue::Bytes(value.to_vec())
    }
}

impl From<Vec<u8>> for RunValue {
    fn from(value: Vec<u8>) -> Self {
        RunValue::Bytes(value)
    }
}

impl From<&Vec<u8>> for RunValue {
    fn from(value: &Vec<u8>) -> Self {
        RunValue::Bytes(value.clone())
    }
}

impl<const N: usize> From<[u8; N]> for RunValue {
    fn from(value: [u8; N]) -> Self {
        RunValue::Bytes(value.to_vec())
    }
}

impl<const N: usize> From<&[u8; N]> for RunValue {
    fn from(value: &[u8; N]) -> Self {
        RunValue::Bytes(value.to_vec())
    }
}

/// One batch row: its accounts and its row inputs, by name.
#[derive(Clone, Debug, Default)]
pub struct Row {
    accounts: Vec<(String, Pubkey)>,
    inputs: Vec<(String, RunValue)>,
}

impl Row {
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds the row's account `name` to `address`.
    pub fn account(mut self, name: impl Into<String>, address: Pubkey) -> Self {
        self.accounts.push((name.into(), address));
        self
    }

    /// Sets the row's input `name`.
    pub fn input(mut self, name: impl Into<String>, value: impl Into<RunValue>) -> Self {
        self.inputs.push((name.into(), value.into()));
        self
    }
}

/// A run of a compiled template, built by name. Start one with [`CompiledTemplate::run`].
///
/// ```
/// use ballista_sdk::template::prelude::*;
///
/// let compiled = Template::new()
///     .input("reserve", Type::U64)
///     .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
///     .account("vault", account::signer().writable())
///     .account("destination", account::writable())
///     .step(step::require(lamports("vault").gt(input("reserve"))))
///     .compile()?;
///
/// let (template, vault, destination) =
///     (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
/// let instruction = compiled
///     .run(template)
///     .input("reserve", 2_000_000u64)
///     .account("systemProgram", SYSTEM_PROGRAM_ID)
///     .account("vault", vault) // signs and is writable, as declared
///     .account("destination", destination)
///     .instruction()?;
/// assert!(instruction.accounts[2].is_signer);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct Run<'a> {
    compiled: &'a CompiledTemplate,
    template: Pubkey,
    program_id: Pubkey,
    inputs: Vec<(String, RunValue)>,
    accounts: Vec<(String, Pubkey)>,
    rows: Vec<Row>,
    groups: Vec<(String, Vec<AccountMeta>)>,
}

impl CompiledTemplate {
    /// Starts a run of this template, uploaded at `template`. Name every input and every fixed
    /// account, add one [`Row`] per batch row and the members of each account group, then build
    /// it with [`Run::instruction`]. The signer and writable flags come from the declarations.
    pub fn run(&self, template: Pubkey) -> Run<'_> {
        Run {
            compiled: self,
            template,
            program_id: crate::ID,
            inputs: Vec::new(),
            accounts: Vec::new(),
            rows: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// What an `InvalidRunInputs` context points at, as the TypeScript SDK's `explainRunError`
    /// describes it. The program decodes the fixed inputs, then each row's inputs in order, and
    /// the context is the running index of the value that failed; trailing bytes after the last
    /// value report the index one past it. That index is also the first row input of the next
    /// row, so on a row boundary both readings are given. A context of 0 can also mean a short
    /// account-group length prefix, in a template with groups.
    fn describe_input_context(&self, context: usize) -> String {
        let group_prefix = if context == 0 && !self.account_group_order.is_empty() {
            ", or the account-group length prefix is short"
        } else {
            ""
        };
        let fixed = self.input_order.len();
        if let Some(input) = self.input_order.get(context) {
            return format!("input {input} could not be decoded{group_prefix}");
        }
        let per_row = self.row_input_order.len();
        if per_row == 0 {
            return format!("unexpected trailing input bytes{group_prefix}");
        }
        let offset = context - fixed;
        let row = offset / per_row;
        let input = &self.row_input_order[offset % per_row];
        let trailing = match (offset % per_row, row) {
            (0, 0) => ", or unexpected trailing input bytes".to_string(),
            (0, row) => format!(", or unexpected trailing input bytes after row {}", row - 1),
            _ => String::new(),
        };
        format!("input {input} in row {row} could not be decoded{trailing}{group_prefix}")
    }

    /// Explains a failed run's custom error code against this template, as the TypeScript SDK's
    /// `explainRunError(code, compiled).message`: `RequirementFailed at steps[1] (aboveReserve)`
    /// names the step, and its label, that failed. `None` for a code that is not Ballista's, such
    /// as one a called program returned.
    pub fn explain_error(&self, code: u32) -> Option<String> {
        let error = crate::decode_ballista_error(code)?;
        let context = usize::from(error.context);
        if error.source == crate::ErrorSource::Verifier {
            return Some(format!(
                "{} (verifier context {})",
                error.name, error.context
            ));
        }
        Some(match error.name {
            "InvalidRunInputs" => format!("{}: {}", error.name, self.describe_input_context(context)),
            "InvalidAccountRange" => format!(
                "{}: {} runtime accounts or iterations were supplied",
                error.name, error.context
            ),
            "AccountConstraintFailed" => {
                let fixed = &self.fixed_account_order;
                let stride = self.batch_account_order.len();
                let account = if context < fixed.len() {
                    fixed[context].clone()
                } else if stride == 0 {
                    format!("account[{context}]")
                } else {
                    let offset = context - fixed.len();
                    format!(
                        "{} in row {}",
                        self.batch_account_order[offset % stride],
                        offset / stride
                    )
                };
                format!(
                    "{}: account {account} does not satisfy its constraint",
                    error.name
                )
            }
            _ => match self.source(context) {
                Some(step) => match &step.label {
                    Some(label) => format!("{} at {} ({label})", error.name, step.path),
                    None => format!("{} at {}", error.name, step.path),
                },
                None => format!("{} at instruction {}", error.name, error.context),
            },
        })
    }

    /// Starts a run only to encode its inputs with [`Run::encode_inputs`], as for a nested run
    /// whose inputs travel as another template's bytes input.
    pub fn run_inputs(&self) -> Run<'_> {
        self.run(Pubkey::default())
    }
}

impl<'a> Run<'a> {
    /// Runs under the deployment at `program_id` instead of [`crate::ID`].
    pub fn program(mut self, program_id: Pubkey) -> Self {
        self.program_id = program_id;
        self
    }

    /// Sets input `name`.
    pub fn input(mut self, name: impl Into<String>, value: impl Into<RunValue>) -> Self {
        self.inputs.push((name.into(), value.into()));
        self
    }

    /// Binds fixed account `name` to `address`.
    pub fn account(mut self, name: impl Into<String>, address: Pubkey) -> Self {
        self.accounts.push((name.into(), address));
        self
    }

    /// Adds a batch row.
    pub fn row(mut self, row: Row) -> Self {
        self.rows.push(row);
        self
    }

    /// Adds batch rows, in order.
    pub fn rows(mut self, rows: impl IntoIterator<Item = Row>) -> Self {
        self.rows.extend(rows);
        self
    }

    /// Sets the members of account group `name`. A member keeps the writable flag its meta gives
    /// it, and is never a signer.
    pub fn group(
        mut self,
        name: impl Into<String>,
        members: impl IntoIterator<Item = AccountMeta>,
    ) -> Self {
        self.groups
            .push((name.into(), members.into_iter().collect()));
        self
    }

    /// The run's input bytes, without the instruction tag: group lengths, then the fixed inputs
    /// in declaration order, then each row's inputs. The TypeScript SDK's `encodeRunInputs`.
    pub fn encode_inputs(&self) -> Result<Vec<u8>, RunError> {
        let compiled = self.compiled;
        let mut bytes = Vec::new();
        for (name, _) in &self.groups {
            if !compiled.account_group_order.contains(name) {
                return Err(RunError::new(format!("Unknown account group: {name}")));
            }
        }
        for name in &compiled.account_group_order {
            let length = self
                .groups
                .iter()
                .find(|(group, _)| group == name)
                .map_or(0, |(_, members)| members.len());
            let length = u8::try_from(length).map_err(|_| {
                RunError::new(format!("Account group {name} has more than 255 members"))
            })?;
            bytes.push(length);
        }
        encode_values(
            &mut bytes,
            &compiled.input_order,
            &compiled.input_types,
            &self.inputs,
            "input",
        )?;
        if compiled.row_input_order.is_empty() {
            if self.rows.iter().any(|row| !row.inputs.is_empty()) {
                return Err(RunError::new("Template has no row inputs"));
            }
        } else {
            for (index, row) in self.rows.iter().enumerate() {
                encode_values(
                    &mut bytes,
                    &compiled.row_input_order,
                    &compiled.row_input_types,
                    &row.inputs,
                    &format!("row input in row {index}"),
                )?;
            }
        }
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(RunError::new("Encoded run inputs exceed 1024 bytes"));
        }
        Ok(bytes)
    }

    /// The `Run` instruction: the template account, then the fixed accounts in declaration order,
    /// each row's accounts, and each group's members, with the encoded inputs.
    pub fn instruction(&self) -> Result<Instruction, RunError> {
        let compiled = self.compiled;
        let definition = &compiled.template;
        let mut metas = Vec::new();

        for (name, _) in &self.accounts {
            if !compiled.fixed_account_order.contains(name) {
                return Err(RunError::new(format!("Unknown account binding: {name}")));
            }
        }
        for (name, declared) in &definition.accounts {
            let address = bound(&self.accounts, name)
                .ok_or_else(|| RunError::new(format!("Missing account binding: {name}")))?;
            metas.push(meta(address, declared, name)?);
        }

        let (max_rows, min_rows, row_accounts) = match &definition.batch {
            Some(batch) => (
                batch.max_iterations as usize,
                batch.min_iterations as usize,
                &batch.row[..],
            ),
            None => (0, 0, &[][..]),
        };
        if definition.batch.is_none() && !self.rows.is_empty() {
            return Err(RunError::new("Template has no batch range"));
        }
        if self.rows.len() > max_rows {
            return Err(RunError::new(
                "Batch row count exceeds the template maximum",
            ));
        }
        if self.rows.len() < min_rows {
            return Err(RunError::new(
                "Batch row count is below the template minimum",
            ));
        }
        for (index, row) in self.rows.iter().enumerate() {
            for (name, declared) in row_accounts {
                let address = bound(&row.accounts, name).ok_or_else(|| {
                    RunError::new(format!("Missing batch account {name} in row {index}"))
                })?;
                metas.push(meta(address, declared, name)?);
            }
            if let Some((name, _)) = row
                .accounts
                .iter()
                .find(|(name, _)| !compiled.batch_account_order.contains(name))
            {
                return Err(RunError::new(format!(
                    "Unknown batch account {name} in row {index}"
                )));
            }
        }

        for name in &compiled.account_group_order {
            if let Some((_, members)) = self.groups.iter().find(|(group, _)| group == name) {
                for member in members {
                    if member.is_signer {
                        return Err(RunError::new(format!(
                            "A member of {name} is a signer; group members never are"
                        )));
                    }
                    metas.push(member.clone());
                }
            }
        }
        if metas.len() > MAX_RUNTIME_ACCOUNTS {
            return Err(RunError::new(
                "Run uses more than 120 runtime account slots",
            ));
        }

        let inputs = self.encode_inputs()?;
        Ok(crate::run_instruction_for_program(
            self.template,
            metas,
            &inputs,
            &self.program_id,
        ))
    }
}

fn bound(bindings: &[(String, Pubkey)], name: &str) -> Option<Pubkey> {
    bindings
        .iter()
        .find(|(other, _)| other == name)
        .map(|(_, address)| *address)
}

fn meta(address: Pubkey, declared: &Account, name: &str) -> Result<AccountMeta, RunError> {
    if let Some(pinned) = declared.address {
        if address.to_bytes() != pinned {
            return Err(RunError::new(format!(
                "Account {name} does not match its fixed address"
            )));
        }
    }
    Ok(AccountMeta {
        pubkey: address,
        is_signer: declared.signer,
        is_writable: declared.writable,
    })
}

fn encode_values(
    bytes: &mut Vec<u8>,
    order: &[String],
    types: &[Type],
    values: &[(String, RunValue)],
    what: &str,
) -> Result<(), RunError> {
    if let Some((name, _)) = values.iter().find(|(name, _)| !order.contains(name)) {
        return Err(RunError::new(format!("Unknown {what}: {name}")));
    }
    for (name, ty) in order.iter().zip(types) {
        let value = values
            .iter()
            .find(|(other, _)| other == name)
            .map(|(_, value)| value)
            .ok_or_else(|| RunError::new(format!("Missing {what}: {name}")))?;
        encode(bytes, *ty, value, name)?;
    }
    Ok(())
}

fn encode(bytes: &mut Vec<u8>, ty: Type, value: &RunValue, name: &str) -> Result<(), RunError> {
    let mismatch = || RunError::new(format!("Expected {} input for {name}", type_name(ty)));
    match (ty, value) {
        (Type::Bool, RunValue::Bool(value)) => bytes.push(u8::from(*value)),
        (Type::Pubkey, RunValue::Bytes(value)) if value.len() == 32 => {
            bytes.extend_from_slice(value)
        }
        (Type::Bytes(max), RunValue::Bytes(value)) => {
            if value.len() > max as usize {
                return Err(RunError::new(format!(
                    "Expected at most {max} input bytes for {name}"
                )));
            }
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value);
        }
        (Type::U64 | Type::I64 | Type::U128, RunValue::Integer(integer)) => {
            let out_of_range =
                || RunError::new(format!("Input {name} is outside {} range", type_name(ty)));
            match ty {
                Type::U64 if !integer.negative => bytes.extend_from_slice(
                    &u64::try_from(integer.magnitude)
                        .map_err(|_| out_of_range())?
                        .to_le_bytes(),
                ),
                Type::U128 if !integer.negative => {
                    bytes.extend_from_slice(&integer.magnitude.to_le_bytes())
                }
                Type::I64 => {
                    let value = if integer.negative {
                        i128::try_from(integer.magnitude)
                            .map(|magnitude| -magnitude)
                            .map_err(|_| out_of_range())?
                    } else {
                        i128::try_from(integer.magnitude).map_err(|_| out_of_range())?
                    };
                    let value = i64::try_from(value).map_err(|_| out_of_range())?;
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                _ => return Err(out_of_range()),
            }
        }
        _ => return Err(mismatch()),
    }
    Ok(())
}

fn type_name(ty: Type) -> &'static str {
    match ty {
        Type::Bool => "bool",
        Type::U64 => "u64",
        Type::I64 => "i64",
        Type::U128 => "u128",
        Type::Pubkey => "pubkey",
        Type::Bytes(_) => "bytes",
    }
}
