use ballista_common::template::*;
use pinocchio::{
    cpi::invoke_with_bounds,
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{clock::Clock, Sysvar},
    AccountView, ProgramResult,
};
use solana_address::Address;

use crate::error::{vm_error, BallistaError};

/// A typed register value. Public for formal specifications; the module is private otherwise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeValue<'data> {
    Unset,
    Bool(bool),
    U64(u64),
    I64(i64),
    U128([u8; 16]),
    Pubkey([u8; 32]),
    Bytes(&'data [u8]),
}

/// A failure raised while running a template, before it is mapped to a program error.
///
/// VM failures carry their location into the custom error code so callers can tell which
/// instruction or account failed. Failures returned by the runtime or by an invoked program pass
/// through untouched so their codes are never confused with Ballista's.
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// A VM failure whose context (the program counter) is attached by the dispatch loop.
    Vm(BallistaError),
    /// A VM failure that already knows its context: an account or input index.
    VmAt(BallistaError, u16),
    /// A runtime or invoked-program failure.
    Program(ProgramError),
}

impl From<BallistaError> for RunError {
    fn from(kind: BallistaError) -> Self {
        RunError::Vm(kind)
    }
}

impl From<ProgramError> for RunError {
    fn from(error: ProgramError) -> Self {
        RunError::Program(error)
    }
}

pub type RunResult<T> = Result<T, RunError>;

impl RunError {
    /// Maps a failure raised while executing `instruction` at `pc`, logging the location.
    fn at(self, pc: usize, instruction: &InstructionRecord) -> ProgramError {
        match self {
            RunError::Program(error) => error,
            RunError::Vm(kind) => {
                log_failure(
                    pc as u64,
                    instruction.opcode as u64,
                    instruction.a as u64,
                    instruction.b as u64,
                    instruction.dst as u64,
                );
                vm_error(kind, clamp(pc))
            }
            RunError::VmAt(kind, context) => {
                log_failure(
                    pc as u64,
                    instruction.opcode as u64,
                    kind.code() as u64,
                    context as u64,
                    0,
                );
                vm_error(kind, context)
            }
        }
    }

    /// Maps a failure raised before the first instruction runs (input or account validation).
    fn before_execution(self) -> ProgramError {
        match self {
            RunError::Program(error) => error,
            RunError::Vm(kind) => {
                log_failure(u64::MAX, kind.code() as u64, 0, 0, 0);
                vm_error(kind, 0)
            }
            RunError::VmAt(kind, context) => {
                log_failure(u64::MAX, kind.code() as u64, context as u64, 0, 0);
                vm_error(kind, context)
            }
        }
    }
}

fn clamp(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// Logs five words on the failure path only. Off-chain this is a no-op.
#[inline(always)]
fn log_failure(a: u64, b: u64, c: u64, d: u64, e: u64) {
    #[cfg(target_os = "solana")]
    unsafe {
        pinocchio::syscalls::sol_log_64_(a, b, c, d, e);
    }
    #[cfg(not(target_os = "solana"))]
    let _ = (a, b, c, d, e);
}

/// Per-run state: buffers allocated once and reused by every CPI, so heap use does not grow with
/// the number of invocations (the default SBF allocator never frees), plus the invoke trace.
pub struct Scratch<'data> {
    metas: Vec<InstructionAccount<'data>>,
    views: Vec<&'data AccountView>,
    data: Vec<u8>,
    /// Program invoked by the most recent executed CPI, for return-data provenance checks.
    last_invoked: Option<[u8; 32]>,
    /// Invoke instructions reached so far, counted across loop iterations.
    expanded: u8,
    /// Bit `n` is set when the `n`th reached invoke actually ran (its guard was true).
    executed: u64,
    /// `(start, len)` of each account group within the runtime accounts, from the run layout.
    groups: [(u8, u8); MAX_ACCOUNT_GROUPS],
}

impl<'data> Scratch<'data> {
    pub fn new(program: &ProgramView<'data>) -> Self {
        let max_data = program
            .cpis
            .iter()
            .map(CpiDescriptor::max_data_len)
            .max()
            .unwrap_or(0)
            .min(MAX_CPI_DATA_LEN);
        Self {
            metas: Vec::with_capacity(MAX_CPI_ACCOUNTS),
            views: Vec::with_capacity(MAX_CPI_ACCOUNTS),
            data: Vec::with_capacity(max_data),
            last_invoked: None,
            expanded: 0,
            executed: 0,
            groups: [(0, 0); MAX_ACCOUNT_GROUPS],
        }
    }

    /// Records where each account group starts, so CPIs can forward them.
    pub fn set_groups(&mut self, layout: &RunLayout) {
        self.groups = layout.groups;
    }
}

/// Where one run's runtime accounts fall: the fixed accounts, `iterations` batch rows, then the
/// account groups in declaration order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunLayout {
    pub iterations: usize,
    /// Fixed accounts plus batch rows; the first group starts here.
    pub declared: usize,
    /// `(start, len)` of each declared group; unused slots are `(0, 0)`.
    pub groups: [(u8, u8); MAX_ACCOUNT_GROUPS],
}

/// Splits the account-group length prefix off the run data. Templates without groups have an
/// empty prefix.
pub fn split_group_prefix<'data>(
    program: &ProgramView<'_>,
    data: &'data [u8],
) -> RunResult<(&'data [u8], &'data [u8])> {
    data.split_at_checked(program.header.account_group_count())
        .ok_or(RunError::VmAt(BallistaError::InvalidRunInputs, 0))
}

pub fn run<'data>(
    program: &ProgramView<'data>,
    input_bytes: &'data [u8],
    runtime_accounts: &'data [AccountView],
    template_address: &Address,
) -> ProgramResult {
    let (group_lengths, input_bytes) =
        split_group_prefix(program, input_bytes).map_err(RunError::before_execution)?;
    let layout = validate_runtime_accounts(program, runtime_accounts, group_lengths)
        .map_err(RunError::before_execution)?;
    let inputs = parse_run_inputs(program, input_bytes, layout.iterations)
        .map_err(RunError::before_execution)?;
    let mut registers = vec![RuntimeValue::Unset; program.header.register_count()];
    let mut scratch = Scratch::new(program);
    scratch.set_groups(&layout);
    execute_root(
        program,
        &inputs,
        runtime_accounts,
        layout.iterations,
        &mut registers,
        &mut scratch,
    )?;
    if program.header.flags() & PROGRAM_FLAG_EMIT_EVENT != 0 {
        emit_event(&encode_event(
            layout.iterations,
            scratch.expanded,
            scratch.executed,
            template_address,
        ));
    }
    Ok(())
}

/// Magic prefix of the run event emitted through `sol_log_data` when the template opts in.
pub const EVENT_MAGIC: [u8; 4] = *b"BEV1";
/// Size of the run event: magic, bytecode version, iterations, expanded invokes, executed mask,
/// template address.
pub const EVENT_LEN: usize = 4 + 1 + 1 + 1 + 8 + 32;

/// Encodes the run event. Indexers decode it from the `Program data:` log line.
pub fn encode_event(
    iterations: usize,
    expanded: u8,
    executed: u64,
    template_address: &Address,
) -> [u8; EVENT_LEN] {
    let mut event = [0u8; EVENT_LEN];
    event[..4].copy_from_slice(&EVENT_MAGIC);
    event[4] = TEMPLATE_PROGRAM_VERSION;
    event[5] = u8::try_from(iterations).unwrap_or(u8::MAX);
    event[6] = expanded;
    event[7..15].copy_from_slice(&executed.to_le_bytes());
    event[15..].copy_from_slice(template_address.as_ref());
    event
}

