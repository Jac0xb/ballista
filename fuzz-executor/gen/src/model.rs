//! An independent reference interpreter of the Ballista wire format, written from the language and
//! wire-format docs rather than from `programs/ballista`. It predicts the cross-program
//! invocations a successful run should emit and the return data it should set, so a run that
//! succeeds can be checked for doing the right thing, not merely for not aborting.
//!
//! Values the model cannot know without running the chain are tracked as [`Val::Opaque`]:
//! - a CPI's return data,
//! - any account field or data read after the first CPI, since a CPI may have changed it,
//! - an account-group filter over a member a call was passed, or whose owner or data the scenario
//!   does not mirror,
//! - introspection reads.
//!
//! Opaque data taints the CPI data built from it, which the model marks [`CpiData::Opaque`] and the
//! harness then skips comparing, while still checking the call's program, accounts and the flags
//! each account is passed with. A control-flow decision (a guard, a `repeat` count, a `require`, a
//! `select`) that depends on an opaque value makes the whole prediction
//! [`Prediction::Indeterminate`], and the harness falls back to its model-free checks for that run.
//! Concrete control flow, which is the common case, yields a full [`Prediction::Calls`] the harness
//! matches against the captured inner instructions, or a [`Prediction::Fails`] the run must agree
//! with. Every disagreement with a concrete prediction fails the fuzz test.

use ballista_common::template::*;

/// A program the model resolves a CPI's program account to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedCpi {
    pub program: [u8; 32],
    /// One meta per listed account, then the forwarded group members.
    pub accounts: Vec<ExpectedMeta>,
    pub data: CpiData,
    /// Whether the descriptor named a row account as the program, so the program may differ by
    /// batch row.
    pub row_program: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedMeta {
    pub address: [u8; 32],
    pub signer: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CpiData {
    Concrete(Vec<u8>),
    /// Built from a value the model could not predict; compare the program and accounts only.
    Opaque,
}

#[derive(Clone, Debug)]
pub enum Prediction {
    /// The model ran the whole template with concrete control flow: its CPIs in order, the bytes
    /// of every `EMIT` line in order, and the return data. `loop_passes` counts the loop passes it
    /// made, so the harness knows how often a loop's register restore was compared. `exact` says
    /// the model knew every value and made no call, so no instruction can have failed: a run that
    /// gets past its account checks must not fail with a Ballista code while it executes.
    /// `group_values` counts the group opcodes it evaluated to a concrete value.
    Calls {
        cpis: Vec<ExpectedCpi>,
        emits: Vec<CpiData>,
        return_data: CpiData,
        sets_return_data: bool,
        loop_passes: usize,
        exact: bool,
        group_values: usize,
    },
    /// A control-flow decision depended on an opaque value; only the model-free checks apply.
    Indeterminate(&'static str),
    /// The model predicts the run fails, for a reason it computed from concrete values. A run that
    /// succeeds anyway disagrees with the model, and the harness fails the test.
    Fails(Failure),
}

/// A failure the model predicts: the reason, the Ballista error code the executor raises for it,
/// and the instruction that raises it.
#[derive(Clone, Debug)]
pub struct Failure {
    pub reason: &'static str,
    /// The error kind, 6000 to 6026 (`BallistaError`), without the pc the run encodes above it.
    pub kind: u32,
    /// The failing instruction's index.
    pub pc: usize,
    /// Whether the model knew every value up to the failure and no call or registry access came
    /// before it. Then a run that gets past its account checks and fails while it executes must
    /// fail exactly here, with exactly this code.
    pub exact: bool,
    /// Group opcodes the model evaluated to a concrete value on the way to the failure.
    pub group_values: usize,
}

const ARITHMETIC_OVERFLOW: u32 = 6013;
const DIVISION_BY_ZERO: u32 = 6014;
const REQUIREMENT_FAILED: u32 = 6015;
const INVALID_RUNTIME_ACCOUNT: u32 = 6009;
const TYPE_MISMATCH: u32 = 6012;
const LOOP_COUNT_EXCEEDED: u32 = 6022;
const INSTRUCTION_OUT_OF_RANGE: u32 = 6023;
const WRITABLE_ACCOUNT_BYTES_READ: u32 = 6024;
/// Not a Ballista code: a group filter that reaches a member an open holds borrowed fails with the
/// runtime's `AccountBorrowFailed`, which carries no pc. Only a run that opened an entry meets it,
/// so its prediction is never exact and the harness checks only that the run failed.
const ACCOUNT_BORROW_FAILED: u32 = 0;

/// A failure with the code the executor raises; `Model::step` fills in where.
fn fails(reason: &'static str, kind: u32) -> Prediction {
    Prediction::Fails(Failure { reason, kind, pc: usize::MAX, exact: false, group_values: 0 })
}

/// What the model needs to know about one runtime account, by its index among the run's accounts
/// (the template account is not included; index 0 is the first runtime account).
pub trait Accounts {
    fn key(&self, index: usize) -> Option<[u8; 32]>;
    fn owner(&self, index: usize) -> Option<[u8; 32]>;
    fn lamports(&self, index: usize) -> Option<u64>;
    fn data(&self, index: usize) -> Option<&[u8]>;
    fn is_writable(&self, index: usize) -> Option<bool>;
    /// Whether the account is a program. Its data and lamports are set by the loader, which the
    /// model cannot predict, so reads of an executable account are opaque.
    fn executable(&self, index: usize) -> Option<bool>;
    fn count(&self) -> usize;
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Unset,
    Opaque,
    Bool(bool),
    U64(u64),
    I64(i64),
    U128(u128),
    Pubkey([u8; 32]),
    Bytes(Vec<u8>),
}

/// Decoded run input values, fixed first then one row per iteration, exactly as the executor
/// decodes them.
#[derive(Clone, Debug)]
pub enum InputVal {
    Bool(bool),
    U64(u64),
    I64(i64),
    U128(u128),
    Pubkey([u8; 32]),
    Bytes(Vec<u8>),
}

impl InputVal {
    fn to_val(&self) -> Val {
        match self {
            InputVal::Bool(value) => Val::Bool(*value),
            InputVal::U64(value) => Val::U64(*value),
            InputVal::I64(value) => Val::I64(*value),
            InputVal::U128(value) => Val::U128(*value),
            InputVal::Pubkey(value) => Val::Pubkey(*value),
            InputVal::Bytes(value) => Val::Bytes(value.clone()),
        }
    }
}

/// What a run meets besides its accounts: the run data's values and layout, the clock, the rent
/// schedule and Ballista's own address.
pub struct RunContext<'a> {
    /// Decoded run input values, fixed first then one row per iteration.
    pub inputs: &'a [InputVal],
    /// Batch rows the run passes.
    pub iterations: usize,
    /// Each account group's length, from the run data's prefix.
    pub group_lengths: &'a [u8],
    pub clock_slot: u64,
    pub clock_timestamp: i64,
    /// The rent-exempt minimum balance for a data length, which an entry the run creates gets.
    pub rent: &'a dyn Fn(usize) -> u64,
    /// The program that owns the entries a run creates.
    pub ballista: [u8; 32],
}

pub struct Model<'a, A: Accounts> {
    program: &'a ProgramView<'a>,
    accounts: &'a A,
    inputs: &'a [InputVal],
    iterations: usize,
    group_lengths: &'a [u8],
    clock_slot: u64,
    clock_timestamp: i64,
    rent: &'a dyn Fn(usize) -> u64,
    ballista: [u8; 32],
    /// Once any CPI has run, account state may have changed, so reads become opaque.
    mutated: bool,
    /// Every account a CPI so far was passed, the template's or an entry creation's: only those
    /// can have changed, since a program changes only accounts it is passed.
    touched: Vec<[u8; 32]>,
    /// Each entry an open has marked, with its data length: Ballista's, and borrowed for the rest
    /// of the run.
    opened: Vec<([u8; 32], usize)>,
    /// Group opcodes evaluated to a concrete value.
    group_values: usize,
    /// Set once an instruction produced a value the model does not know, or the run made a call or
    /// touched a registry: from then on the run can fail where the model cannot tell.
    uncertain: bool,
    cpis: Vec<ExpectedCpi>,
    emits: Vec<CpiData>,
    return_data: CpiData,
    sets_return_data: bool,
    loop_passes: usize,
}

