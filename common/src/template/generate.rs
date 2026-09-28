//! Generates programs that verify by construction, for property tests that check the verifier and
//! the executor agree: anything the verifier accepts must execute without a structural error.
//!
//! Generation is driven by a stream of arbitrary `u32` choices so that proptest can shrink a
//! failing program by shrinking the choices. Every emitted instruction respects the register
//! typing rules, and every account read is a header field read, so the only failures a generated
//! program may produce at run time are value-dependent: overflow, division by zero, a failed
//! requirement, or a count loop's count above its maximum.

use proptest::prelude::*;

use super::*;

/// A generated program together with the run inputs that decode against its input table.
#[derive(Clone, Debug)]
pub struct GeneratedProgram {
    pub bytes: Vec<u8>,
    /// Encoded fixed input values.
    pub fixed_inputs: Vec<u8>,
    /// One encoded row of input values, repeated once per iteration in the run data.
    pub row_input_bytes: Vec<u8>,
    pub fixed_accounts: usize,
    pub row_accounts: usize,
    pub row_inputs: usize,
    pub account_groups: usize,
    pub max_iterations: usize,
    pub min_iterations: usize,
}

impl GeneratedProgram {
    /// Run data for `iterations` rows and the given group lengths: the group-length prefix, the
    /// fixed values, then one row of values per iteration.
    pub fn run_inputs(&self, iterations: usize, group_lengths: &[u8]) -> Vec<u8> {
        assert_eq!(group_lengths.len(), self.account_groups, "one length per declared group");
        let mut data = group_lengths.to_vec();
        data.extend_from_slice(&self.fixed_inputs);
        for _ in 0..iterations {
            data.extend_from_slice(&self.row_input_bytes);
        }
        data
    }
}

/// Value-dependent runtime error kinds a generated program is allowed to produce: overflow,
/// division by zero, a failed requirement, and `LoopCountExceeded`.
pub const ALLOWED_RUNTIME_ERRORS: [u32; 4] = [6013, 6014, 6015, 6022];

/// Runtime error kinds that indicate the verifier accepted something the executor rejects.
pub const STRUCTURAL_RUNTIME_ERRORS: [u32; 3] = [6002, 6011, 6012];

struct Choices<'a> {
    values: &'a [u32],
    cursor: usize,
}

impl Choices<'_> {
    fn next(&mut self) -> u32 {
        let value = self.values.get(self.cursor).copied().unwrap_or(0);
        self.cursor += 1;
        value
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            self.next() as usize % bound
        }
    }

    fn pick<T: Copy>(&mut self, options: &[T]) -> Option<T> {
        if options.is_empty() {
            None
        } else {
            Some(options[self.below(options.len())])
        }
    }
}

/// Registers known to hold a value in the current range, with their types. Register numbers come
/// from the builder, so a range that starts after a loop body does not reuse body registers.
#[derive(Clone)]
struct Registers {
    entries: Vec<(u8, u8)>,
}

impl Registers {
    fn of_type(&self, value_type: u8) -> Vec<u8> {
        self.entries
            .iter()
            .filter(|(_, kind)| *kind == value_type)
            .map(|(register, _)| *register)
            .collect()
    }

    fn numeric(&self) -> Vec<u8> {
        self.entries
            .iter()
            .filter(|(_, kind)| matches!(*kind, VALUE_U64 | VALUE_I64 | VALUE_U128))
            .map(|(register, _)| *register)
            .collect()
    }

    fn type_of(&self, register: u8) -> u8 {
        self.entries
            .iter()
            .find(|(candidate, _)| *candidate == register)
            .map(|(_, kind)| *kind)
            .expect("register is tracked")
    }

