//! The compiler: a line-by-line port of the TypeScript SDK's `compiler.ts`, so a template
//! compiles to the same bytes in either language. Where this file and `compiler.ts` differ in
//! structure, the order of every table write still matches, because that order is the bytes.

use std::collections::{HashMap, HashSet};

use ballista_common::template::{
    ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, INSTRUCTION_FLAG_DYNAMIC_OFFSET,
    ITERATION_ACCOUNT_BIT, NO_INDEX, PROGRAM_FLAG_EMIT_EVENT,
};
use ballista_common::template::{
    DATA_LITERAL, DATA_REG_BOOL, DATA_REG_BYTES, DATA_REG_I64, DATA_REG_PUBKEY, DATA_REG_U128,
    DATA_REG_U16, DATA_REG_U32, DATA_REG_U64, DATA_REG_U8,
};
use ballista_common::template::{
    INSTRUCTIONS_SYSVAR_ID, MAX_CPI_ACCOUNTS, MAX_CPI_DATA_LEN, MAX_EXPANDED_CPIS, MAX_PDA_SEEDS,
    MAX_PDA_SEED_LEN, MAX_REGISTERS, MAX_REGISTRY_OPENS, MAX_RETURN_DATA_LEN,
    MAX_TEMPLATE_PAYLOAD_LEN, MAX_VM_INSTRUCTIONS, MIN_EMIT_TAG_LEN, REGISTRY_OPEN_CPIS,
    RUN_EVENT_TAG_FAMILY, TEMPLATE_PROGRAM_MAGIC, TEMPLATE_PROGRAM_VERSION,
};
use ballista_common::template::{
    OP_ACCOUNT_DATA_LEN, OP_ACCOUNT_IS_EMPTY, OP_ACCOUNT_KEY, OP_ACCOUNT_LAMPORTS,
    OP_ACCOUNT_OWNER, OP_ADD, OP_AND, OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR, OP_BYTES_LEN, OP_CAST_I64,
    OP_CAST_U128, OP_CAST_U64, OP_CLOCK_SLOT, OP_CLOCK_TIMESTAMP, OP_CONST_BOOL, OP_CONST_BYTES,
    OP_CONST_I64, OP_CONST_PUBKEY, OP_CONST_U128, OP_CONST_U64, OP_CREATE_PDA, OP_DERIVE_PDA,
    OP_DIV, OP_EMIT, OP_EQ, OP_FOREACH, OP_GROUP_ANY, OP_GROUP_COUNT, OP_GROUP_LENGTH, OP_GT,
    OP_GTE, OP_INSTRUCTION_ACCOUNT, OP_INSTRUCTION_ACCOUNT_COUNT, OP_INSTRUCTION_ACCOUNT_FLAGS,
    OP_INSTRUCTION_COUNT, OP_INSTRUCTION_DATA_LEN, OP_INSTRUCTION_INDEX, OP_INSTRUCTION_PROGRAM,
    OP_INVOKE, OP_LOAD_INPUT, OP_LOOP_INDEX, OP_LT, OP_LTE, OP_MAX, OP_MIN, OP_MOVE, OP_MUL,
    OP_MUL_DIV, OP_MUL_DIV_CEIL, OP_NE, OP_NOT, OP_OPEN_REGISTRY, OP_OR, OP_POW10,
    OP_READ_ACCOUNT_BYTES, OP_READ_BOOL, OP_READ_I32, OP_READ_I64, OP_READ_INSTRUCTION_BYTES,
    OP_READ_INSTRUCTION_DATA, OP_READ_PUBKEY, OP_READ_REGISTRY, OP_READ_U128, OP_READ_U16,
    OP_READ_U32, OP_READ_U64, OP_READ_U8, OP_REM, OP_REPEAT, OP_REQUIRE, OP_RETURN_DATA, OP_SELECT,
    OP_SET_RETURN_DATA, OP_SHL, OP_SHR, OP_SUB, OP_WRITE_REGISTRY,
};

use super::model::*;
use super::reuse::{self, Record};
use super::validate;
use super::CompileError;

/// One emitted VM instruction mapped back to the step that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapEntry {
    /// Instruction index, which is also the program counter reported in run errors.
    pub pc: usize,
    /// Step path such as `steps[2]` or `steps[1].steps[0]`.
    pub path: String,
    pub label: Option<String>,
}

/// Sizes of a compiled template, as the TypeScript SDK's `CompileStats`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileStats {
    pub payload_bytes: usize,
    pub fixed_accounts: usize,
    pub batch_stride: usize,
    pub batch_max_iterations: usize,
    pub batch_min_iterations: usize,
    pub inputs: usize,
    /// The registers the header declares, after reuse when the template needed more than 64.
    pub registers: usize,
    pub instructions: usize,
    pub cpis: usize,
    pub max_expanded_cpis: usize,
    pub max_cpi_data_length: usize,
    pub emit_event: bool,
    pub row_inputs: usize,
    pub account_groups: usize,
}

/// A compiled template: the bytes to upload, their hash, and the orders a run follows.
#[derive(Clone, Debug)]
pub struct CompiledTemplate {
    /// The template account's payload.
    pub bytes: Vec<u8>,
    /// SHA-256 of `bytes`, which the template account records.
    pub hash: [u8; 32],
    /// Fixed inputs in declaration order: the order the run data encodes them in.
    pub input_order: Vec<String>,
    /// Each fixed input's type, in `input_order`.
    pub input_types: Vec<Type>,
    /// Row inputs in declaration order; each batch row supplies these.
    pub row_input_order: Vec<String>,
    /// Each row input's type, in `row_input_order`.
    pub row_input_types: Vec<Type>,
    /// Fixed accounts in declaration order: the order a run passes them in.
    pub fixed_account_order: Vec<String>,
    /// Batch row accounts in declaration order.
    pub batch_account_order: Vec<String>,
    /// Account groups in declaration order; the run data prefix and metas follow it.
    pub account_group_order: Vec<String>,
    /// Registries in declaration order: a registry's index, part of its entries' addresses, is
    /// its position here.
    pub registry_order: Vec<String>,
    pub stats: CompileStats,
    pub source_map: Vec<SourceMapEntry>,
    /// The definition, for [`CompiledTemplate::run`] to check a run against.
    pub(crate) template: Template,
}

impl CompiledTemplate {
    /// The index of registry `name`, for [`find_registry_entry_address`](crate::find_registry_entry_address).
    pub fn registry_index(&self, name: &str) -> Option<u8> {
        self.registry_order
            .iter()
            .position(|registry| registry == name)
            .map(|index| index as u8)
    }

    /// The source map entry for program counter `pc`, as a run error reports it.
    pub fn source(&self, pc: usize) -> Option<&SourceMapEntry> {
        self.source_map.iter().find(|entry| entry.pc == pc)
    }
}

impl Template {
    /// Compiles the template to the bytes a template account holds, the same bytes the
    /// TypeScript SDK's `compileTemplate` produces for the same template.
    ///
    /// Fails, with the TypeScript compiler's message, for a template the TypeScript SDK refuses:
    /// a malformed declaration, an unknown name, a type mismatch, an invoked or PDA program that
    /// pins no address, an account read as data that pins neither owner nor address, a template
    /// past the wire format's limits, and the rest.
    pub fn compile(&self) -> Result<CompiledTemplate, CompileError> {
        validate::template(self)?;
        Compiler::new(self).compile()
    }
}

#[derive(Clone, Copy, Debug)]
struct Value {
    register: usize,
    ty: ValueType,
    max_length: usize,
}

type Bindings = Vec<(String, Value)>;

fn binding<'a>(bindings: &'a Bindings, name: &str) -> Option<&'a Value> {
    bindings
        .iter()
        .find(|(other, _)| other == name)
        .map(|(_, value)| value)
}

fn bind(bindings: &mut Bindings, name: &str, value: Value) {
    match bindings.iter_mut().find(|(other, _)| other == name) {
        Some(slot) => slot.1 = value,
        None => bindings.push((name.to_string(), value)),
    }
}

/// The loop a step sits in: `Rows` for a `forEach` body, `Count` for a `repeat` body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoopKind {
    Rows,
    Count,
}

#[derive(Clone, Debug)]
struct RegistryLayout {
    index: usize,
    size: usize,
    fields: Vec<(String, RegistryField)>,
}

#[derive(Clone, Copy, Debug)]
struct RegistryField {
    offset: usize,
    ty: ReadType,
}

/// The key a use-count or a constant is filed under, as `literalKey` and `input:<name>`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum UseKey {
    Literal(Literal),
    Input(String),
}

fn error(message: impl Into<String>) -> CompileError {
    CompileError::new(message)
}

fn require_type(value: &Value, expected: ValueType, context: &str) -> Result<(), CompileError> {
    if value.ty != expected {
        return Err(error(format!(
            "{context} requires {}; received {}",
            expected.name(),
            value.ty.name()
        )));
    }
    Ok(())
}

fn is_numeric(ty: ValueType) -> bool {
    matches!(ty, ValueType::U64 | ValueType::I64 | ValueType::U128)
}

fn is_unsigned(ty: ValueType) -> bool {
    matches!(ty, ValueType::U64 | ValueType::U128)
}

fn fixed_value_length(ty: ValueType) -> usize {
    match ty {
        ValueType::Bool => 1,
        ValueType::U64 | ValueType::I64 => 8,
        ValueType::U128 => 16,
        ValueType::Pubkey => 32,
        ValueType::Bytes => 0,
    }
}

pub(crate) fn read_opcode(ty: ReadType) -> u8 {
    match ty {
        ReadType::Bool => OP_READ_BOOL,
        ReadType::U8 => OP_READ_U8,
        ReadType::U16 => OP_READ_U16,
        ReadType::U32 => OP_READ_U32,
        ReadType::I32 => OP_READ_I32,
        ReadType::U64 => OP_READ_U64,
        ReadType::I64 => OP_READ_I64,
        ReadType::U128 => OP_READ_U128,
        ReadType::Pubkey => OP_READ_PUBKEY,
    }
}