/// Runs the model: what a run of `program` over `accounts` in `context` should do.
pub fn predict<A: Accounts>(program: &ProgramView, accounts: &A, context: &RunContext) -> Prediction {
    let mut model = Model {
        program,
        accounts,
        inputs: context.inputs,
        iterations: context.iterations,
        group_lengths: context.group_lengths,
        clock_slot: context.clock_slot,
        clock_timestamp: context.clock_timestamp,
        rent: context.rent,
        ballista: context.ballista,
        mutated: false,
        touched: Vec::new(),
        opened: Vec::new(),
        group_values: 0,
        uncertain: false,
        cpis: Vec::new(),
        emits: Vec::new(),
        return_data: CpiData::Concrete(Vec::new()),
        sets_return_data: false,
        loop_passes: 0,
    };
    let mut registers = vec![Val::Unset; program.header.register_count()];
    match model.run(&mut registers) {
        Ok(()) => Prediction::Calls {
            cpis: model.cpis,
            emits: model.emits,
            return_data: model.return_data,
            sets_return_data: model.sets_return_data,
            loop_passes: model.loop_passes,
            exact: !model.uncertain,
            group_values: model.group_values,
        },
        Err(stop) => stop,
    }
}

type Flow = Result<(), Prediction>;

impl<A: Accounts> Model<'_, A> {
    fn run(&mut self, registers: &mut [Val]) -> Flow {
        let instructions = self.program.instructions;
        let mut pc = 0;
        while pc < instructions.len() {
            let instruction = &instructions[pc];
            match instruction.opcode {
                OP_FOREACH => {
                    let body = pc + 1..pc + 1 + instruction.a as usize;
                    self.run_loop(registers, body.clone(), self.iterations, true, instruction.immediate())?;
                    // `body.end` is the next instruction after the body; continue from there
                    // without the `pc += 1` below, which would skip it.
                    pc = body.end;
                    continue;
                }
                OP_REPEAT => {
                    let body = pc + 1..pc + 1 + instruction.a as usize;
                    let count = match &registers[instruction.b as usize] {
                        Val::U64(count) => *count as usize,
                        Val::Opaque => return Err(Prediction::Indeterminate("repeat count opaque")),
                        _ => return Err(Prediction::Indeterminate("repeat count not u64")),
                    };
                    if count > instruction.c as usize {
                        return Err(self.locate(fails("repeat count exceeds max", LOOP_COUNT_EXCEEDED), pc));
                    }
                    self.run_loop(registers, body.clone(), count, false, instruction.immediate())?;
                    pc = body.end;
                    continue;
                }
                _ => self.step(registers, pc, None)?,
            }
            pc += 1;
        }
        Ok(())
    }

    fn run_loop(&mut self, registers: &mut [Val], body: std::ops::Range<usize>, passes: usize, foreach: bool, carry: u64) -> Flow {
        // Mirrors `next_pass`: after each pass the carried registers flow into the snapshot, then
        // every register is restored from it, so non-carried registers reset and carried ones
        // carry forward. The final restore leaves carried registers at their last value and every
        // other register as it was before the loop.
        let carried = |register: usize| register < MAX_REGISTERS && carry & (1u64 << register) != 0;
        let mut snapshot: Vec<Val> = registers.to_vec();
        for pass in 0..passes {
            let row_base = if foreach {
                self.program.header.fixed_account_count() + pass * self.program.header.batch_stride()
            } else {
                usize::MAX / 2
            };
            for pc in body.clone() {
                self.step(registers, pc, Some((pass, row_base)))?;
            }
            self.loop_passes += 1;
            for register in 0..registers.len() {
                if carried(register) {
                    snapshot[register] = registers[register].clone();
                }
            }
            registers.clone_from_slice(&snapshot);
        }
        Ok(())
    }

    /// Runs one instruction, places a failure it predicts, and notes whether the model still knows
    /// everything: an opaque result, a call or a registry access ends that.
    fn step(&mut self, registers: &mut [Val], pc: usize, loop_context: Option<(usize, usize)>) -> Flow {
        let instruction = &self.program.instructions[pc];
        if matches!(instruction.opcode, OP_INVOKE | OP_OPEN_REGISTRY | OP_READ_REGISTRY | OP_WRITE_REGISTRY) {
            self.uncertain = true;
        }
        let result = self.execute(registers, pc, loop_context).map_err(|stop| self.locate(stop, pc));
        let dst = instruction.dst as usize;
        if dst < registers.len() && matches!(registers[dst], Val::Opaque) {
            self.uncertain = true;
        }
        result
    }

    fn locate(&self, stop: Prediction, pc: usize) -> Prediction {
        match stop {
            Prediction::Fails(mut failure) if failure.pc == usize::MAX => {
                failure.pc = pc;
                failure.exact = !self.uncertain;
                failure.group_values = self.group_values;
                Prediction::Fails(failure)
            }
            other => other,
        }
    }

    fn execute(&mut self, registers: &mut [Val], pc: usize, loop_context: Option<(usize, usize)>) -> Flow {
        let instruction = &self.program.instructions[pc];
        let dst = instruction.dst as usize;
        macro_rules! set {
            ($value:expr) => {{
                let value = $value;
                registers[dst] = value;
            }};
        }
        match instruction.opcode {
            OP_LOAD_INPUT => set!(self.load_input(instruction, loop_context)?),
            OP_CONST_BOOL => set!(Val::Bool(instruction.a != 0)),
            OP_CONST_U64 => set!(Val::U64(instruction.immediate())),
            OP_CONST_I64 => set!(Val::I64(i64::from_le_bytes(instruction.immediate_le))),
            OP_CONST_U128 => {
                let bytes = self.blob(instruction)?;
                let array: [u8; 16] = bytes.try_into().map_err(|_| Prediction::Indeterminate("u128 blob"))?;
                set!(Val::U128(u128::from_le_bytes(array)));
            }
            OP_CONST_PUBKEY => {
                let key = self.program.pubkeys[instruction.a as usize].bytes;
                set!(Val::Pubkey(key));
            }
            OP_CONST_BYTES => set!(Val::Bytes(self.blob(instruction)?.to_vec())),
            OP_ACCOUNT_KEY => set!(self.account_key(instruction.a, loop_context)?),
            OP_ACCOUNT_OWNER | OP_ACCOUNT_LAMPORTS | OP_ACCOUNT_DATA_LEN | OP_ACCOUNT_IS_EMPTY => {
                set!(self.account_field(instruction, loop_context)?)
            }
            OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_U128
            | OP_READ_PUBKEY | OP_READ_BOOL | OP_READ_I32 => set!(self.read_account(registers, instruction, loop_context)?),
            OP_CLOCK_SLOT => set!(Val::U64(self.clock_slot)),
            OP_CLOCK_TIMESTAMP => set!(Val::I64(self.clock_timestamp)),
            OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX => set!(self.arithmetic(registers, instruction)?),
            OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR => set!(self.integer(registers, instruction)?),
            OP_MUL_DIV | OP_MUL_DIV_CEIL => set!(self.mul_div(registers, instruction)?),
            OP_POW10 => set!(self.pow10(registers, instruction)?),
            OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => set!(self.compare(registers, instruction)?),
            OP_AND | OP_OR | OP_NOT => set!(self.logic(registers, instruction)?),
            OP_SELECT => set!(self.select(registers, instruction)?),
            OP_CAST_U64 | OP_CAST_I64 | OP_CAST_U128 => set!(self.cast(registers, instruction)?),
            OP_LOOP_INDEX => {
                let (pass, _) = loop_context.ok_or(Prediction::Indeterminate("loop index outside loop"))?;
                set!(Val::U64(pass as u64));
            }
            OP_MOVE => set!(registers[instruction.a as usize].clone()),
            OP_BYTES_LEN => {
                let value = registers[instruction.a as usize].clone();
                set!(match value {
                    Val::Bytes(bytes) => Val::U64(bytes.len() as u64),
                    Val::Opaque => Val::Opaque,
                    _ => return Err(Prediction::Indeterminate("bytes_len on non-bytes")),
                });
            }
            OP_DERIVE_PDA | OP_CREATE_PDA => {
                // The address depends on the real PDA derivation; treat it as opaque for data and
                // never as control flow. A mismatch in a PDA check surfaces as a run failure, not
                // here.
                set!(Val::Opaque);
            }
            OP_RETURN_DATA => set!(Val::Opaque),
            OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX | OP_INSTRUCTION_PROGRAM
            | OP_INSTRUCTION_ACCOUNT_COUNT | OP_INSTRUCTION_ACCOUNT | OP_INSTRUCTION_ACCOUNT_FLAGS
            | OP_INSTRUCTION_DATA_LEN | OP_READ_INSTRUCTION_DATA | OP_READ_INSTRUCTION_BYTES => {
                set!(Val::Opaque)
            }
            OP_READ_ACCOUNT_BYTES => set!(self.read_account_bytes(registers, instruction, loop_context)?),
            OP_REQUIRE => {
                match &registers[instruction.a as usize] {
                    Val::Bool(true) => {}
                    Val::Bool(false) => return Err(fails("require false", REQUIREMENT_FAILED)),
                    Val::Opaque => return Err(Prediction::Indeterminate("require opaque")),
                    _ => return Err(Prediction::Indeterminate("require non-bool")),
                }
            }
            OP_INVOKE => self.invoke(registers, instruction, loop_context)?,
            // A log line changes no account, so reads stay predictable after it.
            OP_EMIT => {
                let line = self.build_output(registers, instruction)?;
                self.emits.push(line);
            }
            OP_SET_RETURN_DATA => {
                self.return_data = self.build_output(registers, instruction)?;
                self.sets_return_data = true;
            }
            OP_OPEN_REGISTRY => self.open_registry(instruction)?,
            OP_READ_REGISTRY => set!(Val::Opaque),
            OP_WRITE_REGISTRY => { self.mutated = true; }
            OP_GROUP_LENGTH => set!(self.group_length(instruction)?),
            OP_GROUP_ANY | OP_GROUP_COUNT => set!(self.group_filter(registers, instruction)?),
            _ => return Err(Prediction::Indeterminate("unknown opcode")),
        }
        Ok(())
    }

    fn blob(&self, instruction: &InstructionRecord) -> Result<&[u8], Prediction> {
        let (offset, len) = instruction.blob_range();
        offset.checked_add(len).and_then(|end| self.program.blob.get(offset..end)).ok_or(Prediction::Indeterminate("blob range"))
    }

    fn load_input(&self, instruction: &InstructionRecord, loop_context: Option<(usize, usize)>) -> Result<Val, Prediction> {
        let index = if instruction.a & ITERATION_INPUT_BIT == 0 {
            instruction.a as usize
        } else {
            let (pass, _) = loop_context.ok_or(Prediction::Indeterminate("row input outside loop"))?;
            let offset = (instruction.a & !ITERATION_INPUT_BIT) as usize;
            self.program.header.input_count() + pass * self.program.header.row_input_count() + offset
        };
        Ok(self.inputs.get(index).map(InputVal::to_val).unwrap_or(Val::Unset))
    }

    fn resolve(&self, reference: u8, loop_context: Option<(usize, usize)>) -> Result<usize, Prediction> {
        if reference & ITERATION_ACCOUNT_BIT == 0 {
            Ok(reference as usize)
        } else {
            let (_, row_base) = loop_context.ok_or(Prediction::Indeterminate("row account outside loop"))?;
            Ok(row_base + (reference & !ITERATION_ACCOUNT_BIT) as usize)
        }
    }

    fn account_key(&self, reference: u8, loop_context: Option<(usize, usize)>) -> Result<Val, Prediction> {
        let index = self.resolve(reference, loop_context)?;
        Ok(self.accounts.key(index).map(Val::Pubkey).unwrap_or(Val::Opaque))
    }

    fn account_field(&self, instruction: &InstructionRecord, loop_context: Option<(usize, usize)>) -> Result<Val, Prediction> {
        if self.mutated {
            return Ok(Val::Opaque);
        }
        let index = self.resolve(instruction.a, loop_context)?;
        if self.accounts.executable(index) != Some(false) {
            // A program account's data length and lamports are loader-defined; its key stays
            // readable below, but lamports/data_len/is_empty are opaque. Owner too for a program.
            if instruction.opcode != OP_ACCOUNT_KEY {
                return Ok(Val::Opaque);
            }
        }
        Ok(match instruction.opcode {
            OP_ACCOUNT_OWNER => self.accounts.owner(index).map(Val::Pubkey).unwrap_or(Val::Opaque),
            OP_ACCOUNT_LAMPORTS => self.accounts.lamports(index).map(Val::U64).unwrap_or(Val::Opaque),
            OP_ACCOUNT_DATA_LEN => self.accounts.data(index).map(|data| Val::U64(data.len() as u64)).unwrap_or(Val::Opaque),
            _ => self.accounts.data(index).map(|data| Val::Bool(data.is_empty())).unwrap_or(Val::Opaque),
        })
    }

    fn read_account(&self, registers: &[Val], instruction: &InstructionRecord, loop_context: Option<(usize, usize)>) -> Result<Val, Prediction> {
        if self.mutated {
            return Ok(Val::Opaque);
        }
        let index = self.resolve(instruction.a, loop_context)?;
        if self.accounts.executable(index) != Some(false) {
            return Ok(Val::Opaque);
        }
        let Some(data) = self.accounts.data(index) else { return Ok(Val::Opaque) };
        let offset = if instruction.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0 {
            match &registers[instruction.b as usize] {
                Val::U64(value) => *value as usize,
                Val::Opaque => return Ok(Val::Opaque),
                _ => return Err(Prediction::Indeterminate("dynamic offset not u64")),
            }
        } else {
            instruction.immediate() as usize
        };
        decode_read(instruction.opcode, data, offset)
    }

    fn read_account_bytes(&self, registers: &[Val], instruction: &InstructionRecord, loop_context: Option<(usize, usize)>) -> Result<Val, Prediction> {
        if self.mutated {
            return Ok(Val::Opaque);
        }
        let index = self.resolve(instruction.a, loop_context)?;
        // The executor refuses an account writable in this instruction before it looks at the
        // range: its bytes could change under a CPI.
        match self.accounts.is_writable(index) {
            Some(true) => return Err(fails("account bytes of a writable account", WRITABLE_ACCOUNT_BYTES_READ)),
            Some(false) => {}
            None => return Ok(Val::Opaque),
        }
        if self.mutated || self.accounts.executable(index) != Some(false) {
            return Ok(Val::Opaque);
        }
        let Some(data) = self.accounts.data(index) else { return Ok(Val::Opaque) };
        let offset = match &registers[instruction.b as usize] {
            Val::U64(value) => *value as usize,
            Val::Opaque => return Ok(Val::Opaque),
            _ => return Err(Prediction::Indeterminate("byte offset not u64")),
        };
        let len = instruction.immediate() as usize;
        match offset.checked_add(len).and_then(|end| data.get(offset..end)) {
            Some(bytes) => Ok(Val::Bytes(bytes.to_vec())),
            None => Err(fails("account bytes out of range", INSTRUCTION_OUT_OF_RANGE)),
        }
    }

    fn arithmetic(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let left = &registers[instruction.a as usize];
        let right = &registers[instruction.b as usize];
        if matches!(left, Val::Opaque) || matches!(right, Val::Opaque) {
            return Ok(Val::Opaque);
        }
        macro_rules! op {
            ($l:expr, $r:expr, $wrap:path) => {{
                let value = match instruction.opcode {
                    OP_ADD => $l.checked_add($r),
                    OP_SUB => $l.checked_sub($r),
                    OP_MUL => $l.checked_mul($r),
                    OP_DIV => $l.checked_div($r),
                    OP_MIN => Some($l.min($r)),
                    OP_MAX => Some($l.max($r)),
                    _ => None,
                };
                match value {
                    Some(value) => $wrap(value),
                    None if instruction.opcode == OP_DIV && $r == 0 => return Err(fails("division by zero", DIVISION_BY_ZERO)),
                    None => return Err(fails("arithmetic overflow", ARITHMETIC_OVERFLOW)),
                }
            }};
        }
        Ok(match (left, right) {
            (Val::U64(l), Val::U64(r)) => op!(*l, *r, Val::U64),
            (Val::I64(l), Val::I64(r)) => op!(*l, *r, Val::I64),
            (Val::U128(l), Val::U128(r)) => op!(*l, *r, Val::U128),
            _ => return Err(Prediction::Indeterminate("arithmetic type mismatch")),
        })
    }

    fn integer(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let left = registers[instruction.a as usize].clone();
        let right = registers[instruction.b as usize].clone();
        if matches!(left, Val::Opaque) || matches!(right, Val::Opaque) {
            return Ok(Val::Opaque);
        }
        let opcode = instruction.opcode;
        if opcode == OP_SHL || opcode == OP_SHR {
            let shift = match right {
                Val::U64(value) => value,
                _ => return Err(Prediction::Indeterminate("shift amount not u64")),
            };
            return match left {
                Val::U64(value) => shift_u128(value as u128, shift, 64, opcode).map(|v| Val::U64(v as u64)),
                Val::U128(value) => shift_u128(value, shift, 128, opcode).map(Val::U128),
                _ => Err(Prediction::Indeterminate("shift on non-unsigned")),
            };
        }
        macro_rules! bit {
            ($l:expr, $r:expr, $wrap:path) => {{
                let value = match opcode {
                    OP_REM => $l.checked_rem($r),
                    OP_BIT_AND => Some($l & $r),
                    OP_BIT_OR => Some($l | $r),
                    OP_BIT_XOR => Some($l ^ $r),
                    _ => None,
                };
                match value {
                    Some(value) => $wrap(value),
                    None => return Err(fails("rem by zero", DIVISION_BY_ZERO)),
                }
            }};
        }
        Ok(match (left, right) {
            (Val::U64(l), Val::U64(r)) => bit!(l, r, Val::U64),
            (Val::U128(l), Val::U128(r)) => bit!(l, r, Val::U128),
            (Val::I64(l), Val::I64(r)) if opcode == OP_REM => match l.checked_rem(r) {
                Some(value) => Val::I64(value),
                None if r == 0 => return Err(fails("rem by zero", DIVISION_BY_ZERO)),
                None => return Err(fails("i64 rem overflow", ARITHMETIC_OVERFLOW)),
            },
            _ => return Err(Prediction::Indeterminate("integer op type")),
        })
    }

    fn mul_div(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let a = registers[instruction.a as usize].clone();
        let b = registers[instruction.b as usize].clone();
        let c = registers[instruction.c as usize].clone();
        if [&a, &b, &c].iter().any(|value| matches!(value, Val::Opaque)) {
            return Ok(Val::Opaque);
        }
        let ceil = instruction.opcode == OP_MUL_DIV_CEIL;
        match (a, b, c) {
            (Val::U64(_), Val::U64(_), Val::U64(0)) => Err(fails("mul_div by zero", DIVISION_BY_ZERO)),
            (Val::U64(a), Val::U64(b), Val::U64(c)) => mul_div(a as u128, b as u128, c as u128, ceil)
                .and_then(|value| u64::try_from(value).ok())
                .map(Val::U64)
                .ok_or(fails("mul_div u64 overflow", ARITHMETIC_OVERFLOW)),
            (Val::U128(a), Val::U128(b), Val::U128(c)) => mul_div_u128(a, b, c, ceil),
            _ => Err(Prediction::Indeterminate("mul_div type")),
        }
    }

    fn pow10(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        match registers[instruction.a as usize].clone() {
            Val::Opaque => Ok(Val::Opaque),
            Val::U64(exp) => {
                if exp > 38 {
                    Err(fails("pow10 overflow", ARITHMETIC_OVERFLOW))
                } else {
                    Ok(Val::U128(10u128.pow(exp as u32)))
                }
            }
            _ => Err(Prediction::Indeterminate("pow10 type")),
        }
    }

    fn compare(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let left = registers[instruction.a as usize].clone();
        let right = registers[instruction.b as usize].clone();
        if matches!(left, Val::Opaque) || matches!(right, Val::Opaque) {
            return Ok(Val::Opaque);
        }
        let opcode = instruction.opcode;
        macro_rules! ord {
            ($l:expr, $r:expr) => {
                match opcode {
                    OP_EQ => $l == $r,
                    OP_NE => $l != $r,
                    OP_LT => $l < $r,
                    OP_LTE => $l <= $r,
                    OP_GT => $l > $r,
                    OP_GTE => $l >= $r,
                    _ => unreachable!(),
                }
            };
        }
        let result = match (&left, &right) {
            (Val::U64(l), Val::U64(r)) => ord!(l, r),
            (Val::I64(l), Val::I64(r)) => ord!(l, r),
            (Val::U128(l), Val::U128(r)) => ord!(l, r),
            (Val::Bool(l), Val::Bool(r)) if matches!(opcode, OP_EQ | OP_NE) => ord!(l, r),
            (Val::Pubkey(l), Val::Pubkey(r)) if matches!(opcode, OP_EQ | OP_NE) => ord!(l, r),
            (Val::Bytes(l), Val::Bytes(r)) if matches!(opcode, OP_EQ | OP_NE) => ord!(l, r),
            _ => return Err(Prediction::Indeterminate("compare type")),
        };
        Ok(Val::Bool(result))
    }

    fn logic(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let left = registers[instruction.a as usize].clone();
        if instruction.opcode == OP_NOT {
            return match left {
                Val::Bool(value) => Ok(Val::Bool(!value)),
                Val::Opaque => Ok(Val::Opaque),
                _ => Err(Prediction::Indeterminate("not non-bool")),
            };
        }
        let right = registers[instruction.b as usize].clone();
        if matches!(left, Val::Opaque) || matches!(right, Val::Opaque) {
            return Ok(Val::Opaque);
        }
        match (left, right) {
            (Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(if instruction.opcode == OP_AND { l && r } else { l || r })),
            _ => Err(Prediction::Indeterminate("logic non-bool")),
        }
    }

    fn select(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        match registers[instruction.a as usize].clone() {
            Val::Bool(true) => Ok(registers[instruction.b as usize].clone()),
            Val::Bool(false) => Ok(registers[instruction.c as usize].clone()),
            Val::Opaque => Ok(Val::Opaque),
            _ => Err(Prediction::Indeterminate("select non-bool condition")),
        }
    }

    fn cast(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let value = registers[instruction.a as usize].clone();
        if matches!(value, Val::Opaque) {
            return Ok(Val::Opaque);
        }
        let fail = |_| fails("cast out of range", ARITHMETIC_OVERFLOW);
        Ok(match instruction.opcode {
            OP_CAST_U64 => Val::U64(match value {
                Val::U64(v) => v,
                Val::I64(v) => u64::try_from(v).map_err(fail)?,
                Val::U128(v) => u64::try_from(v).map_err(fail)?,
                _ => return Err(Prediction::Indeterminate("cast type")),
            }),
            OP_CAST_I64 => Val::I64(match value {
                Val::U64(v) => i64::try_from(v).map_err(fail)?,
                Val::I64(v) => v,
                Val::U128(v) => i64::try_from(v).map_err(fail)?,
                _ => return Err(Prediction::Indeterminate("cast type")),
            }),
            _ => Val::U128(match value {
                Val::U64(v) => v as u128,
                Val::I64(v) => u128::try_from(v).map_err(fail)?,
                Val::U128(v) => v,
                _ => return Err(Prediction::Indeterminate("cast type")),
            }),
        })
    }

    fn invoke(&mut self, registers: &[Val], instruction: &InstructionRecord, loop_context: Option<(usize, usize)>) -> Flow {
        let guard = instruction.b;
        if guard != NO_INDEX {
            match &registers[guard as usize] {
                Val::Bool(true) => {}
                Val::Bool(false) => return Ok(()),
                Val::Opaque => return Err(Prediction::Indeterminate("guard opaque")),
                _ => return Err(Prediction::Indeterminate("guard non-bool")),
            }
        }
        let descriptor = &self.program.cpis[instruction.a as usize];
        let program = {
            let index = self.resolve(descriptor.program_account, loop_context)?;
            self.accounts.key(index).ok_or(Prediction::Indeterminate("program account out of range"))?
        };
        let mut accounts = Vec::new();
        let start = descriptor.account_start();
        for record in &self.program.cpi_accounts[start..start + descriptor.account_len as usize] {
            let index = self.resolve(record.account, loop_context)?;
            let address = self.accounts.key(index).ok_or(Prediction::Indeterminate("cpi account out of range"))?;
            accounts.push(ExpectedMeta {
                address,
                signer: record.flags & ACCOUNT_SIGNER != 0,
                writable: record.flags & ACCOUNT_WRITABLE != 0,
            });
        }
        if let Some(group) = descriptor.account_group() {
            // Group members are forwarded writable with the transaction's own flag, never as
            // signers. The model needs their run-time writable flags, which the harness supplies
            // through the account view.
            let groups = self.group_ranges();
            if let Some((group_start, len)) = groups.get(group).copied() {
                for offset in 0..len {
                    let index = group_start + offset;
                    let address = self.accounts.key(index).ok_or(Prediction::Indeterminate("group account"))?;
                    let writable = self.accounts.is_writable(index).unwrap_or(false);
                    accounts.push(ExpectedMeta { address, signer: false, writable });
                }
            } else {
                return Err(Prediction::Indeterminate("group range"));
            }
        }
        let data = self.build_cpi_data(registers, descriptor)?;
        self.touched.extend(accounts.iter().map(|meta| meta.address));
        let row_program = descriptor.program_account & ITERATION_ACCOUNT_BIT != 0;
        self.cpis.push(ExpectedCpi { program, accounts, data, row_program });
        self.mutated = true;
        Ok(())
    }

    /// `OPEN_REGISTRY` on a run that succeeds: an entry Ballista owns already is checked and costs
    /// no call; a missing one is created from the payer's lamports, with one `create_account`
    /// when its address holds none, or else a transfer of any shortfall to rent exemption, then
    /// `allocate` and `assign` (`processor/registry.rs`, `create_entry`). The entry's address and
    /// header are the run's to check; a run that gets past the open passed them.
    fn open_registry(&mut self, instruction: &InstructionRecord) -> Flow {
        let open = RegistryOpen::decode(instruction.immediate()).ok_or(Prediction::Indeterminate("open immediate"))?;
        let entry_index = instruction.a as usize;
        let payer_index = instruction.c as usize;
        let entry = self.accounts.key(entry_index).ok_or(Prediction::Indeterminate("entry account"))?;
        let payer = self.accounts.key(payer_index).ok_or(Prediction::Indeterminate("payer account"))?;
        // The entry's state decides the calls. A call that was passed the entry may have changed it
        // (a transfer to it, say), and so may an earlier open whose payer it was, so the model
        // stops there.
        if self.touched.contains(&entry) {
            return Err(Prediction::Indeterminate("open of an account a call was passed"));
        }
        let (Some(owner), Some(lamports)) = (self.accounts.owner(entry_index), self.accounts.lamports(entry_index)) else {
            return Err(Prediction::Indeterminate("entry state unknown"));
        };
        self.touched.extend([payer, entry]);
        self.mutated = true;
        self.opened.push((entry, REGISTRY_ENTRY_HEADER_LEN + open.size as usize));
        if owner == self.ballista {
            return Ok(());
        }
        let space = REGISTRY_ENTRY_HEADER_LEN + open.size as usize;
        let required = (self.rent)(space);
        let system = SYSTEM_PROGRAM_ADDRESS;
        let meta = |address, signer, writable| ExpectedMeta { address, signer, writable };
        let call = |accounts, data: Vec<u8>| ExpectedCpi { program: system, accounts, data: CpiData::Concrete(data), row_program: false };
        let space_le = (space as u64).to_le_bytes();
        if lamports == 0 {
            let mut data = 0u32.to_le_bytes().to_vec();
            data.extend_from_slice(&required.to_le_bytes());
            data.extend_from_slice(&space_le);
            data.extend_from_slice(&self.ballista);
            self.cpis.push(call(vec![meta(payer, true, true), meta(entry, true, true)], data));
        } else {
            let shortfall = required.saturating_sub(lamports);
            if shortfall > 0 {
                let mut data = 2u32.to_le_bytes().to_vec();
                data.extend_from_slice(&shortfall.to_le_bytes());
                self.cpis.push(call(vec![meta(payer, true, true), meta(entry, false, true)], data));
            }
            let mut data = 8u32.to_le_bytes().to_vec();
            data.extend_from_slice(&space_le);
            self.cpis.push(call(vec![meta(entry, true, true)], data));
            let mut data = 1u32.to_le_bytes().to_vec();
            data.extend_from_slice(&self.ballista);
            self.cpis.push(call(vec![meta(entry, true, true)], data));
        }
        Ok(())
    }

    /// Where each declared account group falls among the runtime accounts, from the run layout:
    /// after the fixed accounts and every batch row, in declaration order, each as long as the run
    /// data's prefix says.
    fn group_ranges(&self) -> Vec<(usize, usize)> {
        let header = self.program.header;
        let mut start = header.fixed_account_count() + self.iterations * header.batch_stride();
        let mut ranges = Vec::new();
        for &len in self.group_lengths.iter().take(header.account_group_count()) {
            ranges.push((start, len as usize));
            start += len as usize;
        }
        ranges
    }

    /// `GROUP_LENGTH`: how many members the caller put in the group, its own number from the run
    /// data's prefix.
    fn group_length(&mut self, instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let (_, len) = self.group_ranges().get(instruction.a as usize).copied().ok_or(Prediction::Indeterminate("group range"))?;
        self.group_values += 1;
        Ok(Val::U64(len as u64))
    }

    /// `GROUP_ANY` and `GROUP_COUNT`, from the wire format: a member matches when its owner is one
    /// of the one or two programs `b` and `c` name, its data holds at least the floor, its address
    /// is none of the except keys, and its data holds every match value, encoded as invocation
    /// data encodes it, at the match's offset. `GROUP_ANY` stops at the first match. A member that
    /// passes the owner, floor and except tests while an open holds it borrowed fails the run,
    /// since its data cannot be read: an open marks its entry for the rest of the run.
    ///
    /// The result is opaque when a member the model cannot follow could change it: one whose owner
    /// it does not know (a program, which its loader owns, or the sysvar), one whose data it does
    /// not know when the owner passes (the template, an account a probe instruction before the
    /// run changed), one a call was passed since the run began (which may have reassigned,
    /// resized or rewritten it), or one tested against a match value or except key it does not
    /// know. A known match still decides `GROUP_ANY`: the scan stops there or earlier, true
    /// either way.
    fn group_filter(&mut self, registers: &[Val], instruction: &InstructionRecord) -> Result<Val, Prediction> {
        let any = instruction.opcode == OP_GROUP_ANY;
        let program = |index: u8| {
            self.program.pubkeys.get(index as usize).map(|record| record.bytes).ok_or(Prediction::Indeterminate("group program"))
        };
        let first = program(instruction.b)?;
        let second = if instruction.c == NO_INDEX { first } else { program(instruction.c)? };
        let filter = GroupScan::decode(instruction.immediate());
        let (start, end) = filter.segment_range();
        let segments = self.program.data_segments.get(start..end).ok_or(Prediction::Indeterminate("group segments"))?;
        let (match_segments, except_segments) = segments.split_at(filter.matches as usize);
        // Each match's offset and its value's bytes, `None` when the value is opaque.
        let mut matches = Vec::new();
        for segment in match_segments {
            let bytes = match &registers[segment.register as usize] {
                Val::Opaque => None,
                value => Some(encode_register(segment.kind, value)?),
            };
            matches.push((segment.offset(), bytes));
        }
        let mut excepts = Vec::new();
        for segment in except_segments {
            excepts.push(match &registers[segment.register as usize] {
                Val::Pubkey(key) => Some(*key),
                Val::Opaque => None,
                _ => return Err(Prediction::Indeterminate("except key not a pubkey")),
            });
        }
        let floor = filter.min_data_len as usize;
        let (group_start, len) = self.group_ranges().get(instruction.a as usize).copied().ok_or(Prediction::Indeterminate("group range"))?;

        let mut count = 0u64;
        // Whether a member so far could have matched or not, as far as the model knows.
        let mut unknown = false;
        for index in group_start..group_start + len {
            let address = self.accounts.key(index).ok_or(Prediction::Indeterminate("group account"))?;
            let excepted = if excepts.contains(&Some(address)) {
                Some(true)
            } else if excepts.contains(&None) {
                None
            } else {
                Some(false)
            };
            if let Some(&(_, data_len)) = self.opened.iter().find(|(entry, _)| *entry == address) {
                // An opened entry is Ballista's and `data_len` long, and its mark stays.
                if (self.ballista != first && self.ballista != second) || data_len < floor || excepted == Some(true) {
                    continue;
                }
                // Whether the scan gets here depends on what the model does not know: an unknown
                // except key, or for `any` an unknown member before it that may have matched.
                if excepted.is_none() || (any && unknown) {
                    return Err(Prediction::Indeterminate("group filter may reach an opened entry"));
                }
                return Err(fails("group filter reached an entry an open holds", ACCOUNT_BORROW_FAILED));
            }
            let owner = match self.accounts.executable(index) {
                Some(false) if !self.touched.contains(&address) => self.accounts.owner(index),
                _ => None,
            };
            let Some(owner) = owner else {
                unknown = true;
                continue;
            };
            if owner != first && owner != second {
                continue;
            }
            let Some(data) = self.accounts.data(index) else {
                unknown = true;
                continue;
            };
            if data.len() < floor || excepted == Some(true) {
                continue;
            }
            let mut holds = Some(true);
            for (offset, bytes) in &matches {
                match bytes {
                    Some(bytes) if offset.checked_add(bytes.len()).and_then(|end| data.get(*offset..end)) != Some(bytes.as_slice()) => {
                        holds = Some(false);
                        break;
                    }
                    Some(_) => {}
                    None => holds = None,
                }
            }
            match (holds, excepted) {
                (Some(false), _) => {}
                (Some(true), Some(false)) => {
                    count += 1;
                    if any {
                        break;
                    }
                }
                _ => unknown = true,
            }
        }
        if unknown && !(any && count > 0) {
            return Ok(Val::Opaque);
        }
        self.group_values += 1;
        Ok(if any { Val::Bool(count > 0) } else { Val::U64(count) })
    }

    fn build_cpi_data(&self, registers: &[Val], descriptor: &CpiDescriptor) -> Result<CpiData, Prediction> {
        let start = descriptor.segment_start();
        let segments = &self.program.data_segments[start..start + descriptor.segment_len as usize];
        self.encode_segments(registers, segments)
    }

    fn build_output(&self, registers: &[Val], instruction: &InstructionRecord) -> Result<CpiData, Prediction> {
        let (start, count) = instruction.blob_range();
        let segments = &self.program.data_segments[start..start + count];
        self.encode_segments(registers, segments)
    }

    fn encode_segments(&self, registers: &[Val], segments: &[DataSegment]) -> Result<CpiData, Prediction> {
        let mut bytes = Vec::new();
        for segment in segments {
            if segment.kind == DATA_LITERAL {
                let data = self.program.blob.get(segment.offset()..segment.offset() + segment.len())
                    .ok_or(Prediction::Indeterminate("literal range"))?;
                bytes.extend_from_slice(data);
                continue;
            }
            let value = registers[segment.register as usize].clone();
            if matches!(value, Val::Opaque) {
                return Ok(CpiData::Opaque);
            }
            match encode_register(segment.kind, &value) {
                Ok(encoded) => bytes.extend_from_slice(&encoded),
                Err(stop) => return Err(stop),
            }
        }
        Ok(CpiData::Concrete(bytes))
    }
}