#[inline(always)]
fn emit_event(event: &[u8]) {
    #[cfg(target_os = "solana")]
    unsafe {
        let slices: [&[u8]; 1] = [event];
        pinocchio::syscalls::sol_log_data(slices.as_ptr() as *const u8, 1);
    }
    #[cfg(not(target_os = "solana"))]
    let _ = event;
}

/// Decodes the fixed inputs followed by `iterations` copies of the row inputs.
pub fn parse_run_inputs<'data>(
    program: &ProgramView<'_>,
    data: &'data [u8],
    iterations: usize,
) -> RunResult<Vec<RuntimeValue<'data>>> {
    let fixed = program.header.input_count();
    let row = program.inputs.get(fixed..).unwrap_or(&[]);
    let descriptors = program.inputs[..fixed.min(program.inputs.len())]
        .iter()
        .chain((0..iterations).flat_map(|_| row.iter()));
    let count = fixed + iterations * row.len();
    parse_sequence(descriptors, count, data)
}

/// Decodes a flat sequence of input values.
#[cfg(any(test, feature = "spec-api"))]
pub fn parse_inputs<'data>(
    descriptors: &[InputDescriptor],
    data: &'data [u8],
) -> RunResult<Vec<RuntimeValue<'data>>> {
    parse_sequence(descriptors.iter(), descriptors.len(), data)
}

fn parse_sequence<'data, 'd>(
    descriptors: impl Iterator<Item = &'d InputDescriptor>,
    count: usize,
    mut data: &'data [u8],
) -> RunResult<Vec<RuntimeValue<'data>>> {
    let mut inputs = vec![RuntimeValue::Unset; count];
    for (index, descriptor) in descriptors.enumerate() {
        let fail = || RunError::VmAt(BallistaError::InvalidRunInputs, clamp(index));
        let value = match descriptor.value_type {
            VALUE_BOOL => {
                let (value, remaining) = take::<1>(data).map_err(|_| fail())?;
                data = remaining;
                match value[0] {
                    0 => RuntimeValue::Bool(false),
                    1 => RuntimeValue::Bool(true),
                    _ => return Err(fail()),
                }
            }
            VALUE_U64 => {
                let (value, remaining) = take::<8>(data).map_err(|_| fail())?;
                data = remaining;
                RuntimeValue::U64(u64::from_le_bytes(*value))
            }
            VALUE_I64 => {
                let (value, remaining) = take::<8>(data).map_err(|_| fail())?;
                data = remaining;
                RuntimeValue::I64(i64::from_le_bytes(*value))
            }
            VALUE_U128 => {
                let (value, remaining) = take::<16>(data).map_err(|_| fail())?;
                data = remaining;
                RuntimeValue::U128(*value)
            }
            VALUE_PUBKEY => {
                let (value, remaining) = take::<32>(data).map_err(|_| fail())?;
                data = remaining;
                RuntimeValue::Pubkey(*value)
            }
            VALUE_BYTES => {
                let (len, remaining) = take::<2>(data).map_err(|_| fail())?;
                let len = u16::from_le_bytes(*len) as usize;
                let (bytes, remaining) = remaining.split_at_checked(len).ok_or_else(fail)?;
                if len > descriptor.max_len() {
                    return Err(fail());
                }
                data = remaining;
                RuntimeValue::Bytes(bytes)
            }
            _ => return Err(fail()),
        };
        inputs[index] = value;
    }
    if !data.is_empty() {
        return Err(RunError::VmAt(BallistaError::InvalidRunInputs, clamp(count)));
    }
    Ok(inputs)
}

/// Checks the runtime accounts against the schema and returns where everything falls.
///
/// `group_lengths` is the run data prefix: one byte per declared account group. Group accounts sit
/// after the batch rows and carry no constraints; everything before them must match the schema.
pub fn validate_runtime_accounts(
    program: &ProgramView<'_>,
    accounts: &[AccountView],
    group_lengths: &[u8],
) -> RunResult<RunLayout> {
    let range_error = |context: usize| RunError::VmAt(BallistaError::InvalidAccountRange, clamp(context));
    if accounts.len() > MAX_RUNTIME_ACCOUNTS {
        return Err(range_error(accounts.len()));
    }
    if group_lengths.len() != program.header.account_group_count() {
        return Err(RunError::VmAt(BallistaError::InvalidRunInputs, 0));
    }
    let group_total: usize = group_lengths.iter().map(|len| *len as usize).sum();
    let fixed = program.header.fixed_account_count();
    let stride = program.header.batch_stride();
    let rows = accounts
        .len()
        .checked_sub(fixed)
        .and_then(|rest| rest.checked_sub(group_total))
        .ok_or_else(|| range_error(accounts.len()))?;
    let iterations = if stride == 0 {
        if rows != 0 {
            return Err(range_error(accounts.len()));
        }
        0
    } else {
        if rows % stride != 0 {
            return Err(range_error(accounts.len()));
        }
        let iterations = rows / stride;
        if iterations > program.header.batch_max_iterations()
            || iterations < program.header.batch_min_iterations()
        {
            return Err(range_error(iterations));
        }
        iterations
    };
    let declared = fixed + rows;
    let mut groups = [(0u8, 0u8); MAX_ACCOUNT_GROUPS];
    let mut start = declared;
    for (slot, len) in groups.iter_mut().zip(group_lengths) {
        *slot = (clamp(start) as u8, *len);
        start += *len as usize;
    }

    for (index, account) in accounts[..declared].iter().enumerate() {
        let constraint_index = if index < fixed {
            index
        } else {
            fixed + (index - fixed) % stride
        };
        let constraint = program
            .accounts
            .get(constraint_index)
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        validate_account(program, account, constraint).map_err(|error| match error {
            RunError::Vm(BallistaError::InvalidRuntimeAccount) => {
                RunError::VmAt(BallistaError::AccountConstraintFailed, clamp(index))
            }
            RunError::Vm(kind) => RunError::VmAt(kind, clamp(index)),
            other => other,
        })?;
    }
    Ok(RunLayout {
        iterations,
        declared,
        groups,
    })
}