fn read_result_type(ty: ReadType) -> ValueType {
    match ty {
        ReadType::Bool => ValueType::Bool,
        ReadType::U8 | ReadType::U16 | ReadType::U32 | ReadType::U64 => ValueType::U64,
        ReadType::I32 | ReadType::I64 => ValueType::I64,
        ReadType::U128 => ValueType::U128,
        ReadType::Pubkey => ValueType::Pubkey,
    }
}

/// A registry field's read width, as `RegistryFieldType` names it.
pub(crate) fn registry_read_type(ty: Type) -> Option<ReadType> {
    match ty {
        Type::Bool => Some(ReadType::Bool),
        Type::U64 => Some(ReadType::U64),
        Type::I64 => Some(ReadType::I64),
        Type::U128 => Some(ReadType::U128),
        Type::Pubkey => Some(ReadType::Pubkey),
        Type::Bytes(_) => None,
    }
}

fn value_type_code(ty: Type) -> u8 {
    match ty {
        Type::Bool => 1,
        Type::U64 => 2,
        Type::I64 => 3,
        Type::U128 => 4,
        Type::Pubkey => 5,
        Type::Bytes(_) => 6,
    }
}

fn range_immediate(offset: usize, length: usize) -> u64 {
    offset as u64 | ((length as u64) << 32)
}

fn registry_field_immediate(field: RegistryField) -> u64 {
    field.offset as u64 | (u64::from(read_opcode(field.ty)) << 16)
}

#[allow(clippy::too_many_arguments)]
fn instruction_record(
    operation: u8,
    dst: u8,
    a: u8,
    b: u8,
    c: u8,
    immediate: u64,
    flags: u8,
) -> Record {
    let mut record = [0u8; 16];
    record[0] = operation;
    record[1] = dst;
    record[2] = a;
    record[3] = b;
    record[4] = c;
    record[5] = flags;
    record[6..14].copy_from_slice(&immediate.to_le_bytes());
    record
}

fn segment_record(kind: u8, register: u8, offset: u16, length: u16) -> [u8; 8] {
    let mut record = [0u8; 8];
    record[0] = kind;
    record[1] = register;
    record[2..4].copy_from_slice(&offset.to_le_bytes());
    record[4..6].copy_from_slice(&length.to_le_bytes());
    record
}

/// The conjuncts of a requirement: `and(and(a, b), c)` is three separate assertions.
fn flatten_conjunction<'a>(condition: &'a Expr, into: &mut Vec<&'a Expr>) {
    if let Node::Binary {
        op: BinaryOp::And,
        left,
        right,
    } = &condition.0
    {
        flatten_conjunction(left, into);
        flatten_conjunction(right, into);
    } else {
        into.push(condition);
    }
}

// ----------------------------------------------------------------------------- tree walking

/// Visits every expression in `steps` in the order the TypeScript compiler's walk of the parsed
/// object tree reaches them: each node before its children, children in schema field order.
/// `collectLiterals`, `countUses` and `collectInputNames` all walk that order.
fn walk_steps(steps: &[Step], visit: &mut dyn FnMut(&Expr)) {
    for step in steps {
        match &step.0 {
            StepNode::Require { condition, .. } => walk_expr(condition, visit),
            StepNode::Let { value, .. }
            | StepNode::Assign { value, .. }
            | StepNode::SetRegistry { value, .. } => walk_expr(value, visit),
            StepNode::Invoke(invoke) => {
                walk_parts(&invoke.data, visit);
                if let Some(when) = &invoke.when {
                    walk_expr(when, visit);
                }
            }
            StepNode::Emit { parts, .. } | StepNode::SetReturnData { parts, .. } => {
                walk_parts(parts, visit)
            }
            StepNode::Loop(body) => {
                if let Some((count, _)) = &body.repeat {
                    walk_expr(count, visit);
                }
                walk_steps(&body.steps, visit);
            }
            StepNode::Invalid(_) => {}
        }
    }
}

fn walk_parts(parts: &[DataPart], visit: &mut dyn FnMut(&Expr)) {
    for part in parts {
        if let DataNode::Encoded(_, value) = &part.0 {
            walk_expr(value, visit);
        }
    }
}

fn walk_expr(expr: &Expr, visit: &mut dyn FnMut(&Expr)) {
    visit(expr);
    match &expr.0 {
        Node::AccountData { offset, .. } => {
            if let Offset::Dynamic(offset) = offset.as_ref() {
                walk_expr(offset, visit);
            }
        }
        Node::Pda { seeds, bump, .. } => {
            for seed in seeds {
                walk_expr(seed, visit);
            }
            if let Some(bump) = bump {
                walk_expr(bump, visit);
            }
        }
        Node::Binary { left, right, .. } => {
            walk_expr(left, visit);
            walk_expr(right, visit);
        }
        Node::MultiplyDivide {
            left,
            right,
            divisor,
            ..
        } => {
            walk_expr(left, visit);
            walk_expr(right, visit);
            walk_expr(divisor, visit);
        }
        Node::PowerOfTen(value) | Node::Not(value) | Node::BytesLength(value) => {
            walk_expr(value, visit)
        }
        Node::Cast { value, .. } => walk_expr(value, visit),
        Node::Select {
            condition,
            if_true,
            if_false,
        } => {
            walk_expr(condition, visit);
            walk_expr(if_true, visit);
            walk_expr(if_false, visit);
        }
        Node::Instruction { index, .. } => walk_expr(index, visit),
        Node::InstructionAccount {
            index, position, ..
        } => {
            walk_expr(index, visit);
            walk_expr(position, visit);
        }
        Node::InstructionData { index, offset, .. }
        | Node::InstructionDataBytes { index, offset, .. } => {
            walk_expr(index, visit);
            walk_expr(offset, visit);
        }
        Node::AccountDataBytes { offset, .. } => walk_expr(offset, visit),
        // The TypeScript walk reaches a filter's fields in schema order: the programs, which hold
        // no expression, the floor, each match's value, then the except keys.
        Node::GroupFilter { filter, .. } => {
            for (_, value) in &filter.matches {
                walk_expr(value, visit);
            }
            for key in &filter.except_keys {
                walk_expr(key, visit);
            }
        }
        Node::Input(_)
        | Node::RowInput(_)
        | Node::Variable(_)
        | Node::Literal(_)
        | Node::AccountField(..)
        | Node::ReturnData { .. }
        | Node::ClockSlot
        | Node::ClockUnixTimestamp
        | Node::LoopIndex
        | Node::InstructionCount(_)
        | Node::CurrentInstructionIndex(_)
        | Node::Registry { .. }
        | Node::GroupLength(_)
        | Node::Invalid(_) => {}
    }
}

fn count_cpis(steps: &[Step]) -> usize {
    steps
        .iter()
        .map(|step| match &step.0 {
            StepNode::Invoke(_) => 1,
            StepNode::Loop(body) => count_cpis(&body.steps),
            _ => 0,
        })
        .sum()
}

/// The most invocations a run can reach: each loop's body runs its maximum number of times.
fn worst_case_cpis(steps: &[Step], batch_max_iterations: usize) -> usize {
    steps
        .iter()
        .map(|step| match &step.0 {
            StepNode::Loop(body) => match &body.repeat {
                None => count_cpis(&body.steps) * batch_max_iterations,
                Some((_, max)) => count_cpis(&body.steps) * (*max as usize),
            },
            StepNode::Invoke(_) => 1,
            _ => 0,
        })
        .sum()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ------------------------------------------------------------------------------------ compiler

struct Location {
    path: String,
    label: Option<String>,
}

struct Compiler<'t> {
    template: &'t Template,
    input_indices: HashMap<&'t str, usize>,
    row_input_indices: HashMap<&'t str, usize>,
    fixed_indices: HashMap<&'t str, usize>,
    batch_indices: HashMap<&'t str, usize>,
    account_group_indices: HashMap<&'t str, usize>,
    /// The register each referenced fixed input was loaded into, before the first step.
    fixed_inputs: Vec<(String, Value)>,
    /// The register each distinct constant was materialized into, in insertion order.
    constants: Vec<(Literal, Value)>,
    pubkeys: Vec<[u8; 32]>,
    blob: Vec<u8>,
    account_records: Vec<[u8; 8]>,
    input_records: Vec<[u8; 4]>,
    instructions: Vec<Record>,
    cpis: Vec<[u8; 12]>,
    cpi_accounts: Vec<[u8; 2]>,
    data_segments: Vec<[u8; 8]>,
    /// Highest byte any fixed-offset read touches, per fixed (`false`) or row (`true`) account.
    required_data_length: HashMap<(bool, String), usize>,
    source_map: Vec<SourceMapEntry>,
    /// The registers each loop carries, by the loop's program counter.
    loop_carries: HashMap<usize, Vec<usize>>,
    registries: Vec<(String, RegistryLayout)>,
    opened_entries: HashSet<String>,
    keyed_entry: Option<String>,
    uses: HashMap<UseKey, usize>,
    location: Location,
    next_register: usize,
    max_cpi_data_length: usize,
    return_data_set: bool,
}

fn index_map<'a>(names: impl Iterator<Item = &'a str>) -> HashMap<&'a str, usize> {
    let mut map = HashMap::new();
    for (index, name) in names.enumerate() {
        map.entry(name).or_insert(index);
    }
    map
}