/// A typed read of account data the model knows: past the end, or a `bool` byte other than 0 or 1,
/// the run fails.
fn decode_read(opcode: u8, data: &[u8], offset: usize) -> Result<Val, Prediction> {
    let width = read_width(opcode);
    let Some(slice) = offset.checked_add(width).and_then(|end| data.get(offset..end)) else {
        return Err(fails("typed read past the account data", INVALID_RUNTIME_ACCOUNT));
    };
    Ok(match opcode {
        OP_READ_U8 => Val::U64(slice[0] as u64),
        OP_READ_U16 => Val::U64(u16::from_le_bytes(slice.try_into().unwrap()) as u64),
        OP_READ_U32 => Val::U64(u32::from_le_bytes(slice.try_into().unwrap()) as u64),
        OP_READ_U64 => Val::U64(u64::from_le_bytes(slice.try_into().unwrap())),
        OP_READ_I64 => Val::I64(i64::from_le_bytes(slice.try_into().unwrap())),
        OP_READ_I32 => Val::I64(i32::from_le_bytes(slice.try_into().unwrap()) as i64),
        OP_READ_U128 => Val::U128(u128::from_le_bytes(slice.try_into().unwrap())),
        OP_READ_PUBKEY => Val::Pubkey(slice.try_into().unwrap()),
        OP_READ_BOOL => match slice[0] {
            0 => Val::Bool(false),
            1 => Val::Bool(true),
            _ => return Err(fails("bool byte other than 0 or 1", TYPE_MISMATCH)),
        },
        _ => Val::Opaque,
    })
}