pub fn validate_account(
    program: &ProgramView<'_>,
    account: &AccountView,
    constraint: &AccountConstraint,
) -> RunResult<()> {
    if constraint.flags & ACCOUNT_SIGNER != 0 && !account.is_signer() {
        return Err(ProgramError::MissingRequiredSignature.into());
    }
    if constraint.flags & ACCOUNT_WRITABLE != 0 && !account.is_writable() {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    if constraint.flags & ACCOUNT_EXECUTABLE != 0 && !account.executable() {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    if constraint.address_index != NO_INDEX {
        let expected = program
            .pubkeys
            .get(constraint.address_index as usize)
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        if account.address().as_ref() != expected.bytes.as_slice() {
            return Err(BallistaError::InvalidRuntimeAccount.into());
        }
    }
    if constraint.owner_index != NO_INDEX {
        let expected = program
            .pubkeys
            .get(constraint.owner_index as usize)
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        if account.owner().as_ref() != expected.bytes.as_slice() {
            return Err(BallistaError::InvalidRuntimeAccount.into());
        }
    }
    if account.data_len() < constraint.min_data_len() {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    Ok(())
}

fn execute_root<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    iterations: usize,
    registers: &mut [RuntimeValue<'data>],
    scratch: &mut Scratch<'data>,
) -> ProgramResult {
    let mut pc = 0usize;
    while pc < program.instructions.len() {
        let instruction = &program.instructions[pc];
        if instruction.opcode == OP_FOREACH {
            let body_start = pc + 1;
            let body_end = body_start
                .checked_add(instruction.a as usize)
                .filter(|end| *end <= program.instructions.len())
                .ok_or_else(|| {
                    RunError::from(BallistaError::InvalidTemplateProgram).at(pc, instruction)
                })?;
            // Registers written inside the body are discarded after each iteration, except the
            // ones named in the carry mask, which flow into the next iteration and out of the loop.
            let carry = instruction.immediate();
            let mut base_registers = registers.to_vec();
            for iteration in 0..iterations {
                registers.copy_from_slice(&base_registers);
                let row_base = program.header.fixed_account_count()
                    + iteration * program.header.batch_stride();
                execute_range(
                    program,
                    inputs,
                    accounts,
                    registers,
                    scratch,
                    body_start,
                    body_end,
                    Some((iteration, row_base)),
                )?;
                if carry != 0 {
                    for (register, slot) in base_registers.iter_mut().enumerate() {
                        if register < 64 && carry & (1u64 << register) != 0 {
                            *slot = registers[register];
                        }
                    }
                }
            }
            registers.copy_from_slice(&base_registers);
            pc = body_end;
            continue;
        }
        execute_instruction(
            program,
            inputs,
            accounts,
            registers,
            scratch,
            instruction,
            None,
        )
        .map_err(|error| error.at(pc, instruction))?;
        pc += 1;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_range<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    registers: &mut [RuntimeValue<'data>],
    scratch: &mut Scratch<'data>,
    start: usize,
    end: usize,
    loop_context: Option<(usize, usize)>,
) -> ProgramResult {
    for pc in start..end {
        let instruction = &program.instructions[pc];
        if instruction.opcode == OP_FOREACH {
            return Err(RunError::from(BallistaError::InvalidTemplateProgram).at(pc, instruction));
        }
        execute_instruction(
            program,
            inputs,
            accounts,
            registers,
            scratch,
            instruction,
            loop_context,
        )
        .map_err(|error| error.at(pc, instruction))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn execute_instruction<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    registers: &mut [RuntimeValue<'data>],
    scratch: &mut Scratch<'data>,
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> RunResult<()> {
    let dst = instruction.dst as usize;
    match instruction.opcode {
        OP_LOAD_INPUT => {
            let index = if instruction.a & ITERATION_INPUT_BIT == 0 {
                instruction.a as usize
            } else {
                let (iteration, _) = loop_context.ok_or(BallistaError::InvalidTemplateProgram)?;
                let offset = (instruction.a & !ITERATION_INPUT_BIT) as usize;
                if offset >= program.header.row_input_count() {
                    return Err(BallistaError::InvalidTemplateProgram.into());
                }
                program.header.input_count() + iteration * program.header.row_input_count() + offset
            };
            let value = *inputs
                .get(index)
                .ok_or(BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, value)?;
        }
        OP_CONST_BOOL => set(registers, dst, RuntimeValue::Bool(instruction.a != 0))?,
        OP_CONST_U64 => set(registers, dst, RuntimeValue::U64(instruction.immediate()))?,
        OP_CONST_I64 => set(
            registers,
            dst,
            RuntimeValue::I64(i64::from_le_bytes(instruction.immediate_le)),
        )?,
        OP_CONST_U128 => {
            let bytes: &[u8; 16] = blob_range(program, instruction)?
                .try_into()
                .map_err(|_| BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, RuntimeValue::U128(*bytes))?;
        }
        OP_CONST_PUBKEY => {
            let pubkey = program
                .pubkeys
                .get(instruction.a as usize)
                .ok_or(BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, RuntimeValue::Pubkey(pubkey.bytes))?;
        }
        OP_CONST_BYTES => {
            let bytes = blob_range(program, instruction)?;
            set(registers, dst, RuntimeValue::Bytes(bytes))?;
        }
        OP_ACCOUNT_KEY => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(
                registers,
                dst,
                RuntimeValue::Pubkey(account.address().to_bytes()),
            )?;
        }
        OP_ACCOUNT_OWNER => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(
                registers,
                dst,
                RuntimeValue::Pubkey(account.owner().to_bytes()),
            )?;
        }
        OP_ACCOUNT_LAMPORTS => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::U64(account.lamports()))?;
        }
        OP_ACCOUNT_DATA_LEN => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::U64(account.data_len() as u64))?;
        }
        OP_ACCOUNT_IS_EMPTY => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::Bool(account.is_data_empty()))?;
        }
        OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_U128
        | OP_READ_PUBKEY | OP_READ_BOOL => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            let offset = if instruction.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0 {
                match get(registers, instruction.b)? {
                    RuntimeValue::U64(value) => usize::try_from(value)
                        .map_err(|_| BallistaError::InvalidRuntimeAccount)?,
                    _ => return Err(BallistaError::TypeMismatch.into()),
                }
            } else {
                instruction.immediate() as usize
            };
            let data = account.try_borrow()?;
            let value = read_value(instruction.opcode, &data, offset)?;
            set(registers, dst, value)?;
        }
        OP_CLOCK_SLOT => set(registers, dst, RuntimeValue::U64(Clock::get()?.slot))?,
        OP_CLOCK_TIMESTAMP => set(
            registers,
            dst,
            RuntimeValue::I64(Clock::get()?.unix_timestamp),
        )?,
        OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX => {
            let left = get(registers, instruction.a)?;
            let right = get(registers, instruction.b)?;
            set(registers, dst, arithmetic(instruction.opcode, left, right)?)?;
        }
        OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => {
            let left = get(registers, instruction.a)?;
            let right = get(registers, instruction.b)?;
            let result = compare(instruction.opcode, left, right)?;
            set(registers, dst, RuntimeValue::Bool(result))?;
        }
        OP_AND | OP_OR => {
            let left = as_bool(get(registers, instruction.a)?)?;
            let right = as_bool(get(registers, instruction.b)?)?;
            set(
                registers,
                dst,
                RuntimeValue::Bool(if instruction.opcode == OP_AND {
                    left && right
                } else {
                    left || right
                }),
            )?;
        }
        OP_NOT => set(
            registers,
            dst,
            RuntimeValue::Bool(!as_bool(get(registers, instruction.a)?)?),
        )?,
        OP_SELECT => {
            let condition = as_bool(get(registers, instruction.a)?)?;
            let selected = if condition {
                get(registers, instruction.b)?
            } else {
                get(registers, instruction.c)?
            };
            set(registers, dst, selected)?;
        }
        OP_CAST_U64 | OP_CAST_I64 | OP_CAST_U128 => {
            let value = cast(instruction.opcode, get(registers, instruction.a)?)?;
            set(registers, dst, value)?;
        }
        OP_LOOP_INDEX => {
            let (iteration, _) = loop_context.ok_or(BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, RuntimeValue::U64(iteration as u64))?;
        }
        OP_MOVE => {
            let value = get(registers, instruction.a)?;
            set(registers, dst, value)?;
        }
        OP_DERIVE_PDA => {
            let program_account = resolve_account(program, accounts, instruction.a, loop_context)?;
            let (start, count) = instruction.blob_range();
            if count == 0 || count > MAX_PDA_SEEDS {
                return Err(BallistaError::InvalidTemplateProgram.into());
            }
            let end = start
                .checked_add(count)
                .ok_or(BallistaError::InvalidTemplateProgram)?;
            let segments = program
                .data_segments
                .get(start..end)
                .ok_or(BallistaError::InvalidTemplateProgram)?;
            let mut storage = [[0u8; MAX_PDA_SEED_LEN]; MAX_PDA_SEEDS];
            let mut lengths = [0usize; MAX_PDA_SEEDS];
            for (slot, segment) in segments.iter().enumerate() {
                let mut sink = FixedSink::new(&mut storage[slot]);
                encode_segment(program, registers, segment, &mut sink)?;
                lengths[slot] = sink.len;
            }
            let mut seeds: [&[u8]; MAX_PDA_SEEDS] = [&[]; MAX_PDA_SEEDS];
            for slot in 0..count {
                seeds[slot] = &storage[slot][..lengths[slot]];
            }
            let (derived, _) =
                Address::try_find_program_address(&seeds[..count], program_account.address())
                    .ok_or(BallistaError::InvalidPdaDerivation)?;
            set(registers, dst, RuntimeValue::Pubkey(derived.to_bytes()))?;
        }
        OP_REQUIRE => {
            if !as_bool(get(registers, instruction.a)?)? {
                return Err(BallistaError::RequirementFailed.into());
            }
        }
        OP_RETURN_DATA => read_return_data(scratch, instruction, registers)?,
        OP_INVOKE => {
            let slot = scratch.expanded;
            scratch.expanded = scratch.expanded.saturating_add(1);
            scratch.last_invoked = None;
            if instruction.b != NO_INDEX && !as_bool(get(registers, instruction.b)?)? {
                return Ok(());
            }
            invoke_cpi(
                program,
                accounts,
                registers,
                instruction.a as usize,
                loop_context,
                scratch,
            )?;
            if slot < 64 {
                scratch.executed |= 1u64 << slot;
            }
        }
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    }
    Ok(())
}