impl<'t> Compiler<'t> {
    fn new(template: &'t Template) -> Self {
        let empty: &'t [(String, Type)] = &[];
        let row_inputs = template
            .batch
            .as_ref()
            .map_or(empty, |batch| &batch.row_inputs[..]);
        let row: &'t [(String, Account)] =
            template.batch.as_ref().map_or(&[], |batch| &batch.row[..]);
        let mut uses = HashMap::new();
        walk_steps(&template.steps, &mut |expr| {
            let key = match &expr.0 {
                Node::Literal(literal) => UseKey::Literal(literal.clone()),
                Node::Input(name) => UseKey::Input(name.clone()),
                _ => return,
            };
            *uses.entry(key).or_insert(0) += 1;
        });
        let registries = template
            .registries
            .iter()
            .enumerate()
            .map(|(index, (name, fields))| {
                let mut offset = 0;
                let mut layout = RegistryLayout {
                    index,
                    size: 0,
                    fields: Vec::new(),
                };
                for (field, ty) in fields {
                    let ty = registry_read_type(*ty).unwrap_or(ReadType::U64);
                    layout
                        .fields
                        .push((field.clone(), RegistryField { offset, ty }));
                    offset += ty.width();
                }
                layout.size = offset;
                (name.clone(), layout)
            })
            .collect();
        Compiler {
            template,
            input_indices: index_map(template.inputs.iter().map(|(name, _)| name.as_str())),
            row_input_indices: index_map(row_inputs.iter().map(|(name, _)| name.as_str())),
            fixed_indices: index_map(template.accounts.iter().map(|(name, _)| name.as_str())),
            batch_indices: index_map(row.iter().map(|(name, _)| name.as_str())),
            account_group_indices: index_map(template.account_groups.iter().map(String::as_str)),
            fixed_inputs: Vec::new(),
            constants: Vec::new(),
            pubkeys: Vec::new(),
            blob: Vec::new(),
            account_records: Vec::new(),
            input_records: Vec::new(),
            instructions: Vec::new(),
            cpis: Vec::new(),
            cpi_accounts: Vec::new(),
            data_segments: Vec::new(),
            required_data_length: HashMap::new(),
            source_map: Vec::new(),
            loop_carries: HashMap::new(),
            registries,
            opened_entries: HashSet::new(),
            keyed_entry: None,
            uses,
            location: Location {
                path: "template".into(),
                label: None,
            },
            next_register: 0,
            max_cpi_data_length: 0,
            return_data_set: false,
        }
    }