fn encode_register(kind: u8, value: &Val) -> Result<Vec<u8>, Prediction> {
    let as_u128 = |value: &Val| -> Result<u128, Prediction> {
        match value {
            Val::U64(v) => Ok(*v as u128),
            Val::U128(v) => Ok(*v),
            _ => Err(Prediction::Indeterminate("encode narrow type")),
        }
    };
    Ok(match kind {
        DATA_REG_U8 => {
            let value = as_u128(value)?;
            vec![u8::try_from(value).map_err(|_| fails("u8 narrow", ARITHMETIC_OVERFLOW))?]
        }
        DATA_REG_U16 => {
            let value = u16::try_from(as_u128(value)?).map_err(|_| fails("u16 narrow", ARITHMETIC_OVERFLOW))?;
            value.to_le_bytes().to_vec()
        }
        DATA_REG_U32 => {
            let value = u32::try_from(as_u128(value)?).map_err(|_| fails("u32 narrow", ARITHMETIC_OVERFLOW))?;
            value.to_le_bytes().to_vec()
        }
        DATA_REG_U64 => {
            let value = u64::try_from(as_u128(value)?).map_err(|_| fails("u64 narrow", ARITHMETIC_OVERFLOW))?;
            value.to_le_bytes().to_vec()
        }
        DATA_REG_I64 => match value {
            Val::I64(v) => v.to_le_bytes().to_vec(),
            _ => return Err(Prediction::Indeterminate("i64 encode type")),
        },
        DATA_REG_U128 => match value {
            Val::U128(v) => v.to_le_bytes().to_vec(),
            _ => return Err(Prediction::Indeterminate("u128 encode type")),
        },
        DATA_REG_PUBKEY => match value {
            Val::Pubkey(v) => v.to_vec(),
            _ => return Err(Prediction::Indeterminate("pubkey encode type")),
        },
        DATA_REG_BOOL => match value {
            Val::Bool(v) => vec![u8::from(*v)],
            _ => return Err(Prediction::Indeterminate("bool encode type")),
        },
        DATA_REG_BYTES => match value {
            Val::Bytes(v) => v.clone(),
            _ => return Err(Prediction::Indeterminate("bytes encode type")),
        },
        _ => return Err(Prediction::Indeterminate("unknown segment kind")),
    })
}