    fn push(&mut self, register: u8, value_type: u8) {
        self.entries.push((register, value_type));
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn last(&self) -> (u8, u8) {
        *self.entries.last().expect("at least the seed register")
    }
}

const SCALAR_TYPES: [u8; 5] = [VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY];

impl GeneratedProgram {
    /// Builds a program deterministically from a choice stream.
    pub fn from_choices(values: &[u32]) -> Self {
        let mut choices = Choices { values, cursor: 0 };
        let mut builder = ProgramBuilder::new();
        let mut registers = Registers { entries: Vec::new() };

        let fixed_accounts = choices.below(4);
        let accounts: Vec<u8> = (0..fixed_accounts)
            .map(|_| builder.account(0, None, None, 0))
            .collect();
        let batched = choices.below(2) == 1;
        let (row_accounts, max_iterations, min_iterations) = if batched {
            let max_iterations = 1 + choices.below(3);
            let min_iterations = choices.below(max_iterations + 1);
            builder.batch(max_iterations as u8, min_iterations as u8);
            (1usize, max_iterations, min_iterations)
        } else {
            (0, 0, 0)
        };

        let input_count = choices.below(4);
        let mut fixed_inputs = Vec::new();
        let mut inputs: Vec<(u8, u8)> = Vec::new();
        for _ in 0..input_count {
            let value_type = SCALAR_TYPES[choices.below(SCALAR_TYPES.len())];
            inputs.push((builder.input(value_type, 0), value_type));
            encode_input(&mut choices, value_type, &mut fixed_inputs);
        }
        // Row inputs exist only alongside a batch; their loads are valid inside the body.
        let row_input_count = if batched { choices.below(3) } else { 0 };
        let mut row_input_bytes = Vec::new();
        let mut body_inputs = inputs.clone();
        for _ in 0..row_input_count {
            let value_type = SCALAR_TYPES[choices.below(SCALAR_TYPES.len())];
            body_inputs.push((builder.row_input(value_type, 0), value_type));
            encode_input(&mut choices, value_type, &mut row_input_bytes);
        }
        // Groups have no schema, so the generator only declares them; the run supplies members.
        let account_groups = choices.below(3);
        builder.account_groups(account_groups as u8);

        // Every generated program starts with something in a register so later ops have operands.
        let seed = builder.const_u64(choices.next() as u64);
        registers.push(seed, VALUE_U64);

        let root_ops = 1 + choices.below(10);
        for _ in 0..root_ops {
            emit_operation(&mut choices, &mut builder, &mut registers, &accounts, &inputs, false);
        }

        // Loops run one after another and never nest. A batched program starts with a FOREACH,
        // and every later loop is a FOREACH or a REPEAT at random; an unbatched program has only
        // REPEATs, possibly none.
        let row = batched.then(|| builder.row_account(0, None, None, 0));
        let loops = if batched { 1 + choices.below(3) } else { choices.below(3) };
        for index in 0..loops {
            let foreach_row = row.filter(|_| index == 0 || choices.below(2) == 0);
            emit_loop(
                &mut choices,
                &mut builder,
                &mut registers,
                &accounts,
                &inputs,
                &body_inputs,
                foreach_row,
            );
            // Registers written inside a body are not visible afterwards; the type table for the
            // root stays as it was before the loop.
            let after_ops = choices.below(4);
            for _ in 0..after_ops {
                emit_operation(&mut choices, &mut builder, &mut registers, &accounts, &inputs, false);
            }
        }

        // A run may set its return data once, after its last invoke. Generated programs invoke
        // nothing, so the end of the root is always a legal place.
        if choices.below(2) == 1 {
            let part = output_part(&mut choices, &registers);
            builder.set_return_data(&[part]);
        }

        let bytes = builder.build().expect("generated programs stay within the payload limit");
        Self {
            bytes,
            fixed_inputs,
            row_input_bytes,
            fixed_accounts,
            row_accounts,
            row_inputs: row_input_count,
            account_groups,
            max_iterations,
            min_iterations,
        }
    }
}

/// Emits one loop at the root: a FOREACH over the batch rows when `row` names the row account,
/// otherwise a REPEAT. A FOREACH body may name the row account and load `foreach_inputs`, which
/// include the row inputs; a REPEAT body has fixed accounts and fixed inputs only. A REPEAT's count
/// is a fresh constant that can exceed its maximum, so some runs fail with `LoopCountExceeded`.
fn emit_loop(
    choices: &mut Choices<'_>,
    builder: &mut ProgramBuilder,
    registers: &mut Registers,
    accounts: &[u8],
    inputs: &[(u8, u8)],
    foreach_inputs: &[(u8, u8)],
    row: Option<u8>,
) {
    let count = if row.is_none() {
        let max = 1 + choices.below(3);
        let count = builder.const_u64(choices.below(max + 2) as u64);
        registers.push(count, VALUE_U64);
        Some((count, max as u8))
    } else {
        None
    };
    let carried = choices.pick(&registers.numeric());
    let carry_mask = carried.map_or(0, |register| 1u64 << register);
    let body_ops = 1 + choices.below(6);
    let mut body_accounts = accounts.to_vec();
    body_accounts.extend(row);
    let body_inputs = if row.is_some() { foreach_inputs } else { inputs };
    let root_registers = registers.clone();
    let body = |body: &mut ProgramBuilder| {
        let mut body_registers = root_registers.clone();
        for _ in 0..body_ops {
            emit_operation(choices, body, &mut body_registers, &body_accounts, body_inputs, true);
        }
        if let Some(register) = carried {
            // Accumulate into the carried register with a same-typed operand.
            let kind = body_registers.type_of(register);
            let operands = body_registers.of_type(kind);
            let other = operands[choices.below(operands.len())];
            let sum = body.binary(OP_ADD, register, other);
            body_registers.push(sum, kind);
            body.mov(register, sum);
        }
        // Operations may emit nothing when no operand of the right type exists, so end every
        // body with a requirement that always holds to keep it non-empty.
        let always = body.const_bool(true);
        body.require(always);
    };
    match count {
        Some((count, max)) => builder.repeat(count, max, carry_mask, body),
        None => builder.for_each(carry_mask, body),
    };
}

fn encode_input(choices: &mut Choices<'_>, value_type: u8, output: &mut Vec<u8>) {
    match value_type {
        VALUE_BOOL => output.push((choices.next() % 2) as u8),
        VALUE_U64 => output.extend_from_slice(&(choices.next() as u64).to_le_bytes()),
        VALUE_I64 => output.extend_from_slice(&(choices.next() as i32 as i64).to_le_bytes()),
        VALUE_U128 => output.extend_from_slice(&(choices.next() as u128).to_le_bytes()),
        VALUE_PUBKEY => output.extend_from_slice(&[(choices.next() % 256) as u8; 32]),
        _ => unreachable!("scalar input types only"),
    }
}

/// Emits one well-typed instruction chosen from the choice stream.
fn emit_operation(
    choices: &mut Choices<'_>,
    builder: &mut ProgramBuilder,
    registers: &mut Registers,
    accounts: &[u8],
    inputs: &[(u8, u8)],
    in_loop: bool,
) {
    // Loop bodies take registers the root never sees again, so the builder's own count is the
    // one that must stay under the 64-register limit, with room for each loop's fixed extras.
    if registers.len() >= 56 || builder.register_count() >= 48 {
        return;
    }
    match choices.below(17) {
        0 => {
            let value_type = SCALAR_TYPES[choices.below(SCALAR_TYPES.len())];
            let register = match value_type {
                VALUE_BOOL => builder.const_bool(choices.below(2) == 1),
                VALUE_U64 => builder.const_u64(choices.next() as u64),
                VALUE_I64 => builder.const_i64(choices.next() as i32 as i64),
                VALUE_U128 => builder.const_u128(choices.next() as u128),
                _ => builder.const_pubkey([(choices.next() % 256) as u8; 32]),
            };
            registers.push(register, value_type);
        }
        1 => {
            if let Some((input, value_type)) = choices.pick(inputs) {
                let register = builder.load_input(input);
                registers.push(register, value_type);
            }
        }
        2 => {
            if let Some(account) = choices.pick(accounts) {
                let (kind, opcode) = match choices.below(5) {
                    0 => (VALUE_PUBKEY, OP_ACCOUNT_KEY),
                    1 => (VALUE_PUBKEY, OP_ACCOUNT_OWNER),
                    2 => (VALUE_U64, OP_ACCOUNT_LAMPORTS),
                    3 => (VALUE_U64, OP_ACCOUNT_DATA_LEN),
                    _ => (VALUE_BOOL, OP_ACCOUNT_IS_EMPTY),
                };
                let register = builder.op(opcode, account, NO_INDEX, NO_INDEX, 0);
                registers.push(register, kind);
            }
        }
        3 | 4 => {
            let numeric = registers.numeric();
            if let Some(left) = choices.pick(&numeric) {
                let kind = registers.type_of(left);
                let same = registers.of_type(kind);
                let right = same[choices.below(same.len())];
                let opcode = [OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MIN, OP_MAX][choices.below(6)];
                let register = builder.binary(opcode, left, right);
                registers.push(register, kind);
            }
        }
        5 => {
            let numeric = registers.numeric();
            if let Some(left) = choices.pick(&numeric) {
                let kind = registers.type_of(left);
                let same = registers.of_type(kind);
                let right = same[choices.below(same.len())];
                let opcode = [OP_EQ, OP_NE, OP_LT, OP_LTE, OP_GT, OP_GTE][choices.below(6)];
                let register = builder.binary(opcode, left, right);
                registers.push(register, VALUE_BOOL);
            }
        }
        6 => {
            let value_type = SCALAR_TYPES[choices.below(SCALAR_TYPES.len())];
            let same = registers.of_type(value_type);
            if same.len() >= 2 {
                let left = same[choices.below(same.len())];
                let right = same[choices.below(same.len())];
                let opcode = if choices.below(2) == 0 { OP_EQ } else { OP_NE };
                let register = builder.binary(opcode, left, right);
                registers.push(register, VALUE_BOOL);
            }
        }
        7 => {
            let bools = registers.of_type(VALUE_BOOL);
            if let Some(left) = choices.pick(&bools) {
                let register = match choices.below(3) {
                    0 => builder.not(left),
                    1 => {
                        let right = bools[choices.below(bools.len())];
                        builder.binary(OP_AND, left, right)
                    }
                    _ => {
                        let right = bools[choices.below(bools.len())];
                        builder.binary(OP_OR, left, right)
                    }
                };
                registers.push(register, VALUE_BOOL);
            }
        }
        8 => {
            let bools = registers.of_type(VALUE_BOOL);
            let value_type = SCALAR_TYPES[choices.below(SCALAR_TYPES.len())];
            let same = registers.of_type(value_type);
            if let (Some(condition), false) = (choices.pick(&bools), same.is_empty()) {
                let if_true = same[choices.below(same.len())];
                let if_false = same[choices.below(same.len())];
                let register = builder.select(condition, if_true, if_false);
                registers.push(register, value_type);
            }
        }
        9 => {
            let numeric = registers.numeric();
            if let Some(value) = choices.pick(&numeric) {
                let (opcode, kind) = match choices.below(3) {
                    0 => (OP_CAST_U64, VALUE_U64),
                    1 => (OP_CAST_I64, VALUE_I64),
                    _ => (OP_CAST_U128, VALUE_U128),
                };
                let register = builder.cast(opcode, value);
                registers.push(register, kind);
            }
        }
        10 => {
            // Requirements are mostly satisfied so runs exercise later instructions, but some are
            // arbitrary booleans and may fail with RequirementFailed.
            if choices.below(4) == 0 {
                let bools = registers.of_type(VALUE_BOOL);
                if let Some(flag) = choices.pick(&bools) {
                    builder.require(flag);
                }
            } else {
                let flag = builder.const_bool(true);
                registers.push(flag, VALUE_BOOL);
                builder.require(flag);
            }
        }
        11 => {
            // Remainder on any numeric type; bitwise operations on unsigned ones.
            let numeric = registers.numeric();
            if let Some(left) = choices.pick(&numeric) {
                let kind = registers.type_of(left);
                let same = registers.of_type(kind);
                let right = same[choices.below(same.len())];
                let opcode = if kind == VALUE_I64 {
                    OP_REM
                } else {
                    [OP_REM, OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR][choices.below(4)]
                };
                let register = builder.binary(opcode, left, right);
                registers.push(register, kind);
            }
        }
        12 => {
            let unsigned: Vec<u8> = registers
                .numeric()
                .into_iter()
                .filter(|register| registers.type_of(*register) != VALUE_I64)
                .collect();
            if let Some(value) = choices.pick(&unsigned) {
                let kind = registers.type_of(value);
                // Amounts past the width exercise the zero and overflow rules too.
                let bits = builder.const_u64(choices.below(140) as u64);
                registers.push(bits, VALUE_U64);
                let opcode = if choices.below(2) == 0 { OP_SHL } else { OP_SHR };
                let register = builder.binary(opcode, value, bits);
                registers.push(register, kind);
            }
        }
        13 => {
            let unsigned: Vec<u8> = registers
                .numeric()
                .into_iter()
                .filter(|register| registers.type_of(*register) != VALUE_I64)
                .collect();
            if let Some(a) = choices.pick(&unsigned) {
                let kind = registers.type_of(a);
                let same = registers.of_type(kind);
                let b = same[choices.below(same.len())];
                let c = same[choices.below(same.len())];
                let register = if choices.below(2) == 0 {
                    builder.mul_div(a, b, c)
                } else {
                    builder.mul_div_ceil(a, b, c)
                };
                registers.push(register, kind);
            }
        }
        14 => {
            // Exponents past 38 overflow, which is an allowed value-dependent failure.
            let exponent = builder.const_u64(choices.below(45) as u64);
            registers.push(exponent, VALUE_U64);
            let register = builder.pow10(exponent);
            registers.push(register, VALUE_U128);
        }
        15 => {
            // A log line of one to three registers, sometimes after a literal byte. Outputs may
            // appear anywhere, loop bodies included, and write no register.
            let mut parts = Vec::new();
            if choices.below(4) == 0 {
                parts.push(Segment::Literal(builder.blob(&[choices.next() as u8])));
            }
            for _ in 0..1 + choices.below(3) {
                parts.push(output_part(choices, registers));
            }
            builder.emit_data(&parts);
        }
        _ => {
            if in_loop {
                let register = builder.loop_index();
                registers.push(register, VALUE_U64);
            } else {
                let (source, kind) = registers.last();
                let dst = builder.register();
                builder.mov(dst, source);
                registers.push(dst, kind);
            }
        }
    }
}

/// One output part: a register that holds a value, with an encoding its type accepts. Unsigned
/// values may be narrowed, and a value too wide for its encoding fails the run with
/// `ArithmeticOverflow`, an allowed value-dependent failure.
fn output_part(choices: &mut Choices<'_>, registers: &Registers) -> Segment {
    let (register, kind) = registers.entries[choices.below(registers.len())];
    let encoding = match kind {
        VALUE_BOOL => DATA_REG_BOOL,
        VALUE_I64 => DATA_REG_I64,
        VALUE_PUBKEY => DATA_REG_PUBKEY,
        VALUE_U128 if choices.below(2) == 0 => DATA_REG_U128,
        _ => [DATA_REG_U8, DATA_REG_U16, DATA_REG_U32, DATA_REG_U64][choices.below(4)],
    };
    Segment::Register(encoding, register)
}

/// A strategy producing programs that verify by construction.
pub fn any_program() -> impl Strategy<Value = GeneratedProgram> {
    prop::collection::vec(any::<u32>(), 8..96).prop_map(|choices| GeneratedProgram::from_choices(&choices))
}