/// Reads a typed value from the return data of the CPI that just ran.
///
/// Kept out of line, and reading through the syscall directly into one buffer: SBPF version 0
/// gives every function a fixed 4 KiB frame, and pinocchio's `get_return_data` returns a
/// kilobyte-sized struct by value, which the compiler copied several times and overflowed that
/// frame.
#[inline(never)]
fn read_return_data<'data>(
    scratch: &Scratch<'data>,
    instruction: &InstructionRecord,
    registers: &mut [RuntimeValue<'data>],
) -> RunResult<()> {
    let expected_program = scratch
        .last_invoked
        .ok_or(BallistaError::MissingReturnData)?;
    let mut buffer = [0u8; MAX_RETURN_DATA_LEN];
    let mut program_id = [0u8; 32];
    let size = fetch_return_data(&mut buffer, &mut program_id);
    if size == 0 {
        return Err(BallistaError::MissingReturnData.into());
    }
    if program_id != expected_program {
        return Err(BallistaError::ReturnDataMismatch.into());
    }
    let bytes = &buffer[..size.min(MAX_RETURN_DATA_LEN)];
    let offset = instruction.immediate() as usize;
    let width = read_width(instruction.a);
    if width == 0
        || offset
            .checked_add(width)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(BallistaError::MissingReturnData.into());
    }
    let value = read_value(instruction.a, bytes, offset)?;
    set(registers, instruction.dst as usize, value)
}

/// Copies the current return data into `buffer` and its setter into `program_id`, returning the
/// full size the runtime reports (zero when no return data is set). Off-chain there is none.
#[inline(always)]
fn fetch_return_data(buffer: &mut [u8; MAX_RETURN_DATA_LEN], program_id: &mut [u8; 32]) -> usize {
    #[cfg(target_os = "solana")]
    {
        let size = unsafe {
            pinocchio::syscalls::sol_get_return_data(
                buffer.as_mut_ptr(),
                buffer.len() as u64,
                program_id.as_mut_ptr(),
            )
        };
        size as usize
    }
    #[cfg(not(target_os = "solana"))]
    {
        let _ = (buffer, program_id);
        0
    }
}

/// The blob slice addressed by an instruction's packed `(offset, len)` immediate.
fn blob_range<'data>(
    program: &ProgramView<'data>,
    instruction: &InstructionRecord,
) -> RunResult<&'data [u8]> {
    let (offset, len) = instruction.blob_range();
    let end = offset
        .checked_add(len)
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    program
        .blob
        .get(offset..end)
        .ok_or_else(|| BallistaError::InvalidTemplateProgram.into())
}

pub fn read_value<'data>(opcode: u8, data: &[u8], offset: usize) -> RunResult<RuntimeValue<'data>> {
    Ok(match opcode {
        OP_READ_U8 => RuntimeValue::U64(read_array::<1>(data, offset)?[0] as u64),
        OP_READ_U16 => RuntimeValue::U64(u16::from_le_bytes(*read_array(data, offset)?) as u64),
        OP_READ_U32 => RuntimeValue::U64(u32::from_le_bytes(*read_array(data, offset)?) as u64),
        OP_READ_U64 => RuntimeValue::U64(u64::from_le_bytes(*read_array(data, offset)?)),
        OP_READ_I64 => RuntimeValue::I64(i64::from_le_bytes(*read_array(data, offset)?)),
        OP_READ_U128 => RuntimeValue::U128(*read_array(data, offset)?),
        OP_READ_PUBKEY => RuntimeValue::Pubkey(*read_array(data, offset)?),
        OP_READ_BOOL => match read_array::<1>(data, offset)?[0] {
            0 => RuntimeValue::Bool(false),
            1 => RuntimeValue::Bool(true),
            _ => return Err(BallistaError::TypeMismatch.into()),
        },
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    })
}

fn invoke_cpi<'data>(
    program: &ProgramView<'data>,
    accounts: &'data [AccountView],
    registers: &[RuntimeValue<'data>],
    cpi_index: usize,
    loop_context: Option<(usize, usize)>,
    scratch: &mut Scratch<'data>,
) -> RunResult<()> {
    let descriptor = program
        .cpis
        .get(cpi_index)
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    let account_len = descriptor.account_len as usize;
    if account_len > MAX_CPI_ACCOUNTS {
        return Err(BallistaError::InvalidTemplateProgram.into());
    }
    let account_end = descriptor
        .account_start()
        .checked_add(account_len)
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    let records = program
        .cpi_accounts
        .get(descriptor.account_start()..account_end)
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    let segment_end = descriptor
        .segment_start()
        .checked_add(descriptor.segment_len as usize)
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    let segments = program
        .data_segments
        .get(descriptor.segment_start()..segment_end)
        .ok_or(BallistaError::InvalidTemplateProgram)?;

    scratch.metas.clear();
    scratch.views.clear();
    scratch.data.clear();
    for record in records {
        let account = resolve_account(program, accounts, record.account, loop_context)?;
        scratch.metas.push(InstructionAccount::new(
            account.address(),
            record.flags & ACCOUNT_WRITABLE != 0,
            record.flags & ACCOUNT_SIGNER != 0,
        ));
        scratch.views.push(account);
    }
    if let Some(group) = descriptor.account_group() {
        // Group accounts are forwarded with the transaction's writable flag and never as
        // signers: a template delegates signatures only through declared slots.
        let (start, len) = *scratch
            .groups
            .get(group)
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        let total = account_len + len as usize;
        if total > MAX_CPI_ACCOUNTS {
            return Err(RunError::VmAt(
                BallistaError::CpiAccountLimitExceeded,
                clamp(total),
            ));
        }
        let range = start as usize..start as usize + len as usize;
        let group_accounts = accounts
            .get(range)
            .ok_or(BallistaError::InvalidRuntimeAccount)?;
        for account in group_accounts {
            scratch.metas.push(InstructionAccount::new(
                account.address(),
                account.is_writable(),
                false,
            ));
            scratch.views.push(account);
        }
    }
    for segment in segments {
        encode_segment(program, registers, segment, &mut scratch.data)?;
    }
    if scratch.data.len() > descriptor.max_data_len() || scratch.data.len() > MAX_CPI_DATA_LEN {
        return Err(BallistaError::CpiDataTooLarge.into());
    }

    let program_account =
        resolve_account(program, accounts, descriptor.program_account, loop_context)?;
    let instruction = InstructionView {
        program_id: program_account.address(),
        accounts: scratch.metas.as_slice(),
        data: scratch.data.as_slice(),
    };
    bounded_invoke(&instruction, scratch.views.as_slice())?;
    scratch.last_invoked = Some(program_account.address().to_bytes());
    Ok(())
}