fn shift_u128(value: u128, shift: u64, width: u32, opcode: u8) -> Result<u128, Prediction> {
    if opcode == OP_SHL {
        if shift >= width as u64 {
            return if value == 0 { Ok(0) } else { Err(fails("shl drops a bit", ARITHMETIC_OVERFLOW)) };
        }
        let shifted = value << shift;
        // A set bit shifted past the width is lost.
        let mask = if width == 128 { u128::MAX } else { (1u128 << width) - 1 };
        if shifted & !mask != 0 || (shifted & mask) >> shift != value {
            return Err(fails("shl drops a bit", ARITHMETIC_OVERFLOW));
        }
        Ok(shifted & mask)
    } else {
        if shift >= width as u64 {
            Ok(0)
        } else {
            Ok(value >> shift)
        }
    }
}

fn mul_div(a: u128, b: u128, c: u128, ceil: bool) -> Option<u128> {
    if c == 0 {
        return None;
    }
    let product = a.checked_mul(b)?;
    if ceil {
        Some(product.div_ceil(c))
    } else {
        Some(product / c)
    }
}

fn mul_div_u128(a: u128, b: u128, c: u128, ceil: bool) -> Result<Val, Prediction> {
    if c == 0 {
        return Err(fails("mul_div by zero", DIVISION_BY_ZERO));
    }
    // The program holds `a * b` exactly in 256 bits and fails only if the quotient exceeds u128.
    // When the product already fits u128 the model computes it; when it overflows, the exact
    // quotient is beyond what the model needs, so it returns an opaque value rather than guess.
    match a.checked_mul(b) {
        Some(product) => Ok(Val::U128(if ceil { product.div_ceil(c) } else { product / c })),
        None => Ok(Val::Opaque),
    }
}