    fn row_inputs(&self) -> &'t [(String, Type)] {
        self.template
            .batch
            .as_ref()
            .map_or(&[], |batch| &batch.row_inputs[..])
    }

    fn row(&self) -> &'t [(String, Account)] {
        self.template
            .batch
            .as_ref()
            .map_or(&[], |batch| &batch.row[..])
    }

    fn fixed_account(&self, name: &str) -> Option<&'t Account> {
        self.fixed_indices
            .get(name)
            .map(|&index| &self.template.accounts[index].1)
    }

    fn set_location(&mut self, path: String, label: Option<&str>) {
        self.location = Location {
            path,
            label: label.map(str::to_string),
        };
    }

    fn compile(mut self) -> Result<CompiledTemplate, CompileError> {
        let template = self.template;
        // Fixed inputs load once, before any step.
        let mut used = HashSet::new();
        walk_steps(&template.steps, &mut |expr| {
            if let Node::Input(name) = &expr.0 {
                used.insert(name.clone());
            }
        });
        for (index, (name, ty)) in template.inputs.iter().enumerate() {
            if !used.contains(name) || self.fixed_inputs.iter().any(|(other, _)| other == name) {
                continue;
            }
            self.set_location(format!("inputs.{name}"), None);
            let value = self.emit(
                OP_LOAD_INPUT,
                ty.value_type(),
                ty.max_length(),
                index as u8,
                NO_INDEX,
                NO_INDEX,
                0,
                0,
            )?;
            self.fixed_inputs.push((name.clone(), value));
        }
        // Constants are hoisted for the same reasons, and shared between uses.
        let mut literals: Vec<Literal> = Vec::new();
        walk_steps(&template.steps, &mut |expr| {
            if let Node::Literal(literal) = &expr.0 {
                if !literals.contains(literal) {
                    literals.push(literal.clone());
                }
            }
        });
        for (constant_index, literal) in literals.into_iter().enumerate() {
            self.set_location(format!("constants[{constant_index}]"), None);
            let value = self.emit_literal(&literal)?;
            self.constants.push((literal, value));
        }
        self.compile_registry_opens()?;
        self.set_location("template".into(), None);
        // Steps compile first so every static read has already raised its account's data floor.
        self.compile_steps(
            &template.steps,
            None,
            &mut Vec::new(),
            &HashSet::new(),
            "steps",
        )?;
        for (name, constraint) in &template.accounts {
            let floor = self
                .required_data_length
                .get(&(false, name.clone()))
                .copied()
                .unwrap_or(0);
            let record = self.compile_account_constraint(constraint, floor)?;
            self.account_records.push(record);
        }
        for (name, constraint) in self.row() {
            let floor = self
                .required_data_length
                .get(&(true, name.clone()))
                .copied()
                .unwrap_or(0);
            let record = self.compile_account_constraint(constraint, floor)?;
            self.account_records.push(record);
        }
        for (_, ty) in template.inputs.iter().chain(self.row_inputs()) {
            let mut record = [0u8; 4];
            record[0] = value_type_code(*ty);
            record[2..4].copy_from_slice(&(ty.max_length() as u16).to_le_bytes());
            self.input_records.push(record);
        }

        let registers = if self.next_register > MAX_REGISTERS {
            self.reuse_registers()?
        } else {
            self.next_register
        };
        if self.instructions.len() > MAX_VM_INSTRUCTIONS {
            return Err(error("Template uses more than 128 VM instructions"));
        }
        if self.cpis.len() > 0xff
            || self.cpi_accounts.len() > 0xffff
            || self.data_segments.len() > 0xffff
        {
            return Err(error("Template table count exceeds the wire format"));
        }
        if self.pubkeys.len() > 0xff || self.blob.len() > 0xffff {
            return Err(error("Template constants exceed the wire format"));
        }

        let opens = template
            .accounts
            .iter()
            .filter(|(_, constraint)| constraint.registry.is_some())
            .count();
        let batch_max = template
            .batch
            .as_ref()
            .map_or(0, |batch| batch.max_iterations as usize);
        let batch_min = template
            .batch
            .as_ref()
            .map_or(0, |batch| batch.min_iterations as usize);
        let max_expanded_cpis =
            worst_case_cpis(&template.steps, batch_max) + REGISTRY_OPEN_CPIS * opens;
        if max_expanded_cpis > MAX_EXPANDED_CPIS {
            return Err(error(format!(
                "Template can expand to {max_expanded_cpis} CPIs; maximum is 64"
            )));
        }

        let row = self.row();
        let row_inputs = self.row_inputs();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&TEMPLATE_PROGRAM_MAGIC);
        bytes.push(TEMPLATE_PROGRAM_VERSION);
        bytes.push(template.accounts.len() as u8);
        bytes.push(row.len() as u8);
        bytes.push(batch_max as u8);
        bytes.push(template.inputs.len() as u8);
        bytes.push(registers as u8);
        bytes.push(self.instructions.len() as u8);
        bytes.push(self.cpis.len() as u8);
        bytes.extend_from_slice(&(self.cpi_accounts.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&(self.data_segments.len() as u16).to_le_bytes());
        bytes.push(self.pubkeys.len() as u8);
        bytes.push(if template.emit_event {
            PROGRAM_FLAG_EMIT_EVENT
        } else {
            0
        });
        bytes.extend_from_slice(&(self.blob.len() as u16).to_le_bytes());
        bytes.push(batch_min as u8);
        bytes.push(row_inputs.len() as u8);
        bytes.push(template.account_groups.len() as u8);
        bytes.push(0);
        for record in &self.account_records {
            bytes.extend_from_slice(record);
        }
        for record in &self.input_records {
            bytes.extend_from_slice(record);
        }
        for record in &self.instructions {
            bytes.extend_from_slice(record);
        }
        for record in &self.cpis {
            bytes.extend_from_slice(record);
        }
        for record in &self.cpi_accounts {
            bytes.extend_from_slice(record);
        }
        for record in &self.data_segments {
            bytes.extend_from_slice(record);
        }
        for pubkey in &self.pubkeys {
            bytes.extend_from_slice(pubkey);
        }
        bytes.extend_from_slice(&self.blob);
        if bytes.len() > MAX_TEMPLATE_PAYLOAD_LEN {
            return Err(error(format!(
                "Compiled template is {} bytes; maximum is 10240",
                bytes.len()
            )));
        }

        let hash = solana_sha256_hasher::hash(&bytes).to_bytes();
        let names =
            |entries: &[(String, Type)]| entries.iter().map(|(name, _)| name.clone()).collect();
        let types = |entries: &[(String, Type)]| entries.iter().map(|(_, ty)| *ty).collect();
        Ok(CompiledTemplate {
            hash,
            input_order: names(&template.inputs),
            input_types: types(&template.inputs),
            row_input_order: names(row_inputs),
            row_input_types: types(row_inputs),
            fixed_account_order: template
                .accounts
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
            batch_account_order: row.iter().map(|(name, _)| name.clone()).collect(),
            account_group_order: template.account_groups.clone(),
            registry_order: self
                .registries
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
            stats: CompileStats {
                payload_bytes: bytes.len(),
                fixed_accounts: template.accounts.len(),
                batch_stride: row.len(),
                batch_max_iterations: batch_max,
                batch_min_iterations: batch_min,
                inputs: template.inputs.len(),
                registers,
                instructions: self.instructions.len(),
                cpis: self.cpis.len(),
                max_expanded_cpis,
                max_cpi_data_length: self.max_cpi_data_length,
                emit_event: template.emit_event,
                row_inputs: row_inputs.len(),
                account_groups: template.account_groups.len(),
            },
            source_map: self.source_map,
            bytes,
            template: template.clone(),
        })
    }

    fn compile_account_constraint(
        &mut self,
        constraint: &Account,
        inferred: usize,
    ) -> Result<[u8; 8], CompileError> {
        let mut record = [0u8; 8];
        record[0] = (if constraint.signer { ACCOUNT_SIGNER } else { 0 })
            | (if constraint.writable {
                ACCOUNT_WRITABLE
            } else {
                0
            })
            | (if constraint.executable {
                ACCOUNT_EXECUTABLE
            } else {
                0
            });
        record[1] = match constraint.address {
            Some(address) => self.add_pubkey(address)?,
            None => NO_INDEX,
        };
        record[2] = match constraint.owner {
            Some(owner) => self.add_pubkey(owner)?,
            None => NO_INDEX,
        };
        let floor = (constraint.min_data_length as usize).max(inferred);
        let floor = u32::try_from(floor).map_err(|_| error("Expected u32"))?;
        record[4..8].copy_from_slice(&floor.to_le_bytes());
        Ok(record)
    }

    /// One `OPEN_REGISTRY` per registry account, in declaration order, before the first step.
    fn compile_registry_opens(&mut self) -> Result<(), CompileError> {
        let template = self.template;
        let entries: Vec<&(String, Account)> = template
            .accounts
            .iter()
            .filter(|(_, constraint)| constraint.registry.is_some())
            .collect();
        if entries.is_empty() {
            return Ok(());
        }
        if entries.len() > MAX_REGISTRY_OPENS {
            return Err(error(format!(
                "A template opens at most {MAX_REGISTRY_OPENS} registry entries"
            )));
        }
        let Some(system_program) = template
            .accounts
            .iter()
            .position(|(_, constraint)| constraint.address == Some([0; 32]))
        else {
            return Err(error(
                "Registry accounts need a fixed account pinned to the System program: declare one with account::system_program()",
            ));
        };
        for (name, constraint) in entries {
            let registry = constraint.registry.as_ref().expect("filtered on registry");
            self.set_location(format!("accounts.{name}"), None);
            let Some((_, layout)) = self
                .registries
                .iter()
                .find(|(other, _)| *other == registry.name)
            else {
                return Err(error(format!("Unknown registry: {}", registry.name)));
            };
            let (layout_index, layout_size) = (layout.index, layout.size);
            if !constraint.writable
                || constraint.signer
                || constraint.executable
                || constraint.address.is_some()
                || constraint.owner.is_some()
                || constraint.min_data_length != 0
            {
                return Err(error(format!(
                    "{name} must be declared only writable: build it with account::registry"
                )));
            }
            let payer = self.fixed_account(&registry.payer);
            if !payer
                .is_some_and(|payer| payer.signer && payer.writable && payer.registry.is_none())
            {
                return Err(error(format!(
                    "{name}'s payer {} must be a fixed account declared signer and writable",
                    registry.payer
                )));
            }
            let mut key = NO_INDEX;
            if let Some(expression) = &registry.key {
                self.keyed_entry = Some(name.clone());
                let value = self.compile_expression(expression, None, &Vec::new())?;
                self.keyed_entry = None;
                require_type(&value, ValueType::Pubkey, &format!("{name}'s key"))?;
                key = value.register as u8;
            }
            let immediate =
                layout_index as u64 | ((layout_size as u64) << 8) | ((system_program as u64) << 24);
            self.push_instruction(instruction_record(
                OP_OPEN_REGISTRY,
                NO_INDEX,
                self.fixed_indices[name.as_str()] as u8,
                key,
                self.fixed_indices[registry.payer.as_str()] as u8,
                immediate,
                0,
            ));
            self.opened_entries.insert(name.clone());
        }
        Ok(())
    }

    /// The field `field` of the registry entry in fixed account `account`.
    fn registry_field(&self, account: &str, field: &str) -> Result<RegistryField, CompileError> {
        let Some(registry) = self
            .fixed_account(account)
            .and_then(|constraint| constraint.registry.as_ref())
        else {
            return Err(error(format!("{account} is not a registry account")));
        };
        self.registries
            .iter()
            .find(|(name, _)| *name == registry.name)
            .and_then(|(_, layout)| layout.fields.iter().find(|(name, _)| name == field))
            .map(|(_, found)| *found)
            .ok_or_else(|| error(format!("{} has no field {field}", registry.name)))
    }

    fn compile_steps(
        &mut self,
        steps: &[Step],
        loop_kind: Option<LoopKind>,
        bindings: &mut Bindings,
        carried: &HashSet<String>,
        path: &str,
    ) -> Result<(), CompileError> {
        let mut previous: Option<&Step> = None;
        for (index, current) in steps.iter().enumerate() {
            let step_path = format!("{path}[{index}]");
            self.set_location(step_path.clone(), current.label_text());
            match &current.0 {
                StepNode::Loop(body) => {
                    if loop_kind.is_some() {
                        return Err(error("Nested loops are not supported"));
                    }
                    let mut carry = 0u64;
                    let mut carried_names = HashSet::new();
                    let mut carried_registers = Vec::new();
                    for name in body.carry.iter().flatten() {
                        let Some(mut value) = binding(bindings, name).copied() else {
                            return Err(error(format!(
                                "Carried variable must be defined before the loop: {name}"
                            )));
                        };
                        if self.shares_register(name, value.register, bindings) {
                            value = self.emit(
                                OP_MOVE,
                                value.ty,
                                value.max_length,
                                value.register as u8,
                                NO_INDEX,
                                NO_INDEX,
                                0,
                                0,
                            )?;
                            bind(bindings, name, value);
                        }
                        carried_names.insert(name.clone());
                        carried_registers.push(value.register);
                        carry |= 1u64.checked_shl(value.register as u32).unwrap_or(0);
                    }
                    let mut count = NO_INDEX;
                    let mut max = NO_INDEX;
                    if let Some((count_expression, max_passes)) = &body.repeat {
                        let value = self.compile_expression(count_expression, None, bindings)?;
                        require_type(&value, ValueType::U64, "repeat count")?;
                        count = value.register as u8;
                        max = *max_passes as u8;
                    }
                    let (operation, kind, name) = match body.repeat {
                        Some(_) => (OP_REPEAT, LoopKind::Count, "repeat"),
                        None => (OP_FOREACH, LoopKind::Rows, "forEach"),
                    };
                    let loop_pc = self.push_instruction(instruction_record(
                        operation, NO_INDEX, 0, count, max, carry, 0,
                    ));
                    self.loop_carries.insert(loop_pc, carried_registers);
                    let body_start = self.instructions.len();
                    let mut inner = bindings.clone();
                    self.compile_steps(
                        &body.steps,
                        Some(kind),
                        &mut inner,
                        &carried_names,
                        &format!("{step_path}.steps"),
                    )?;
                    let body_length = self.instructions.len() - body_start;
                    if body_length == 0 || body_length > 0xff {
                        return Err(error(format!("Invalid {name} body length")));
                    }
                    self.instructions[loop_pc] = instruction_record(
                        operation,
                        NO_INDEX,
                        body_length as u8,
                        count,
                        max,
                        carry,
                        0,
                    );
                }
                StepNode::Let { name, value, .. } => {
                    if binding(bindings, name).is_some() {
                        return Err(error(format!("Variable already defined: {name}")));
                    }
                    let result = match &value.0 {
                        Node::ReturnData { offset, ty } => {
                            self.compile_return_data(*offset, *ty, previous)?
                        }
                        _ => self.compile_expression(value, loop_kind, bindings)?,
                    };
                    bindings.push((name.clone(), result));
                }
                StepNode::Assign { name, value, .. } => {
                    if loop_kind.is_none() {
                        return Err(error("assign is only valid inside a loop"));
                    }
                    let target = binding(bindings, name).copied();
                    let Some(target) = target.filter(|_| carried.contains(name)) else {
                        return Err(error(format!(
                            "assign target must be listed in the loop's carry: {name}"
                        )));
                    };
                    let result = self.compile_expression(value, loop_kind, bindings)?;
                    if result.ty != target.ty
                        || (result.ty == ValueType::Bytes && result.max_length != target.max_length)
                    {
                        return Err(error(format!(
                            "assign to {name} must keep its {} type and size",
                            target.ty.name()
                        )));
                    }
                    self.push_instruction(instruction_record(
                        OP_MOVE,
                        target.register as u8,
                        result.register as u8,
                        NO_INDEX,
                        NO_INDEX,
                        0,
                        0,
                    ));
                }
                StepNode::Require { condition, .. } => {
                    let mut conjuncts = Vec::new();
                    flatten_conjunction(condition, &mut conjuncts);
                    for conjunct in conjuncts {
                        let result = self.compile_expression(conjunct, loop_kind, bindings)?;
                        require_type(&result, ValueType::Bool, "require condition")?;
                        self.push_instruction(instruction_record(
                            OP_REQUIRE,
                            NO_INDEX,
                            result.register as u8,
                            NO_INDEX,
                            NO_INDEX,
                            0,
                            0,
                        ));
                    }
                }
                StepNode::Emit { parts, .. } => {
                    self.compile_output(true, parts, loop_kind, bindings)?
                }
                StepNode::SetReturnData { parts, .. } => {
                    self.compile_output(false, parts, loop_kind, bindings)?
                }
                StepNode::SetRegistry {
                    account,
                    field,
                    value,
                    ..
                } => {
                    let found = self.registry_field(account, field)?;
                    let result = self.compile_expression(value, loop_kind, bindings)?;
                    if result.ty != read_result_type(found.ty) {
                        return Err(error(format!(
                            "{account}.{field}: {field} is a {}, not a {}",
                            found.ty.name(),
                            result.ty.name()
                        )));
                    }
                    self.push_instruction(instruction_record(
                        OP_WRITE_REGISTRY,
                        NO_INDEX,
                        result.register as u8,
                        self.fixed_indices[account.as_str()] as u8,
                        NO_INDEX,
                        registry_field_immediate(found),
                        0,
                    ));
                }
                StepNode::Invoke(invoke) => self.compile_invoke(invoke, loop_kind, bindings)?,
                StepNode::Invalid(message) => return Err(error(message.clone())),
            }
            self.set_location(step_path, current.label_text());
            previous = Some(current);
        }
        Ok(())
    }

    /// Whether anything besides variable `name` reads `register`: another variable, or a hoisted
    /// constant or fixed input that appears more than once in the template.
    fn shares_register(&self, name: &str, register: usize, bindings: &Bindings) -> bool {
        if bindings
            .iter()
            .any(|(other, value)| other != name && value.register == register)
        {
            return true;
        }
        if let Some((literal, _)) = self
            .constants
            .iter()
            .find(|(_, value)| value.register == register)
        {
            return self
                .uses
                .get(&UseKey::Literal(literal.clone()))
                .copied()
                .unwrap_or(0)
                > 1;
        }
        if let Some((input, _)) = self
            .fixed_inputs
            .iter()
            .find(|(_, value)| value.register == register)
        {
            return self
                .uses
                .get(&UseKey::Input(input.clone()))
                .copied()
                .unwrap_or(0)
                > 1;
        }
        false
    }

    fn compile_invoke(
        &mut self,
        current: &Invoke,
        loop_kind: Option<LoopKind>,
        bindings: &Bindings,
    ) -> Result<(), CompileError> {
        if self.return_data_set {
            return Err(error(
                "invoke cannot follow setReturnData: invoking a program clears the return data",
            ));
        }
        let program_account = self.encode_account_reference(&current.program, loop_kind)?;
        let program_constraint = self.constraint_for(&current.program, loop_kind)?;
        require_pinned_program(&current.program, program_constraint, "Invoke program")?;
        if let (Some(expected), Some(pinned)) =
            (current.program_address, program_constraint.address)
        {
            if expected != pinned {
                return Err(error(format!(
                    "Invoke targets program {} but account {} pins {}",
                    to_hex(&expected),
                    current.program.name(),
                    to_hex(&pinned)
                )));
            }
        }
        if current.accounts.len() > MAX_CPI_ACCOUNTS {
            return Err(error("CPI passes more than 64 accounts"));
        }

        let account_start = self.cpi_accounts.len();
        for account in &current.accounts {
            let reference = self.encode_account_reference(&account.account, loop_kind)?;
            let constraint = self.constraint_for(&account.account, loop_kind)?;
            if account.signer && !constraint.signer {
                return Err(error("CPI signer is not required by its account schema"));
            }
            if account.writable && !constraint.writable {
                return Err(error("CPI writable account is not writable in its schema"));
            }
            if account.writable {
                if let AccountRef::Fixed(name) = &account.account {
                    if self
                        .fixed_account(name)
                        .is_some_and(|fixed| fixed.registry.is_some())
                    {
                        return Err(error(format!(
                            "{name} is a registry entry: a CPI that passes it writable fails with RegistryReentry"
                        )));
                    }
                }
            }
            self.cpi_accounts.push([
                reference,
                (if account.signer { ACCOUNT_SIGNER } else { 0 })
                    | (if account.writable {
                        ACCOUNT_WRITABLE
                    } else {
                        0
                    }),
            ]);
        }

        let (segment_start, max_data_length) =
            self.compile_data_parts(&current.data, loop_kind, bindings)?;
        if max_data_length > MAX_CPI_DATA_LEN {
            return Err(error("CPI data can exceed 4096 bytes"));
        }
        self.max_cpi_data_length = self.max_cpi_data_length.max(max_data_length);

        let mut account_group = NO_INDEX;
        if let Some(group) = &current.account_group {
            let Some(&index) = self.account_group_indices.get(group.as_str()) else {
                return Err(error(format!("Unknown account group: {group}")));
            };
            account_group = index as u8;
        }
        let mut descriptor = [0u8; 12];
        descriptor[0] = program_account;
        descriptor[1] = account_group;
        descriptor[2..4].copy_from_slice(&(account_start as u16).to_le_bytes());
        descriptor[4] = current.accounts.len() as u8;
        descriptor[5] = current.data.len() as u8;
        descriptor[6..8].copy_from_slice(&(segment_start as u16).to_le_bytes());
        descriptor[8..10].copy_from_slice(&(max_data_length as u16).to_le_bytes());
        let cpi_index = self.cpis.len();
        self.cpis.push(descriptor);

        let mut guard = NO_INDEX;
        if let Some(when) = &current.when {
            let result = self.compile_expression(when, loop_kind, bindings)?;
            require_type(&result, ValueType::Bool, "invoke guard")?;
            guard = result.register as u8;
        }
        self.push_instruction(instruction_record(
            OP_INVOKE,
            NO_INDEX,
            cpi_index as u8,
            guard,
            NO_INDEX,
            0,
            0,
        ));
        Ok(())
    }

    /// EMIT and SET_RETURN_DATA: the parts are encoded as invocation data is, to at most 1,024
    /// bytes.
    fn compile_output(
        &mut self,
        emit: bool,
        parts: &[DataPart],
        loop_kind: Option<LoopKind>,
        bindings: &Bindings,
    ) -> Result<(), CompileError> {
        let kind = if emit { "emit" } else { "setReturnData" };
        if emit {
            let tag = match parts.first().map(|part| &part.0) {
                Some(DataNode::Literal(bytes)) if bytes.len() >= MIN_EMIT_TAG_LEN => bytes,
                _ => {
                    return Err(error(format!(
                        "emit must start with a literal tag of at least {MIN_EMIT_TAG_LEN} bytes, so its log cannot pass for Ballista's run event"
                    )))
                }
            };
            if tag.starts_with(&RUN_EVENT_TAG_FAMILY) {
                return Err(error(
                    "emit tag cannot start with \"BEV\": that tag family is reserved for Ballista's run event",
                ));
            }
        } else {
            if loop_kind.is_some() {
                return Err(error("setReturnData is not allowed inside a loop"));
            }
            if self.return_data_set {
                return Err(error("setReturnData may appear only once"));
            }
            self.return_data_set = true;
        }
        let (segment_start, max_length) = self.compile_data_parts(parts, loop_kind, bindings)?;
        if max_length > MAX_RETURN_DATA_LEN {
            return Err(error(format!(
                "{kind} can encode {max_length} bytes; maximum is {MAX_RETURN_DATA_LEN}"
            )));
        }
        let operation = if emit { OP_EMIT } else { OP_SET_RETURN_DATA };
        self.push_instruction(instruction_record(
            operation,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            range_immediate(segment_start, parts.len()),
            0,
        ));
        Ok(())
    }

    fn compile_return_data(
        &mut self,
        offset: i64,
        ty: ReadType,
        previous: Option<&Step>,
    ) -> Result<Value, CompileError> {
        let after_unconditional_invoke = matches!(previous.map(|step| &step.0), Some(StepNode::Invoke(invoke)) if invoke.when.is_none());
        if !after_unconditional_invoke {
            return Err(error(
                "returnData must be the value of a let step directly after an unconditional invoke",
            ));
        }
        if offset as usize + ty.width() > MAX_RETURN_DATA_LEN {
            return Err(error(format!(
                "returnData read extends past {MAX_RETURN_DATA_LEN} bytes"
            )));
        }
        self.emit(
            OP_RETURN_DATA,
            read_result_type(ty),
            0,
            read_opcode(ty),
            NO_INDEX,
            NO_INDEX,
            offset as u64,
            0,
        )
    }

    /// Compiles a step's data parts, then appends their segments as one contiguous run and
    /// returns where it starts, and the most bytes they encode.
    fn compile_data_parts(
        &mut self,
        parts: &[DataPart],
        loop_kind: Option<LoopKind>,
        bindings: &Bindings,
    ) -> Result<(usize, usize), CompileError> {
        let mut compiled = Vec::with_capacity(parts.len());
        for part in parts {
            compiled.push(self.compile_data_part(part, loop_kind, bindings)?);
        }
        let segment_start = self.data_segments.len();
        let mut total = 0;
        for (record, max_length) in compiled {
            self.data_segments.push(record);
            total += max_length;
        }
        Ok((segment_start, total))
    }

    fn compile_data_part(
        &mut self,
        part: &DataPart,
        loop_kind: Option<LoopKind>,
        bindings: &Bindings,
    ) -> Result<([u8; 8], usize), CompileError> {
        let (encoding, value) = match &part.0 {
            DataNode::Literal(bytes) => {
                let offset = self.add_blob(bytes)?;
                return Ok((
                    segment_record(DATA_LITERAL, NO_INDEX, offset as u16, bytes.len() as u16),
                    bytes.len(),
                ));
            }
            DataNode::Encoded(encoding, value) => (*encoding, value),
        };
        let result = self.compile_expression(value, loop_kind, bindings)?;
        let context = format!("{} encoding", encoding.name());
        match encoding {
            Encoding::U8 | Encoding::U16 | Encoding::U32 | Encoding::U64 => {
                if !is_unsigned(result.ty) {
                    return Err(error(format!(
                        "{} encoding requires u64 or u128",
                        encoding.name()
                    )));
                }
            }
            Encoding::I64 => require_type(&result, ValueType::I64, &context)?,
            Encoding::U128 => require_type(&result, ValueType::U128, &context)?,
            Encoding::Pubkey => require_type(&result, ValueType::Pubkey, &context)?,
            Encoding::Bool => require_type(&result, ValueType::Bool, &context)?,
            Encoding::Bytes => require_type(&result, ValueType::Bytes, &context)?,
        }
        let (kind, max_length) = match encoding {
            Encoding::U8 => (DATA_REG_U8, 1),
            Encoding::U16 => (DATA_REG_U16, 2),
            Encoding::U32 => (DATA_REG_U32, 4),
            Encoding::U64 => (DATA_REG_U64, 8),
            Encoding::I64 => (DATA_REG_I64, 8),
            Encoding::U128 => (DATA_REG_U128, 16),
            Encoding::Pubkey => (DATA_REG_PUBKEY, 32),
            Encoding::Bool => (DATA_REG_BOOL, 1),
            Encoding::Bytes => (DATA_REG_BYTES, result.max_length),
        };
        Ok((
            segment_record(kind, result.register as u8, 0, 0),
            max_length,
        ))
    }

    fn compile_expression(
        &mut self,
        current: &Expr,
        loop_kind: Option<LoopKind>,
        bindings: &Bindings,
    ) -> Result<Value, CompileError> {
        match &current.0 {
            Node::Input(name) => {
                if let Some((_, loaded)) = self.fixed_inputs.iter().find(|(other, _)| other == name)
                {
                    return Ok(*loaded);
                }
                let Some(&index) = self.input_indices.get(name.as_str()) else {
                    return Err(error(format!("Unknown input: {name}")));
                };
                let ty = self.template.inputs[index].1;
                self.emit(
                    OP_LOAD_INPUT,
                    ty.value_type(),
                    ty.max_length(),
                    index as u8,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::RowInput(name) => {
                if loop_kind != Some(LoopKind::Rows) {
                    return Err(error("Row inputs are only valid inside forEach"));
                }
                let Some(&index) = self.row_input_indices.get(name.as_str()) else {
                    return Err(error(format!("Unknown row input: {name}")));
                };
                let ty = self.row_inputs()[index].1;
                self.emit(
                    OP_LOAD_INPUT,
                    ty.value_type(),
                    ty.max_length(),
                    ITERATION_ACCOUNT_BIT | index as u8,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::Variable(name) => binding(bindings, name)
                .copied()
                .ok_or_else(|| error(format!("Unknown variable: {name}"))),
            Node::Literal(literal) => {
                if let Some((_, loaded)) = self.constants.iter().find(|(other, _)| other == literal)
                {
                    return Ok(*loaded);
                }
                let value = self.emit_literal(literal)?;
                self.constants.push((literal.clone(), value));
                Ok(value)
            }
            Node::AccountField(account, field) => {
                let reference = self.encode_account_reference(account, loop_kind)?;
                let (operation, ty) = match field {
                    AccountField::Key => (OP_ACCOUNT_KEY, ValueType::Pubkey),
                    AccountField::Owner => (OP_ACCOUNT_OWNER, ValueType::Pubkey),
                    AccountField::Lamports => (OP_ACCOUNT_LAMPORTS, ValueType::U64),
                    AccountField::DataLength => (OP_ACCOUNT_DATA_LEN, ValueType::U64),
                    AccountField::IsEmpty => (OP_ACCOUNT_IS_EMPTY, ValueType::Bool),
                };
                self.emit(operation, ty, 0, reference, NO_INDEX, NO_INDEX, 0, 0)
            }
            Node::AccountData {
                account,
                offset,
                ty,
            } => {
                let reference = self.encode_account_reference(account, loop_kind)?;
                let constraint = self.constraint_for(account, loop_kind)?;
                refuse_registry_data(account, constraint)?;
                require_pinned_for_read(account, constraint)?;
                let operation = read_opcode(*ty);
                let result_type = read_result_type(*ty);
                match offset.as_ref() {
                    Offset::Static(offset) => {
                        let offset = *offset as usize;
                        self.raise_data_floor(account, offset + ty.width());
                        self.emit(
                            operation,
                            result_type,
                            0,
                            reference,
                            NO_INDEX,
                            NO_INDEX,
                            offset as u64,
                            0,
                        )
                    }
                    Offset::Dynamic(offset) => {
                        let offset = self.compile_expression(offset, loop_kind, bindings)?;
                        require_type(&offset, ValueType::U64, "accountData offset")?;
                        self.emit(
                            operation,
                            result_type,
                            0,
                            reference,
                            offset.register as u8,
                            NO_INDEX,
                            0,
                            INSTRUCTION_FLAG_DYNAMIC_OFFSET,
                        )
                    }
                }
            }
            Node::ReturnData { .. } => Err(error(
                "returnData must be the value of a let step directly after an unconditional invoke",
            )),
            Node::ClockSlot => self.emit(
                OP_CLOCK_SLOT,
                ValueType::U64,
                0,
                NO_INDEX,
                NO_INDEX,
                NO_INDEX,
                0,
                0,
            ),
            Node::ClockUnixTimestamp => self.emit(
                OP_CLOCK_TIMESTAMP,
                ValueType::I64,
                0,
                NO_INDEX,
                NO_INDEX,
                NO_INDEX,
                0,
                0,
            ),
            Node::LoopIndex => {
                if loop_kind.is_none() {
                    return Err(error("loopIndex is only valid inside a loop"));
                }
                self.emit(
                    OP_LOOP_INDEX,
                    ValueType::U64,
                    0,
                    NO_INDEX,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::Pda {
                program,
                seeds,
                bump,
            } => {
                let program_account = self.encode_account_reference(program, loop_kind)?;
                let program_constraint = self.constraint_for(program, loop_kind)?;
                require_pinned_program(program, program_constraint, "PDA program")?;
                if seeds.is_empty() || seeds.len() > MAX_PDA_SEEDS {
                    return Err(error(format!(
                        "PDA derivation requires 1 to {MAX_PDA_SEEDS} seeds"
                    )));
                }
                let mut bump_register = NO_INDEX;
                if let Some(bump) = bump {
                    let value = self.compile_expression(bump, loop_kind, bindings)?;
                    require_type(&value, ValueType::U64, "PDA bump")?;
                    bump_register = value.register as u8;
                }
                let mut values = Vec::with_capacity(seeds.len());
                for seed in seeds {
                    let value = self.compile_expression(seed, loop_kind, bindings)?;
                    let seed_length = if value.ty == ValueType::Bytes {
                        value.max_length
                    } else {
                        fixed_value_length(value.ty)
                    };
                    if seed_length > MAX_PDA_SEED_LEN {
                        return Err(error(format!(
                            "PDA seed can exceed {MAX_PDA_SEED_LEN} bytes"
                        )));
                    }
                    values.push(value);
                }
                let segment_start = self.data_segments.len();
                for value in &values {
                    self.data_segments.push(seed_segment(value));
                }
                let operation = if bump.is_none() {
                    OP_DERIVE_PDA
                } else {
                    OP_CREATE_PDA
                };
                self.emit(
                    operation,
                    ValueType::Pubkey,
                    0,
                    program_account,
                    bump_register,
                    NO_INDEX,
                    range_immediate(segment_start, seeds.len()),
                    0,
                )
            }
            Node::Not(value) => {
                let value = self.compile_expression(value, loop_kind, bindings)?;
                require_type(&value, ValueType::Bool, "not")?;
                self.emit(
                    OP_NOT,
                    ValueType::Bool,
                    0,
                    value.register as u8,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::MultiplyDivide {
                left,
                right,
                divisor,
                round_up,
            } => {
                let left = self.compile_expression(left, loop_kind, bindings)?;
                let right = self.compile_expression(right, loop_kind, bindings)?;
                let divisor = self.compile_expression(divisor, loop_kind, bindings)?;
                if left.ty != right.ty || left.ty != divisor.ty || !is_unsigned(left.ty) {
                    return Err(error(
                        "multiplyDivide requires three u64 or three u128 operands",
                    ));
                }
                let operation = if *round_up {
                    OP_MUL_DIV_CEIL
                } else {
                    OP_MUL_DIV
                };
                self.emit(
                    operation,
                    left.ty,
                    0,
                    left.register as u8,
                    right.register as u8,
                    divisor.register as u8,
                    0,
                    0,
                )
            }
            Node::PowerOfTen(exponent) => {
                let exponent = self.compile_expression(exponent, loop_kind, bindings)?;
                require_type(&exponent, ValueType::U64, "powerOfTen")?;
                self.emit(
                    OP_POW10,
                    ValueType::U128,
                    0,
                    exponent.register as u8,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::Cast { to, value } => {
                let value = self.compile_expression(value, loop_kind, bindings)?;
                if !is_numeric(value.ty) {
                    return Err(error("cast requires a numeric expression"));
                }
                let (operation, ty) = match to {
                    CastType::U64 => (OP_CAST_U64, ValueType::U64),
                    CastType::I64 => (OP_CAST_I64, ValueType::I64),
                    CastType::U128 => (OP_CAST_U128, ValueType::U128),
                };
                self.emit(
                    operation,
                    ty,
                    0,
                    value.register as u8,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::InstructionCount(sysvar) | Node::CurrentInstructionIndex(sysvar) => {
                let sysvar_account = self.encode_sysvar(sysvar)?;
                let operation = if matches!(current.0, Node::InstructionCount(_)) {
                    OP_INSTRUCTION_COUNT
                } else {
                    OP_INSTRUCTION_INDEX
                };
                self.emit(
                    operation,
                    ValueType::U64,
                    0,
                    sysvar_account,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::Instruction {
                sysvar,
                index,
                field,
            } => {
                let sysvar_account = self.encode_sysvar(sysvar)?;
                let index = self.compile_expression(index, loop_kind, bindings)?;
                require_type(&index, ValueType::U64, "instruction index")?;
                let (operation, ty) = match field {
                    InstructionField::Program => (OP_INSTRUCTION_PROGRAM, ValueType::Pubkey),
                    InstructionField::AccountCount => {
                        (OP_INSTRUCTION_ACCOUNT_COUNT, ValueType::U64)
                    }
                    InstructionField::DataLength => (OP_INSTRUCTION_DATA_LEN, ValueType::U64),
                };
                self.emit(
                    operation,
                    ty,
                    0,
                    sysvar_account,
                    index.register as u8,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::InstructionAccount {
                sysvar,
                index,
                position,
                flags,
            } => {
                let sysvar_account = self.encode_sysvar(sysvar)?;
                let index = self.compile_expression(index, loop_kind, bindings)?;
                let position = self.compile_expression(position, loop_kind, bindings)?;
                require_type(&index, ValueType::U64, "instruction index")?;
                require_type(&position, ValueType::U64, "instruction account position")?;
                let (operation, ty) = if *flags {
                    (OP_INSTRUCTION_ACCOUNT_FLAGS, ValueType::U64)
                } else {
                    (OP_INSTRUCTION_ACCOUNT, ValueType::Pubkey)
                };
                self.emit(
                    operation,
                    ty,
                    0,
                    sysvar_account,
                    index.register as u8,
                    position.register as u8,
                    0,
                    0,
                )
            }
            Node::InstructionData {
                sysvar,
                index,
                offset,
                ty,
            } => {
                let sysvar_account = self.encode_sysvar(sysvar)?;
                let index = self.compile_expression(index, loop_kind, bindings)?;
                let offset = self.compile_expression(offset, loop_kind, bindings)?;
                require_type(&index, ValueType::U64, "instruction index")?;
                require_type(&offset, ValueType::U64, "instructionData offset")?;
                self.emit(
                    OP_READ_INSTRUCTION_DATA,
                    read_result_type(*ty),
                    0,
                    sysvar_account,
                    index.register as u8,
                    offset.register as u8,
                    u64::from(read_opcode(*ty)),
                    0,
                )
            }
            Node::InstructionDataBytes {
                sysvar,
                index,
                offset,
                length,
            } => {
                let sysvar_account = self.encode_sysvar(sysvar)?;
                let index = self.compile_expression(index, loop_kind, bindings)?;
                let offset = self.compile_expression(offset, loop_kind, bindings)?;
                require_type(&index, ValueType::U64, "instruction index")?;
                require_type(&offset, ValueType::U64, "instructionDataBytes offset")?;
                self.emit(
                    OP_READ_INSTRUCTION_BYTES,
                    ValueType::Bytes,
                    *length,
                    sysvar_account,
                    index.register as u8,
                    offset.register as u8,
                    *length as u64,
                    0,
                )
            }
            Node::AccountDataBytes {
                account,
                offset,
                length,
            } => {
                let reference = self.encode_account_reference(account, loop_kind)?;
                let constraint = self.constraint_for(account, loop_kind)?;
                refuse_registry_data(account, constraint)?;
                require_pinned_for_read(account, constraint)?;
                if constraint.writable {
                    return Err(error(format!(
                        "accountDataBytes reads only accounts this instruction cannot write; {} is declared writable",
                        account.name()
                    )));
                }
                let offset = self.compile_expression(offset, loop_kind, bindings)?;
                require_type(&offset, ValueType::U64, "accountDataBytes offset")?;
                self.emit(
                    OP_READ_ACCOUNT_BYTES,
                    ValueType::Bytes,
                    *length,
                    reference,
                    offset.register as u8,
                    NO_INDEX,
                    *length as u64,
                    0,
                )
            }
            Node::BytesLength(value) => {
                let value = self.compile_expression(value, loop_kind, bindings)?;
                require_type(&value, ValueType::Bytes, "bytesLength")?;
                self.emit(
                    OP_BYTES_LEN,
                    ValueType::U64,
                    0,
                    value.register as u8,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::Registry { account, field } => {
                let found = self.registry_field(account, field)?;
                if !self.opened_entries.contains(account) {
                    let keyed = self.keyed_entry.clone().unwrap_or_else(|| account.clone());
                    return Err(error(if *account == keyed {
                        format!("{keyed}'s key reads its own entry, which opens only once its key is known")
                    } else {
                        format!(
                            "{keyed}'s key reads {account}, whose entry opens after {keyed}'s: declare {account} before {keyed}"
                        )
                    }));
                }
                self.emit(
                    OP_READ_REGISTRY,
                    read_result_type(found.ty),
                    0,
                    self.fixed_indices[account.as_str()] as u8,
                    NO_INDEX,
                    NO_INDEX,
                    registry_field_immediate(found),
                    0,
                )
            }
            Node::GroupLength(group) => {
                let group = self.group_index(group)?;
                self.emit(
                    OP_GROUP_LENGTH,
                    ValueType::U64,
                    0,
                    group,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Node::GroupFilter {
                group,
                count,
                filter,
            } => self.compile_group_filter(group, *count, filter, loop_kind, bindings),
            Node::Select {
                condition,
                if_true,
                if_false,
            } => {
                let condition = self.compile_expression(condition, loop_kind, bindings)?;
                let if_true = self.compile_expression(if_true, loop_kind, bindings)?;
                let if_false = self.compile_expression(if_false, loop_kind, bindings)?;
                require_type(&condition, ValueType::Bool, "select condition")?;
                require_type(&if_false, if_true.ty, "select branches")?;
                self.emit(
                    OP_SELECT,
                    if_true.ty,
                    if_true.max_length.max(if_false.max_length),
                    condition.register as u8,
                    if_true.register as u8,
                    if_false.register as u8,
                    0,
                    0,
                )
            }
            Node::Binary { op, left, right } => {
                let left = self.compile_expression(left, loop_kind, bindings)?;
                let right = self.compile_expression(right, loop_kind, bindings)?;
                self.compile_binary(*op, left, right)
            }
            Node::Invalid(message) => Err(error(message.clone())),
        }
    }

    fn group_index(&self, group: &str) -> Result<u8, CompileError> {
        self.account_group_indices
            .get(group)
            .map(|&index| index as u8)
            .ok_or_else(|| error(format!("Unknown account group: {group}")))
    }

    /// `GROUP_ANY` or `GROUP_COUNT`, as `compileGroupFilter`: every match value and except key
    /// compiles before the filter's segments are appended, matches first, then the programs are
    /// interned.
    fn compile_group_filter(
        &mut self,
        group: &str,
        count: bool,
        filter: &GroupFilter,
        loop_kind: Option<LoopKind>,
        bindings: &Bindings,
    ) -> Result<Value, CompileError> {
        let kind = if count { "groupCount" } else { "groupAny" };
        let group = self.group_index(group)?;
        let mut matches = Vec::with_capacity(filter.matches.len());
        for (offset, equals) in &filter.matches {
            let value = self.compile_expression(equals, loop_kind, bindings)?;
            if value.ty == ValueType::Bytes {
                return Err(error(format!(
                    "{kind} match at offset {offset}: a match value is a bool, u64, i64, u128 or pubkey, not bytes"
                )));
            }
            matches.push((*offset, value));
        }
        let mut excepts = Vec::with_capacity(filter.except_keys.len());
        for key in &filter.except_keys {
            let value = self.compile_expression(key, loop_kind, bindings)?;
            require_type(&value, ValueType::Pubkey, &format!("{kind} exceptKeys"))?;
            excepts.push(value);
        }
        let floor = matches
            .iter()
            .map(|(offset, value)| *offset as usize + fixed_value_length(value.ty))
            .max()
            .unwrap_or(0);
        let min_data_length = filter
            .min_data_length
            .map_or(floor, |length| length as usize);
        if min_data_length < floor {
            return Err(error(format!(
                "{kind}: minDataLength {min_data_length} is shorter than the matches, which read to byte {floor}"
            )));
        }
        let segment_start = self.data_segments.len();
        for (offset, value) in &matches {
            let mut record = seed_segment(value);
            record[2..4].copy_from_slice(&offset.to_le_bytes());
            self.data_segments.push(record);
        }
        for value in &excepts {
            self.data_segments.push(seed_segment(value));
        }
        let first = self.add_pubkey(filter.programs[0])?;
        let second = match filter.programs.get(1) {
            Some(program) => self.add_pubkey(*program)?,
            None => NO_INDEX,
        };
        let immediate = segment_start as u64
            | (matches.len() as u64) << 16
            | (excepts.len() as u64) << 24
            | (min_data_length as u64) << 32;
        let (operation, ty) = if count {
            (OP_GROUP_COUNT, ValueType::U64)
        } else {
            (OP_GROUP_ANY, ValueType::Bool)
        };
        self.emit(operation, ty, 0, group, first, second, immediate, 0)
    }

    fn compile_binary(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
    ) -> Result<Value, CompileError> {
        let operation = match op {
            BinaryOp::Add => OP_ADD,
            BinaryOp::Subtract => OP_SUB,
            BinaryOp::Multiply => OP_MUL,
            BinaryOp::Divide => OP_DIV,
            BinaryOp::Min => OP_MIN,
            BinaryOp::Max => OP_MAX,
            BinaryOp::Equal => OP_EQ,
            BinaryOp::NotEqual => OP_NE,
            BinaryOp::LessThan => OP_LT,
            BinaryOp::LessThanOrEqual => OP_LTE,
            BinaryOp::GreaterThan => OP_GT,
            BinaryOp::GreaterThanOrEqual => OP_GTE,
            BinaryOp::And => OP_AND,
            BinaryOp::Or => OP_OR,
            BinaryOp::Remainder => OP_REM,
            BinaryOp::ShiftLeft => OP_SHL,
            BinaryOp::ShiftRight => OP_SHR,
            BinaryOp::BitAnd => OP_BIT_AND,
            BinaryOp::BitOr => OP_BIT_OR,
            BinaryOp::BitXor => OP_BIT_XOR,
        };
        let name = op.name();
        let (a, b) = (left.register as u8, right.register as u8);
        match op {
            BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
                if !is_unsigned(left.ty) {
                    return Err(error(format!("{name} requires a u64 or u128 value")));
                }
                require_type(&right, ValueType::U64, &format!("{name} amount"))?;
                self.emit(operation, left.ty, 0, a, b, NO_INDEX, 0, 0)
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                if left.ty != right.ty || !is_unsigned(left.ty) {
                    return Err(error(format!(
                        "{name} requires matching u64 or u128 operands"
                    )));
                }
                self.emit(operation, left.ty, 0, a, b, NO_INDEX, 0, 0)
            }
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Min
            | BinaryOp::Max
            | BinaryOp::Remainder => {
                if left.ty != right.ty || !is_numeric(left.ty) {
                    return Err(error(format!("{name} requires matching numeric types")));
                }
                self.emit(operation, left.ty, 0, a, b, NO_INDEX, 0, 0)
            }
            BinaryOp::And | BinaryOp::Or => {
                require_type(&left, ValueType::Bool, name)?;
                require_type(&right, ValueType::Bool, name)?;
                self.emit(operation, ValueType::Bool, 0, a, b, NO_INDEX, 0, 0)
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::LessThan
            | BinaryOp::LessThanOrEqual
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterThanOrEqual => {
                if left.ty != right.ty {
                    return Err(error(format!("{name} requires matching types")));
                }
                if !matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) && !is_numeric(left.ty) {
                    return Err(error(format!("{name} requires numeric operands")));
                }
                self.emit(operation, ValueType::Bool, 0, a, b, NO_INDEX, 0, 0)
            }
        }
    }

    /// Introspection reads the Instructions sysvar through a fixed account pinned to its address.
    fn encode_sysvar(&mut self, reference: &AccountRef) -> Result<u8, CompileError> {
        let pinned = match reference {
            AccountRef::Fixed(_) => {
                self.constraint_for(reference, None)?.address == Some(INSTRUCTIONS_SYSVAR_ID)
            }
            AccountRef::Iteration(_) => false,
        };
        if !pinned {
            return Err(error(format!(
                "Account {} must be a fixed account pinned to the Instructions sysvar (INSTRUCTIONS_SYSVAR_ID)",
                reference.name()
            )));
        }
        self.encode_account_reference(reference, None)
    }

    fn raise_data_floor(&mut self, reference: &AccountRef, end: usize) {
        let key = match reference {
            AccountRef::Fixed(name) => (false, name.clone()),
            AccountRef::Iteration(name) => (true, name.clone()),
        };
        let floor = self.required_data_length.entry(key).or_insert(0);
        *floor = (*floor).max(end);
    }

    fn push_instruction(&mut self, record: Record) -> usize {
        let pc = self.instructions.len();
        self.instructions.push(record);
        self.source_map.push(SourceMapEntry {
            pc,
            path: self.location.path.clone(),
            label: self.location.label.clone(),
        });
        pc
    }

    /// Renumbers the registers of a template whose values need more than 64, as the TypeScript
    /// compiler's `reuseRegisters`, and returns how many it needs after reuse.
    fn reuse_registers(&mut self) -> Result<usize, CompileError> {
        let original = reuse::Program {
            instructions: self.instructions.clone(),
            cpis: self.cpis.clone(),
            segments: self.data_segments.clone(),
            loops: reuse::loop_spans(&self.instructions, &self.loop_carries),
        };
        let (first, last) = reuse::live_ranges(&original, self.next_register);
        let assigned = reuse::assign_registers(&first, &last);
        let count = assigned.iter().copied().max().map_or(0, |max| max + 1);
        if count > MAX_REGISTERS {
            let busiest = reuse::busiest_instruction(&first, &last, &self.source_map);
            let entry = &self.source_map[busiest];
            let label = entry
                .label
                .as_ref()
                .map(|label| format!(" ({label})"))
                .unwrap_or_default();
            return Err(error(format!(
                "Template uses more than 64 registers: {count} values are in use at once at {}{label}",
                entry.path
            )));
        }
        let renamed = reuse::rename_registers(&original, &assigned)?;
        for runs in 0..(1usize << original.loops.len()) {
            if reuse::values_read(&original, runs) != reuse::values_read(&renamed, runs) {
                return Err(error(
                    "Register reuse changed what a read sees. This is a compiler bug: please report the template.",
                ));
            }
        }
        self.instructions = renamed.instructions;
        self.data_segments = renamed.segments;
        Ok(count)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        operation: u8,
        ty: ValueType,
        max_length: usize,
        a: u8,
        b: u8,
        c: u8,
        immediate: u64,
        flags: u8,
    ) -> Result<Value, CompileError> {
        let register = self.next_register;
        self.next_register += 1;
        if register >= MAX_VM_INSTRUCTIONS {
            return Err(error("Template uses more than 128 VM instructions"));
        }
        self.push_instruction(instruction_record(
            operation,
            register as u8,
            a,
            b,
            c,
            immediate,
            flags,
        ));
        Ok(Value {
            register,
            ty,
            max_length,
        })
    }

    fn emit_literal(&mut self, literal: &Literal) -> Result<Value, CompileError> {
        match literal {
            Literal::Bool(value) => self.emit(
                OP_CONST_BOOL,
                ValueType::Bool,
                0,
                u8::from(*value),
                NO_INDEX,
                NO_INDEX,
                0,
                0,
            ),
            Literal::U64(value) => self.emit(
                OP_CONST_U64,
                ValueType::U64,
                0,
                NO_INDEX,
                NO_INDEX,
                NO_INDEX,
                *value,
                0,
            ),
            Literal::I64(value) => self.emit(
                OP_CONST_I64,
                ValueType::I64,
                0,
                NO_INDEX,
                NO_INDEX,
                NO_INDEX,
                *value as u64,
                0,
            ),
            Literal::U128(value) => {
                let offset = self.add_blob(&value.to_le_bytes())?;
                self.emit(
                    OP_CONST_U128,
                    ValueType::U128,
                    0,
                    NO_INDEX,
                    NO_INDEX,
                    NO_INDEX,
                    range_immediate(offset, 16),
                    0,
                )
            }
            Literal::Pubkey(value) => {
                let index = self.add_pubkey(*value)?;
                self.emit(
                    OP_CONST_PUBKEY,
                    ValueType::Pubkey,
                    0,
                    index,
                    NO_INDEX,
                    NO_INDEX,
                    0,
                    0,
                )
            }
            Literal::Bytes(value) => {
                let offset = self.add_blob(value)?;
                self.emit(
                    OP_CONST_BYTES,
                    ValueType::Bytes,
                    value.len(),
                    NO_INDEX,
                    NO_INDEX,
                    NO_INDEX,
                    range_immediate(offset, value.len()),
                    0,
                )
            }
        }
    }

    fn encode_account_reference(
        &self,
        reference: &AccountRef,
        loop_kind: Option<LoopKind>,
    ) -> Result<u8, CompileError> {
        match reference {
            AccountRef::Fixed(name) => self
                .fixed_indices
                .get(name.as_str())
                .map(|&index| index as u8)
                .ok_or_else(|| error(format!("Unknown fixed account: {name}"))),
            AccountRef::Iteration(name) => {
                if loop_kind != Some(LoopKind::Rows) {
                    return Err(error("Iteration accounts are only valid inside forEach"));
                }
                self.batch_indices
                    .get(name.as_str())
                    .map(|&index| ITERATION_ACCOUNT_BIT | index as u8)
                    .ok_or_else(|| error(format!("Unknown batch account: {name}")))
            }
        }
    }

    fn constraint_for(
        &self,
        reference: &AccountRef,
        loop_kind: Option<LoopKind>,
    ) -> Result<&'t Account, CompileError> {
        match reference {
            AccountRef::Fixed(name) => self
                .fixed_account(name)
                .ok_or_else(|| error(format!("Unknown fixed account: {name}"))),
            AccountRef::Iteration(name) => {
                if loop_kind != Some(LoopKind::Rows) {
                    return Err(error("Iteration accounts are only valid inside forEach"));
                }
                self.batch_indices
                    .get(name.as_str())
                    .map(|&index| &self.row()[index].1)
                    .ok_or_else(|| error(format!("Unknown batch account: {name}")))
            }
        }
    }

    fn add_pubkey(&mut self, value: [u8; 32]) -> Result<u8, CompileError> {
        if let Some(index) = self.pubkeys.iter().position(|pubkey| *pubkey == value) {
            return Ok(index as u8);
        }
        let index = self.pubkeys.len();
        if index >= NO_INDEX as usize {
            return Err(error("Template contains too many pubkey constants"));
        }
        self.pubkeys.push(value);
        Ok(index as u8)
    }

    fn add_blob(&mut self, value: &[u8]) -> Result<usize, CompileError> {
        let offset = self.blob.len();
        self.blob.extend_from_slice(value);
        if self.blob.len() > 0xffff {
            return Err(error("Template blob exceeds 65535 bytes"));
        }
        Ok(offset)
    }
}

fn seed_segment(value: &Value) -> [u8; 8] {
    let kind = match value.ty {
        ValueType::Bool => DATA_REG_BOOL,
        ValueType::U64 => DATA_REG_U64,
        ValueType::I64 => DATA_REG_I64,
        ValueType::U128 => DATA_REG_U128,
        ValueType::Pubkey => DATA_REG_PUBKEY,
        ValueType::Bytes => DATA_REG_BYTES,
    };
    segment_record(kind, value.register as u8, 0, 0)
}

/// Programs that are invoked or derive PDAs must be pinned unless the author opts out.
fn require_pinned_program(
    reference: &AccountRef,
    constraint: &Account,
    role: &str,
) -> Result<(), CompileError> {
    if !constraint.executable {
        return Err(error(format!(
            "{role} account must require executable=true"
        )));
    }
    if constraint.address.is_none() && !constraint.unsafe_unpinned {
        return Err(error(format!(
            "{role} account {} must pin an address; call .unsafe_unpinned() on its declaration to accept any program",
            reference.name()
        )));
    }
    Ok(())
}

/// A registry entry's data is read only through its fields.
fn refuse_registry_data(reference: &AccountRef, constraint: &Account) -> Result<(), CompileError> {
    if constraint.registry.is_some() {
        let name = reference.name();
        return Err(error(format!(
            "{name} is a registry entry: read its fields with expr::registry(\"{name}\", field)"
        )));
    }
    Ok(())
}

/// Data reads only mean something when the account's layout is known, which needs a pin.
fn require_pinned_for_read(
    reference: &AccountRef,
    constraint: &Account,
) -> Result<(), CompileError> {
    if constraint.owner.is_none() && constraint.address.is_none() && !constraint.unsafe_unpinned {
        return Err(error(format!(
            "Account {} is read as data but pins neither owner nor address; call .unsafe_unpinned() on its declaration to read untrusted data",
            reference.name()
        )));
    }
    Ok(())
}