/// Performs the CPI in its own stack frame. `invoke_with_bounds` places a `MAX_CPI_ACCOUNTS`-slot
/// account array on the stack; combined with the caller's locals that exceeded the fixed 4 KiB
/// frame of SBPF version 0, so the array gets a frame to itself.
#[inline(never)]
fn bounded_invoke(instruction: &InstructionView, views: &[&AccountView]) -> ProgramResult {
    invoke_with_bounds::<MAX_CPI_ACCOUNTS, _>(instruction, views)
}

/// Destination for encoded segment bytes: a `Vec` for CPI data or a fixed stack buffer for PDA
/// seeds. Spec builds keep `push_bytes` out of line so the prover can summarize the copy inside.
pub trait ByteSink {
    fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()>;
}

impl ByteSink for Vec<u8> {
    #[cfg_attr(feature = "spec-api", inline(never))] fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

pub struct FixedSink<'buffer> {
    buffer: &'buffer mut [u8],
    pub len: usize,
}

impl<'buffer> FixedSink<'buffer> {
    pub fn new(buffer: &'buffer mut [u8]) -> Self {
        Self { buffer, len: 0 }
    }
}

impl ByteSink for FixedSink<'_> {
    #[cfg_attr(feature = "spec-api", inline(never))] fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= self.buffer.len())
            .ok_or(BallistaError::InvalidPdaDerivation)?;
        self.buffer[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }
}

/// Encodes one CPI data segment or PDA seed into `sink`.
pub fn encode_segment<'data, S: ByteSink>(
    program: &ProgramView<'data>,
    registers: &[RuntimeValue<'data>],
    segment: &DataSegment,
    sink: &mut S,
) -> RunResult<()> {
    if segment.kind == DATA_LITERAL {
        let end = segment
            .offset()
            .checked_add(segment.len())
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        let bytes = program
            .blob
            .get(segment.offset()..end)
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        return sink.push_bytes(bytes);
    }
    encode_register_segment(registers, segment, sink)
}

/// Encodes a register-backed data segment. Literal segments are handled by `encode_segment`.
pub fn encode_register_segment<'data, S: ByteSink>(
    registers: &[RuntimeValue<'data>],
    segment: &DataSegment,
    sink: &mut S,
) -> RunResult<()> {
    match segment.kind {
        DATA_REG_U8 => {
            let value = as_u128(get(registers, segment.register)?)?;
            let byte = u8::try_from(value).map_err(|_| BallistaError::ArithmeticOverflow)?;
            sink.push_bytes(&[byte])
        }
        DATA_REG_U16 => {
            let value = u16::try_from(as_u128(get(registers, segment.register)?)?)
                .map_err(|_| BallistaError::ArithmeticOverflow)?;
            sink.push_bytes(&value.to_le_bytes())
        }
        DATA_REG_U32 => {
            let value = u32::try_from(as_u128(get(registers, segment.register)?)?)
                .map_err(|_| BallistaError::ArithmeticOverflow)?;
            sink.push_bytes(&value.to_le_bytes())
        }
        DATA_REG_U64 => {
            let value = u64::try_from(as_u128(get(registers, segment.register)?)?)
                .map_err(|_| BallistaError::ArithmeticOverflow)?;
            sink.push_bytes(&value.to_le_bytes())
        }
        DATA_REG_I64 => match get(registers, segment.register)? {
            RuntimeValue::I64(value) => sink.push_bytes(&value.to_le_bytes()),
            _ => Err(BallistaError::TypeMismatch.into()),
        },
        DATA_REG_U128 => match get(registers, segment.register)? {
            RuntimeValue::U128(value) => sink.push_bytes(&value),
            _ => Err(BallistaError::TypeMismatch.into()),
        },
        DATA_REG_PUBKEY => match get(registers, segment.register)? {
            RuntimeValue::Pubkey(value) => sink.push_bytes(&value),
            _ => Err(BallistaError::TypeMismatch.into()),
        },
        DATA_REG_BOOL => {
            let flag = as_bool(get(registers, segment.register)?)?;
            sink.push_bytes(&[u8::from(flag)])
        }
        DATA_REG_BYTES => match get(registers, segment.register)? {
            RuntimeValue::Bytes(value) => sink.push_bytes(value),
            _ => Err(BallistaError::TypeMismatch.into()),
        },
        _ => Err(BallistaError::InvalidTemplateProgram.into()),
    }
}

pub fn resolve_account<'data>(
    program: &ProgramView<'_>,
    accounts: &'data [AccountView],
    reference: u8,
    loop_context: Option<(usize, usize)>,
) -> RunResult<&'data AccountView> {
    let index = if reference & ITERATION_ACCOUNT_BIT == 0 {
        let index = reference as usize;
        if index >= program.header.fixed_account_count() {
            return Err(BallistaError::InvalidRuntimeAccount.into());
        }
        index
    } else {
        let (_, row_base) = loop_context.ok_or(BallistaError::InvalidRuntimeAccount)?;
        let offset = (reference & !ITERATION_ACCOUNT_BIT) as usize;
        if offset >= program.header.batch_stride() {
            return Err(BallistaError::InvalidRuntimeAccount.into());
        }
        row_base + offset
    };
    accounts
        .get(index)
        .ok_or_else(|| BallistaError::InvalidRuntimeAccount.into())
}

pub fn arithmetic<'data>(
    opcode: u8,
    left: RuntimeValue<'data>,
    right: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    macro_rules! checked {
        ($left:expr, $right:expr) => {{
            let value = match opcode {
                OP_ADD => $left.checked_add($right),
                OP_SUB => $left.checked_sub($right),
                OP_MUL => $left.checked_mul($right),
                OP_DIV => $left.checked_div($right),
                OP_MIN => Some($left.min($right)),
                OP_MAX => Some($left.max($right)),
                _ => None,
            };
            value.ok_or_else(|| {
                RunError::from(if opcode == OP_DIV && $right == 0 {
                    BallistaError::DivisionByZero
                } else {
                    BallistaError::ArithmeticOverflow
                })
            })?
        }};
    }
    Ok(match (left, right) {
        (RuntimeValue::U64(left), RuntimeValue::U64(right)) => {
            RuntimeValue::U64(checked!(left, right))
        }
        (RuntimeValue::I64(left), RuntimeValue::I64(right)) => {
            RuntimeValue::I64(checked!(left, right))
        }
        (RuntimeValue::U128(left), RuntimeValue::U128(right)) => {
            let left = u128::from_le_bytes(left);
            let right = u128::from_le_bytes(right);
            RuntimeValue::U128(checked!(left, right).to_le_bytes())
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

pub fn compare(opcode: u8, left: RuntimeValue<'_>, right: RuntimeValue<'_>) -> RunResult<bool> {
    macro_rules! compare_values {
        ($left:expr, $right:expr) => {
            match opcode {
                OP_EQ => $left == $right,
                OP_NE => $left != $right,
                OP_LT => $left < $right,
                OP_LTE => $left <= $right,
                OP_GT => $left > $right,
                OP_GTE => $left >= $right,
                _ => return Err(BallistaError::InvalidTemplateProgram.into()),
            }
        };
    }
    Ok(match (left, right) {
        (RuntimeValue::Bool(left), RuntimeValue::Bool(right)) => {
            if !matches!(opcode, OP_EQ | OP_NE) {
                return Err(BallistaError::TypeMismatch.into());
            }
            compare_values!(left, right)
        }
        (RuntimeValue::U64(left), RuntimeValue::U64(right)) => compare_values!(left, right),
        (RuntimeValue::I64(left), RuntimeValue::I64(right)) => compare_values!(left, right),
        (RuntimeValue::U128(left), RuntimeValue::U128(right)) => {
            compare_values!(u128::from_le_bytes(left), u128::from_le_bytes(right))
        }
        (RuntimeValue::Pubkey(left), RuntimeValue::Pubkey(right)) => {
            if !matches!(opcode, OP_EQ | OP_NE) {
                return Err(BallistaError::TypeMismatch.into());
            }
            compare_values!(left, right)
        }
        (RuntimeValue::Bytes(left), RuntimeValue::Bytes(right)) => {
            if !matches!(opcode, OP_EQ | OP_NE) {
                return Err(BallistaError::TypeMismatch.into());
            }
            compare_values!(left, right)
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

pub fn cast(opcode: u8, value: RuntimeValue<'_>) -> RunResult<RuntimeValue<'_>> {
    match opcode {
        OP_CAST_U64 => Ok(RuntimeValue::U64(match value {
            RuntimeValue::U64(value) => value,
            RuntimeValue::I64(value) => {
                u64::try_from(value).map_err(|_| BallistaError::ArithmeticOverflow)?
            }
            RuntimeValue::U128(value) => u64::try_from(u128::from_le_bytes(value))
                .map_err(|_| BallistaError::ArithmeticOverflow)?,
            _ => return Err(BallistaError::TypeMismatch.into()),
        })),
        OP_CAST_I64 => Ok(RuntimeValue::I64(match value {
            RuntimeValue::U64(value) => {
                i64::try_from(value).map_err(|_| BallistaError::ArithmeticOverflow)?
            }
            RuntimeValue::I64(value) => value,
            RuntimeValue::U128(value) => i64::try_from(u128::from_le_bytes(value))
                .map_err(|_| BallistaError::ArithmeticOverflow)?,
            _ => return Err(BallistaError::TypeMismatch.into()),
        })),
        OP_CAST_U128 => Ok(RuntimeValue::U128(match value {
            RuntimeValue::U64(value) => (value as u128).to_le_bytes(),
            RuntimeValue::I64(value) => u128::try_from(value)
                .map_err(|_| BallistaError::ArithmeticOverflow)?
                .to_le_bytes(),
            RuntimeValue::U128(value) => value,
            _ => return Err(BallistaError::TypeMismatch.into()),
        })),
        _ => Err(BallistaError::InvalidTemplateProgram.into()),
    }
}

pub fn set<'data>(
    registers: &mut [RuntimeValue<'data>],
    index: usize,
    value: RuntimeValue<'data>,
) -> RunResult<()> {
    let register = registers
        .get_mut(index)
        .ok_or(BallistaError::InvalidRegister)?;
    *register = value;
    Ok(())
}

pub fn get<'data>(registers: &[RuntimeValue<'data>], index: u8) -> RunResult<RuntimeValue<'data>> {
    match registers.get(index as usize).copied() {
        Some(RuntimeValue::Unset) | None => Err(BallistaError::InvalidRegister.into()),
        Some(value) => Ok(value),
    }
}

pub fn as_bool(value: RuntimeValue<'_>) -> RunResult<bool> {
    match value {
        RuntimeValue::Bool(value) => Ok(value),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

pub fn as_u128(value: RuntimeValue<'_>) -> RunResult<u128> {
    match value {
        RuntimeValue::U64(value) => Ok(value as u128),
        RuntimeValue::U128(value) => Ok(u128::from_le_bytes(value)),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

pub fn read_array<const N: usize>(data: &[u8], offset: usize) -> RunResult<&[u8; N]> {
    offset
        .checked_add(N)
        .and_then(|end| data.get(offset..end))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| BallistaError::InvalidRuntimeAccount.into())
}

fn take<const N: usize>(data: &[u8]) -> RunResult<(&[u8; N], &[u8])> {
    let (bytes, remaining) = data
        .split_at_checked(N)
        .ok_or(BallistaError::InvalidRunInputs)?;
    Ok((
        bytes
            .try_into()
            .map_err(|_| BallistaError::InvalidRunInputs)?,
        remaining,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use RuntimeValue::*;

    fn err(kind: BallistaError) -> RunError {
        RunError::Vm(kind)
    }

    fn input_err(index: u16) -> RunError {
        RunError::VmAt(BallistaError::InvalidRunInputs, index)
    }

    fn segment(kind: u8, register: u8) -> DataSegment {
        DataSegment {
            kind,
            register,
            offset_le: [0; 2],
            len_le: [0; 2],
            reserved: [0; 2],
        }
    }

    fn descriptor(value_type: u8, max_len: u16) -> InputDescriptor {
        InputDescriptor {
            value_type,
            reserved: 0,
            max_len_le: max_len.to_le_bytes(),
        }
    }

    fn instruction(opcode: u8) -> InstructionRecord {
        record(opcode, NO_INDEX, 0, 0, 0, 0, 0)
    }

    #[test]
    fn runtime_value_storage_is_bounded() {
        let value_size = core::mem::size_of::<RuntimeValue<'static>>();
        assert!(value_size <= 40);
    }

    #[test]
    fn run_events_have_a_fixed_documented_layout() {
        let template = Address::new_from_array([7; 32]);
        let event = encode_event(3, 5, 0b10110, &template);
        assert_eq!(event.len(), EVENT_LEN);
        assert_eq!(&event[..4], b"BEV1");
        assert_eq!(event[4], TEMPLATE_PROGRAM_VERSION);
        assert_eq!(event[5], 3, "iterations");
        assert_eq!(event[6], 5, "expanded invokes");
        assert_eq!(&event[7..15], &0b10110u64.to_le_bytes(), "executed mask");
        assert_eq!(&event[15..], &[7; 32]);
        assert_eq!(encode_event(300, 0, 0, &template)[5], u8::MAX, "iterations clamp");
    }

    #[test]
    fn vm_errors_carry_the_program_counter_and_pass_callee_errors_through() {
        let require = instruction(OP_REQUIRE);
        assert_eq!(
            err(BallistaError::RequirementFailed).at(7, &require),
            ProgramError::Custom((7 << 16) | BallistaError::RequirementFailed.code())
        );
        assert_eq!(
            RunError::VmAt(BallistaError::InvalidRuntimeAccount, 3).at(7, &require),
            ProgramError::Custom((3 << 16) | BallistaError::InvalidRuntimeAccount.code())
        );
        assert_eq!(
            RunError::Program(ProgramError::Custom(6001)).at(7, &require),
            ProgramError::Custom(6001),
            "an invoked program's own 6001 must not be rewritten"
        );
        assert_eq!(
            RunError::Program(ProgramError::MissingRequiredSignature).before_execution(),
            ProgramError::MissingRequiredSignature
        );
        assert_eq!(
            input_err(2).before_execution(),
            ProgramError::Custom((2 << 16) | BallistaError::InvalidRunInputs.code())
        );
        assert_eq!(
            err(BallistaError::InvalidAccountRange).at(70_000, &require),
            ProgramError::Custom((0xffff << 16) | BallistaError::InvalidAccountRange.code()),
            "program counters clamp to 16 bits"
        );
    }

    #[test]
    fn arithmetic_checks_every_numeric_type() {
        assert_eq!(arithmetic(OP_ADD, U64(1), U64(2)), Ok(U64(3)));
        assert_eq!(
            arithmetic(OP_ADD, U64(u64::MAX), U64(1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(
            arithmetic(OP_SUB, U64(1), U64(2)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(arithmetic(OP_MUL, I64(-3), I64(4)), Ok(I64(-12)));
        assert_eq!(
            arithmetic(OP_MUL, I64(i64::MIN), I64(-1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(arithmetic(OP_DIV, I64(7), I64(-2)), Ok(I64(-3)));
        assert_eq!(
            arithmetic(OP_DIV, U64(7), U64(0)),
            Err(err(BallistaError::DivisionByZero))
        );
        assert_eq!(
            arithmetic(OP_DIV, I64(i64::MIN), I64(-1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(arithmetic(OP_MIN, I64(-1), I64(1)), Ok(I64(-1)));
        assert_eq!(arithmetic(OP_MAX, U64(1), U64(9)), Ok(U64(9)));
        let max = U128(u128::MAX.to_le_bytes());
        let one = U128(1u128.to_le_bytes());
        assert_eq!(
            arithmetic(OP_ADD, max, one),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(
            arithmetic(OP_SUB, max, one),
            Ok(U128((u128::MAX - 1).to_le_bytes()))
        );
        assert_eq!(
            arithmetic(OP_ADD, U64(1), I64(1)),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            arithmetic(OP_ADD, Bool(true), Bool(true)),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            arithmetic(OP_ADD, Pubkey([1; 32]), Pubkey([1; 32])),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            arithmetic(OP_EQ, U64(1), U64(1)),
            Err(err(BallistaError::ArithmeticOverflow)),
            "a non-arithmetic opcode falls through the checked macro as overflow"
        );
    }

    #[test]
    fn comparisons_restrict_ordering_to_numbers() {
        assert_eq!(compare(OP_LT, I64(-5), I64(3)), Ok(true));
        assert_eq!(compare(OP_GTE, U64(3), U64(3)), Ok(true));
        assert_eq!(compare(OP_GT, U64(3), U64(3)), Ok(false));
        assert_eq!(
            compare(
                OP_LTE,
                U128(2u128.to_le_bytes()),
                U128(3u128.to_le_bytes())
            ),
            Ok(true)
        );
        assert_eq!(compare(OP_NE, Bool(true), Bool(false)), Ok(true));
        assert_eq!(
            compare(OP_LT, Bool(false), Bool(true)),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(compare(OP_EQ, Pubkey([1; 32]), Pubkey([1; 32])), Ok(true));
        assert_eq!(
            compare(OP_GT, Pubkey([2; 32]), Pubkey([1; 32])),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(compare(OP_EQ, Bytes(&[1, 2]), Bytes(&[1, 2])), Ok(true));
        assert_eq!(compare(OP_NE, Bytes(&[1, 2]), Bytes(&[1, 2, 3])), Ok(true));
        assert_eq!(
            compare(OP_LTE, Bytes(&[1]), Bytes(&[2])),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            compare(OP_EQ, U64(1), U128(1u128.to_le_bytes())),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            compare(OP_ADD, U64(1), U64(1)),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
    }

    #[test]
    fn casts_cover_every_pair_and_reject_out_of_range() {
        assert_eq!(
            cast(OP_CAST_U64, I64(-1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(cast(OP_CAST_U64, I64(5)), Ok(U64(5)));
        assert_eq!(cast(OP_CAST_U64, U64(5)), Ok(U64(5)));
        assert_eq!(
            cast(OP_CAST_U64, U128((u64::MAX as u128 + 1).to_le_bytes())),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(
            cast(OP_CAST_U64, U128((u64::MAX as u128).to_le_bytes())),
            Ok(U64(u64::MAX))
        );
        assert_eq!(
            cast(OP_CAST_I64, U64(u64::MAX)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(cast(OP_CAST_I64, U64(7)), Ok(I64(7)));
        assert_eq!(cast(OP_CAST_I64, I64(-7)), Ok(I64(-7)));
        assert_eq!(cast(OP_CAST_I64, U128(7u128.to_le_bytes())), Ok(I64(7)));
        assert_eq!(
            cast(OP_CAST_U128, I64(-1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(cast(OP_CAST_U128, I64(1)), Ok(U128(1u128.to_le_bytes())));
        assert_eq!(cast(OP_CAST_U128, U64(9)), Ok(U128(9u128.to_le_bytes())));
        assert_eq!(cast(OP_CAST_U128, U128([3; 16])), Ok(U128([3; 16])));
        assert_eq!(
            cast(OP_CAST_U64, Bool(true)),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            cast(OP_CAST_I64, Pubkey([0; 32])),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            cast(OP_CAST_U128, Bytes(&[1])),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            cast(OP_ADD, U64(1)),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
    }

    #[test]
    fn data_segments_encode_each_kind_and_reject_narrowing_overflow() {
        let registers = [
            U64(300),
            U128(1u128.to_le_bytes()),
            I64(-2),
            Pubkey([7; 32]),
            Bool(true),
            Bytes(&[9, 9]),
            U64(5),
            U128((u64::MAX as u128 + 1).to_le_bytes()),
        ];
        let encode = |kind: u8, register: u8| {
            let mut output = Vec::new();
            encode_register_segment(&registers, &segment(kind, register), &mut output)
                .map(|_| output)
        };
        assert_eq!(
            encode(DATA_REG_U8, 0),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(encode(DATA_REG_U8, 6), Ok(vec![5]));
        assert_eq!(encode(DATA_REG_U16, 0), Ok(vec![44, 1]));
        assert_eq!(encode(DATA_REG_U32, 6), Ok(vec![5, 0, 0, 0]));
        assert_eq!(encode(DATA_REG_U64, 1), Ok(1u64.to_le_bytes().to_vec()));
        assert_eq!(
            encode(DATA_REG_U64, 7),
            Err(err(BallistaError::ArithmeticOverflow)),
            "a u128 above u64::MAX cannot narrow to a u64 segment"
        );
        assert_eq!(encode(DATA_REG_I64, 2), Ok((-2i64).to_le_bytes().to_vec()));
        assert_eq!(encode(DATA_REG_PUBKEY, 3), Ok(vec![7; 32]));
        assert_eq!(encode(DATA_REG_BOOL, 4), Ok(vec![1]));
        assert_eq!(encode(DATA_REG_BYTES, 5), Ok(vec![9, 9]));
        assert_eq!(
            encode(DATA_REG_U128, 1),
            Ok(1u128.to_le_bytes().to_vec())
        );
        assert_eq!(
            encode(DATA_REG_I64, 0),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            encode(DATA_REG_U64, 2),
            Err(err(BallistaError::TypeMismatch)),
            "signed registers never encode as unsigned"
        );
        assert_eq!(
            encode(DATA_REG_U128, 0),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            encode(DATA_REG_PUBKEY, 5),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            encode(DATA_REG_BOOL, 0),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            encode(DATA_REG_BYTES, 0),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            encode(DATA_REG_U8, 9),
            Err(err(BallistaError::InvalidRegister)),
            "out-of-range register"
        );
        assert_eq!(
            encode(0xfe, 0),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
    }

    #[test]
    fn fixed_sinks_reject_seeds_over_thirty_two_bytes() {
        let registers = [Bytes(&[1; 20]), Pubkey([2; 32])];
        let mut buffer = [0u8; MAX_PDA_SEED_LEN];
        let mut sink = FixedSink::new(&mut buffer);
        encode_register_segment(&registers, &segment(DATA_REG_BYTES, 0), &mut sink).unwrap();
        assert_eq!(sink.len, 20);
        assert_eq!(
            encode_register_segment(&registers, &segment(DATA_REG_BYTES, 0), &mut sink),
            Err(err(BallistaError::InvalidPdaDerivation)),
            "a second twenty-byte push overflows the seed"
        );
        let mut buffer = [0u8; MAX_PDA_SEED_LEN];
        let mut sink = FixedSink::new(&mut buffer);
        encode_register_segment(&registers, &segment(DATA_REG_PUBKEY, 1), &mut sink).unwrap();
        assert_eq!(sink.len, 32);
        assert_eq!(buffer, [2; 32]);
    }

    #[test]
    fn input_parsing_covers_each_type_and_rejects_malformed_bytes() {
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BOOL, 0)], &[1]).unwrap()[0],
            Bool(true)
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BOOL, 0)], &[0]).unwrap()[0],
            Bool(false)
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BOOL, 0)], &[2]),
            Err(input_err(0))
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_U64, 0)], &7u64.to_le_bytes()).unwrap()[0],
            U64(7)
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_I64, 0)], &(-7i64).to_le_bytes()).unwrap()[0],
            I64(-7)
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_U128, 0)], &[4; 16]).unwrap()[0],
            U128([4; 16])
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_PUBKEY, 0)], &[5; 32]).unwrap()[0],
            Pubkey([5; 32])
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BYTES, 4)], &[2, 0, 8, 9]).unwrap()[0],
            Bytes(&[8, 9])
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BYTES, 4)], &[0, 0]).unwrap()[0],
            Bytes(&[])
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BYTES, 1)], &[2, 0, 8, 9]),
            Err(input_err(0)),
            "longer than the declared maximum"
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_BYTES, 4)], &[5, 0, 8, 9]),
            Err(input_err(0)),
            "length prefix past the end"
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_U64, 0)], &[1, 2, 3]),
            Err(input_err(0)),
            "truncated"
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_U64, 0)], &[0; 9]),
            Err(input_err(1)),
            "trailing byte is reported after the last input"
        );
        assert_eq!(
            parse_inputs(&[descriptor(VALUE_U64, 0), descriptor(VALUE_BOOL, 0)], &[0; 8]),
            Err(input_err(1)),
            "the missing second input is named"
        );
        assert_eq!(
            parse_inputs(&[descriptor(0xfe, 0)], &[0]),
            Err(input_err(0))
        );
        let two = parse_inputs(
            &[descriptor(VALUE_BOOL, 0), descriptor(VALUE_U64, 0)],
            &[1, 9, 0, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
        assert_eq!(two, vec![Bool(true), U64(9)]);
        assert!(parse_inputs(&[], &[]).unwrap().is_empty());
    }

    #[test]
    fn run_inputs_carry_a_row_per_iteration() {
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(4, 0);
        let fee = builder.input(VALUE_U64, 0);
        let amount = builder.row_input(VALUE_U64, 0);
        let memo = builder.row_input(VALUE_BYTES, 4);
        builder.for_each(0, |body| {
            body.load_input(fee);
            body.load_input(amount);
            body.load_input(memo);
        });
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();

        let mut data = 7u64.to_le_bytes().to_vec();
        data.extend_from_slice(&10u64.to_le_bytes());
        data.extend_from_slice(&[1, 0, 0xaa]);
        data.extend_from_slice(&20u64.to_le_bytes());
        data.extend_from_slice(&[0, 0]);
        let inputs = parse_run_inputs(&program, &data, 2).unwrap();
        assert_eq!(
            inputs,
            vec![U64(7), U64(10), Bytes(&[0xaa]), U64(20), Bytes(&[])]
        );
        assert_eq!(parse_run_inputs(&program, &7u64.to_le_bytes(), 0).unwrap(), vec![U64(7)]);
        assert_eq!(
            parse_run_inputs(&program, &data[..8 + 8 + 3 + 8], 2),
            Err(input_err(4)),
            "the missing value is named by its running index"
        );
        assert_eq!(
            parse_run_inputs(&program, &data, 1),
            Err(input_err(3)),
            "a spare row is trailing data"
        );

        // Inside the loop a row reference resolves against the current iteration; outside it is a
        // structural error, as is an offset past the row.
        let mut registers = vec![Unset; 3];
        let mut scratch = Scratch::new(&program);
        let load = record(OP_LOAD_INPUT, 0, amount, NO_INDEX, NO_INDEX, 0, 0);
        execute_instruction(&program, &inputs, &[], &mut registers, &mut scratch, &load, Some((1, 0)))
            .unwrap();
        assert_eq!(registers[0], U64(20));
        assert_eq!(
            execute_instruction(&program, &inputs, &[], &mut registers, &mut scratch, &load, None),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
        let past = record(OP_LOAD_INPUT, 0, ITERATION_INPUT_BIT | 2, NO_INDEX, NO_INDEX, 0, 0);
        assert_eq!(
            execute_instruction(&program, &inputs, &[], &mut registers, &mut scratch, &past, Some((0, 0))),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
        let fixed = record(OP_LOAD_INPUT, 1, fee, NO_INDEX, NO_INDEX, 0, 0);
        execute_instruction(&program, &inputs, &[], &mut registers, &mut scratch, &fixed, None).unwrap();
        assert_eq!(registers[1], U64(7));
    }

    #[test]
    fn group_prefix_is_one_byte_per_declared_group() {
        let mut builder = ProgramBuilder::new();
        builder.account_groups(2);
        builder.const_bool(true);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(
            split_group_prefix(&program, &[3, 0, 9]).unwrap(),
            (&[3u8, 0][..], &[9u8][..])
        );
        assert_eq!(split_group_prefix(&program, &[3]), Err(input_err(0)));

        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(
            split_group_prefix(&program, &[9]).unwrap(),
            (&[][..], &[9u8][..])
        );
    }

    #[test]
    fn register_and_byte_access_reject_unset_and_out_of_range() {
        let mut registers = vec![Unset; 2];
        assert_eq!(get(&registers, 0), Err(err(BallistaError::InvalidRegister)));
        assert_eq!(get(&registers, 2), Err(err(BallistaError::InvalidRegister)));
        set(&mut registers, 1, U64(1)).unwrap();
        assert_eq!(get(&registers, 1), Ok(U64(1)));
        assert_eq!(
            set(&mut registers, 2, U64(1)),
            Err(err(BallistaError::InvalidRegister))
        );
        assert_eq!(as_bool(Bool(true)), Ok(true));
        assert_eq!(as_bool(U64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(as_u128(U64(3)), Ok(3));
        assert_eq!(as_u128(U128(4u128.to_le_bytes())), Ok(4));
        assert_eq!(as_u128(I64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(
            read_array::<4>(&[1, 2, 3], 0),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
        assert_eq!(
            read_array::<2>(&[1, 2, 3], usize::MAX),
            Err(err(BallistaError::InvalidRuntimeAccount)),
            "offset overflow is not a panic"
        );
        assert_eq!(read_array::<2>(&[1, 2, 3], 1), Ok(&[2, 3]));
        assert_eq!(
            take::<2>(&[1]),
            Err(err(BallistaError::InvalidRunInputs))
        );
        assert_eq!(take::<1>(&[1, 2]), Ok((&[1], &[2][..])));
    }

    #[test]
    fn typed_reads_cover_every_width_and_reject_unknown_opcodes() {
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];
        assert_eq!(read_value(OP_READ_U8, &data, 2), Ok(U64(3)));
        assert_eq!(read_value(OP_READ_U16, &data, 0), Ok(U64(0x0201)));
        assert_eq!(read_value(OP_READ_U32, &data, 0), Ok(U64(0x0403_0201)));
        assert_eq!(
            read_value(OP_READ_U64, &data, 1),
            Ok(U64(u64::from_le_bytes([2, 3, 4, 5, 6, 7, 8, 9])))
        );
        assert_eq!(
            read_value(OP_READ_I64, &data, 0),
            Ok(I64(i64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8])))
        );
        assert_eq!(read_value(OP_READ_U128, &data, 1), Ok(U128([2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17])));
        assert_eq!(
            read_value(OP_READ_U128, &data, 2),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
        assert_eq!(
            read_value(OP_READ_PUBKEY, &data, 0),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
        assert_eq!(read_value(OP_READ_BOOL, &data, 0), Ok(Bool(true)));
        assert_eq!(
            read_value(OP_READ_BOOL, &data, 1),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(
            read_value(OP_ADD, &data, 0),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
    }
}
