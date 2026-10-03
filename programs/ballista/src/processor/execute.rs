use core::mem::MaybeUninit;

use ballista_common::template::*;
use pinocchio::{
    cpi::{invoke_signed_unchecked, CpiAccount},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{clock::Clock, Sysvar},
    AccountView, ProgramResult,
};
use solana_address::Address;

use super::{introspect, math, registry};
use crate::error::{vm_error, BallistaError};
use crate::utils::pda;

/// A typed register value. Public for formal specifications; the module is private otherwise.
///
/// `repr(C, u8)` puts every payload at offset 8, after the one-byte tag. With Rust's default
/// layout the byte-array payloads start at offset 1 while the integers start at 8, so the compiler
/// split every copy and every store of a value into unaligned byte, half, and word pieces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
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
/// through unchanged, including custom codes that fall in Ballista's own range: Anchor programs
/// number their errors from 6000 too (Orca, Jupiter, Kamino and marginfi among them), so a
/// callee's 6001 is the same number as Ballista's `InvalidTemplateAccount`. A code on its own
/// does not say which program raised it; the transaction logs show which program failed.
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
    ///
    /// Out of line and cold: a run fails at most once, so the dispatch loop keeps only a branch to
    /// here and none of the logging in its own body.
    #[cold]
    #[inline(never)]
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
    #[cold]
    #[inline(never)]
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

/// Per-run state: buffers allocated once and reused by every CPI and every loop, so heap use does
/// not grow with the number of invocations or loops (the default SBF allocator never frees), plus
/// the invoke trace.
pub struct Scratch<'data> {
    metas: Vec<InstructionAccount<'data>>,
    views: Vec<&'data AccountView>,
    data: Vec<u8>,
    /// The bytes an `EMIT` or `SET_RETURN_DATA` encodes. Never `data`: inside a batch, `data` can
    /// hold a loop-invariant invocation payload that the next row sends again without encoding
    /// it, and an output written there would replace it.
    ///
    /// `None` until the run's first output, which allocates the buffer at `MAX_RETURN_DATA_LEN`
    /// bytes, the most any verified output encodes, for every later output to reuse. It is leaked
    /// rather than owned: the SBF heap never frees, so that costs nothing, and a null reference is
    /// the cheapest field to initialize. Every run pays for that store, one compute unit here
    /// against two for an `Option<Vec<u8>>` and three or four for an empty `Vec<u8>`.
    output: Option<&'data mut Vec<u8>>,
    /// Program invoked by the most recent executed CPI, for return-data provenance checks. A
    /// reference into the account, not a copy: an address never changes during a run.
    last_invoked: Option<&'data Address>,
    /// Invoke instructions reached so far, counted across loop iterations.
    expanded: u8,
    /// Bit `n` is set when the `n`th reached invoke actually ran (its guard was true).
    executed: u64,
    /// `(start, len)` of each account group within the runtime accounts, from the run layout.
    groups: [(u8, u8); MAX_ACCOUNT_GROUPS],
    /// The one descriptor a batch body invokes, when it invokes exactly one. Resolving an
    /// account costs about 30 compute units and a batch rebuilds the same list on every row, so
    /// the repeat pass re-resolves only the slots naming a row account. A body that invokes more
    /// than one descriptor would overwrite the list each time, so it opts out instead.
    cache_cpi: Option<usize>,
    /// Whether `metas` and `views` currently hold that descriptor's accounts.
    built: bool,
    /// Whether that descriptor's data bytes are the same on every row, so `data` can be kept as
    /// well as the account list. Encoding them again costs more than resolving the accounts.
    data_invariant: bool,
    /// What the repeat pass needs from that descriptor, taken when its list was built.
    built_call: Option<BuiltCall<'data>>,
    /// The registers as the running loop found them, plus every value it has carried so far; each
    /// pass starts from them. Loops never nest, so at most one runs at a time and one buffer
    /// serves every loop of a run: the first allocates it, and every later one copies into it.
    snapshot: Vec<RuntimeValue<'data>>,
}

/// The parts of a cached descriptor the repeat pass reads, looked up and checked once when its
/// account list is built rather than again on every row.
#[derive(Clone, Copy)]
struct BuiltCall<'data> {
    records: &'data [CpiAccountRecord],
    segments: &'data [DataSegment],
    max_data_len: usize,
    program_account: u8,
    /// The program account, resolved once when it is not a row account.
    program: Option<&'data AccountView>,
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
            output: None,
            last_invoked: None,
            expanded: 0,
            executed: 0,
            groups: [(0, 0); MAX_ACCOUNT_GROUPS],
            cache_cpi: None,
            built: false,
            data_invariant: false,
            built_call: None,
            snapshot: Vec::new(),
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

/// Runs `program` against `accounts`, the instruction's whole account list: the template's account
/// first, then the runtime accounts the template names. Taking the list whole keeps the arguments
/// to five words, all passed in registers. The template's address as a sixth argument went on the
/// stack, and handing it on cost every run 2 compute units; this costs none, and the runs without
/// a CPI came out 1 unit cheaper.
pub fn run<'data>(
    program: &ProgramView<'data>,
    input_bytes: &'data [u8],
    accounts: &'data [AccountView],
) -> ProgramResult {
    let [template, runtime_accounts @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let (group_lengths, input_bytes) =
        split_group_prefix(program, input_bytes).map_err(RunError::before_execution)?;
    let layout = validate_runtime_accounts(program, runtime_accounts, group_lengths)
        .map_err(RunError::before_execution)?;
    crate::profile::mark(crate::profile::TAG_ACCOUNTS_VALIDATED);
    let inputs = parse_run_inputs(program, input_bytes, layout.iterations)
        .map_err(RunError::before_execution)?;
    crate::profile::mark(crate::profile::TAG_INPUTS_PARSED);
    let mut registers = vec![RuntimeValue::Unset; program.header.register_count()];
    crate::profile::mark(crate::profile::TAG_REGISTERS_ALLOCATED);
    let mut scratch = Scratch::new(program);
    scratch.set_groups(&layout);
    crate::profile::mark(crate::profile::TAG_STATE_ALLOCATED);
    execute_root(
        program,
        &inputs,
        runtime_accounts,
        layout.iterations,
        &mut registers,
        &mut scratch,
        Some(template),
    )?;
    crate::profile::mark(crate::profile::TAG_EXECUTED);
    if program.header.flags() & PROGRAM_FLAG_EMIT_EVENT != 0 {
        emit_event(&encode_event(
            layout.iterations,
            scratch.expanded,
            scratch.executed,
            template.address(),
        ));
    }
    Ok(())
}

/// Magic prefix of the run event emitted through `sol_log_data` when the template opts in. Its
/// first three bytes are `RUN_EVENT_TAG_FAMILY`, which the verifier refuses as an `EMIT` tag, so no
/// template can log a line that reads as a run event. A later version keeps them.
pub const EVENT_MAGIC: [u8; 4] = *b"BEV1";
/// Size of the run event: magic, bytecode version, iterations, expanded invokes, executed mask,
/// template address.
pub const EVENT_LEN: usize = 4 + 1 + 1 + 1 + 8 + 32;

/// Encodes the run event. Indexers decode it from the `Program data:` log line. `iterations` is
/// the number of batch rows the run was given; a count loop's passes are not counted in it.
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

/// Logs `event` as one `Program data:` field: the run event, and every `EMIT`.
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
    // `iterations` is bounded by the runtime account count, so this cannot overflow. Checked
    // arithmetic would cost a 128-bit multiply on SBF.
    let count = fixed.wrapping_add(iterations.wrapping_mul(row.len()));
    let mut reader = InputReader::new(data, count);
    reader.read_all(&program.inputs[..fixed.min(program.inputs.len())])?;
    for _ in 0..iterations {
        reader.read_all(row)?;
    }
    reader.finish(count)
}

/// Decodes a flat sequence of input values.
#[cfg(any(test, feature = "spec-api"))]
pub fn parse_inputs<'data>(
    descriptors: &[InputDescriptor],
    data: &'data [u8],
) -> RunResult<Vec<RuntimeValue<'data>>> {
    let mut reader = InputReader::new(data, descriptors.len());
    reader.read_all(descriptors)?;
    reader.finish(descriptors.len())
}

/// Decodes run input values in order, each straight into its slot.
struct InputReader<'data> {
    data: &'data [u8],
    values: Vec<RuntimeValue<'data>>,
}

impl<'data> InputReader<'data> {
    #[inline(always)]
    fn new(data: &'data [u8], count: usize) -> Self {
        // A verified template declares at most `MAX_INPUT_VALUES` values; the cap lets the
        // compiler drop the allocation's overflow check. Anything past it would still fit, after
        // one reallocation.
        Self {
            data,
            values: Vec::with_capacity(count.min(MAX_INPUT_VALUES)),
        }
    }

    /// Decodes one value per descriptor. A failure names the value's running index.
    #[inline(always)]
    fn read_all(&mut self, descriptors: &[InputDescriptor]) -> RunResult<()> {
        for descriptor in descriptors {
            if !self.read(descriptor) {
                return Err(RunError::VmAt(
                    BallistaError::InvalidRunInputs,
                    clamp(self.values.len()),
                ));
            }
        }
        Ok(())
    }

    /// Decodes the value `descriptor` names from the front of the remaining bytes, returning
    /// false when they do not hold a valid value of that type. Each arm stores its value
    /// straight into the next slot rather than through a copy of the whole enum.
    #[inline(always)]
    fn read(&mut self, descriptor: &InputDescriptor) -> bool {
        let data = self.data;
        let rest = match descriptor.value_type {
            VALUE_U64 => {
                let Some((bytes, rest)) = data.split_first_chunk::<8>() else {
                    return false;
                };
                self.values
                    .push(RuntimeValue::U64(u64::from_le_bytes(*bytes)));
                rest
            }
            VALUE_PUBKEY => {
                let Some((bytes, rest)) = data.split_first_chunk::<32>() else {
                    return false;
                };
                self.values.push(RuntimeValue::Pubkey(*bytes));
                rest
            }
            VALUE_BOOL => {
                let Some((&byte, rest)) = data.split_first() else {
                    return false;
                };
                let value = match byte {
                    0 => false,
                    1 => true,
                    _ => return false,
                };
                self.values.push(RuntimeValue::Bool(value));
                rest
            }
            VALUE_I64 => {
                let Some((bytes, rest)) = data.split_first_chunk::<8>() else {
                    return false;
                };
                self.values
                    .push(RuntimeValue::I64(i64::from_le_bytes(*bytes)));
                rest
            }
            VALUE_U128 => {
                let Some((bytes, rest)) = data.split_first_chunk::<16>() else {
                    return false;
                };
                self.values.push(RuntimeValue::U128(*bytes));
                rest
            }
            VALUE_BYTES => {
                let Some((len, rest)) = data.split_first_chunk::<2>() else {
                    return false;
                };
                let len = u16::from_le_bytes(*len) as usize;
                if len > descriptor.max_len() {
                    return false;
                }
                let Some((bytes, rest)) = rest.split_at_checked(len) else {
                    return false;
                };
                self.values.push(RuntimeValue::Bytes(bytes));
                rest
            }
            _ => return false,
        };
        self.data = rest;
        true
    }

    /// Rejects trailing bytes, naming the index after the last value.
    #[inline(always)]
    fn finish(mut self, count: usize) -> RunResult<Vec<RuntimeValue<'data>>> {
        if !self.data.is_empty() {
            return Err(RunError::VmAt(
                BallistaError::InvalidRunInputs,
                clamp(count),
            ));
        }
        // Only an inputs table shorter than its header leaves values undecoded, and they read as
        // unset, as they always have.
        if self.values.len() < count {
            self.values.resize(count, RuntimeValue::Unset);
        }
        Ok(self.values)
    }
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

/// What every instruction of one run can read or write, gathered so the dispatch loop takes one
/// pointer instead of seven arguments: SBF passes only five in registers and spills the rest to
/// the stack on every call.
struct Machine<'run, 'data> {
    program: &'run ProgramView<'data>,
    inputs: &'run [RuntimeValue<'data>],
    accounts: &'data [AccountView],
    registers: &'run mut [RuntimeValue<'data>],
    scratch: &'run mut Scratch<'data>,
    /// Batch rows supplied to this run.
    iterations: usize,
    /// The running template's account, which registry entries are keyed by. `None` when a test or
    /// a specification runs one instruction on its own through `execute_instruction`; an open then
    /// fails.
    template: Option<&'data AccountView>,
}

fn execute_root<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    iterations: usize,
    registers: &mut [RuntimeValue<'data>],
    scratch: &mut Scratch<'data>,
    template: Option<&'data AccountView>,
) -> ProgramResult {
    let mut machine = Machine {
        program,
        inputs,
        accounts,
        registers,
        scratch,
        iterations,
        template,
    };
    dispatch(&mut machine)
}

/// The dispatch loop, for the whole program including every loop body. `step` is inlined here, so
/// an instruction costs a dispatch rather than a call. A loop body runs in this same loop: reaching
/// the end of the body hands over to `next_pass`, which rewinds to the body's first
/// instruction until every pass has run, so a pass costs no call and no frame.
#[inline(never)]
fn dispatch<'data>(machine: &mut Machine<'_, 'data>) -> ProgramResult {
    let instructions = machine.program.instructions;
    let mut rest = instructions;
    let mut current: Option<Loop> = None;
    let mut loop_context: Option<(usize, usize)> = None;
    loop {
        let [instruction, tail @ ..] = rest else {
            // The end of the program, or of one pass over a loop body.
            let Some(active) = &mut current else {
                return Ok(());
            };
            loop_context = next_pass(machine, active);
            rest = if loop_context.is_some() {
                &instructions[active.body_start..active.body_end]
            } else {
                let body_end = active.body_end;
                current = None;
                &instructions[body_end..]
            };
            continue;
        };
        if let Err(error) = step(machine, instruction, loop_context) {
            let pc = index_of(instructions, instruction);
            // `step` fails a loop instruction, as an opcode it does not run, before reading an
            // operand. At the root that failure is where the loop starts, which keeps both loop
            // kinds off the path of every other instruction. Only the root can hold a loop: inside
            // a body the failure stands.
            if matches!(instruction.opcode, OP_FOREACH | OP_REPEAT) && loop_context.is_none() {
                let (start, end) = enter_loop(machine, pc, instruction, &mut current)?;
                loop_context = current.as_ref().map(|active| (active.pass, active.row_base));
                rest = &instructions[start..end];
                continue;
            }
            return Err(error.at(pc, instruction));
        }
        rest = tail;
    }
}

/// The index of `instruction` within `instructions`, which holds it. Only a failure or a loop
/// entry needs it, so the dispatch loop walks a slice instead of counting.
#[inline(always)]
fn index_of(instructions: &[InstructionRecord], instruction: &InstructionRecord) -> usize {
    (instruction as *const InstructionRecord as usize)
        .wrapping_sub(instructions.as_ptr() as usize)
        / core::mem::size_of::<InstructionRecord>()
}

/// The row base of a count loop, which has no rows. It is far past any account index, so a row
/// account named there resolves to nothing, though the verifier already rejects one; and far
/// below `usize::MAX`, so adding a row offset cannot wrap it back into range.
pub const NO_ROWS: usize = usize::MAX / 2;

/// A loop in progress: a FOREACH over the batch rows or a REPEAT over a count. The kind decides
/// only how the loop starts, as its pass count, first row base and stride, so it is not stored.
struct Loop {
    body_start: usize,
    body_end: usize,
    /// Passes to run: one per batch row for FOREACH, the count for REPEAT.
    passes: usize,
    /// Added to `row_base` after each pass: the batch stride for FOREACH, and zero for REPEAT,
    /// whose base stays `NO_ROWS`.
    stride: usize,
    /// The registers the carry mask names, in ascending order: the first `carried_len` entries.
    /// Listed once at loop entry so a pass copies just these instead of testing every register.
    carried: [u8; MAX_REGISTERS],
    carried_len: usize,
    /// Whether the body names a destination outside the carry mask. When it does not, every
    /// register already equals its snapshot at the end of a pass (the carried ones were just
    /// copied into it), so the restore would copy the file onto itself and is skipped.
    restore: bool,
    /// The pass running now, from zero: the value `LOOP_INDEX` reads.
    pass: usize,
    /// Where the current pass's row starts among the runtime accounts; `NO_ROWS` for REPEAT.
    row_base: usize,
}

/// Starts the loop at `pc`: checks its body range, finds how many passes it makes, snapshots the
/// registers, and decides what the loop's invocations can reuse between passes. With a pass to
/// run, fills `slot` in place and returns the body's range; with none, leaves it empty and
/// returns the code after the loop.
#[inline(never)]
fn enter_loop(
    machine: &mut Machine<'_, '_>,
    pc: usize,
    instruction: &InstructionRecord,
    slot: &mut Option<Loop>,
) -> Result<(usize, usize), ProgramError> {
    let program = machine.program;
    let body_start = pc + 1;
    let body_end = body_start
        .checked_add(instruction.a as usize)
        .filter(|end| *end <= program.instructions.len())
        .ok_or_else(|| RunError::from(BallistaError::InvalidTemplateProgram).at(pc, instruction))?;
    // FOREACH makes one pass per batch row, starting at the first row. REPEAT makes as many
    // passes as its count register holds now, read once so the body cannot change it, and has
    // no rows.
    let (passes, row_base, stride) = if instruction.opcode == OP_FOREACH {
        (
            machine.iterations,
            program.header.fixed_account_count(),
            program.header.batch_stride(),
        )
    } else {
        let count = loop_count(machine.registers, instruction)
            .map_err(|error| error.at(pc, instruction))?;
        (count, NO_ROWS, 0)
    };
    // Registers written inside the body are discarded after each pass, except the ones named in
    // the carry mask, which flow into the next pass and out of the loop.
    let carry = instruction.immediate();
    let register_count = machine.registers.len();
    // Every write goes through an instruction's `dst`, whatever its opcode, so a body whose
    // destinations are all carried registers, or not registers at all, leaves nothing to restore.
    let restore = program.instructions[body_start..body_end].iter().any(|record| {
        let dst = record.dst as usize;
        dst < register_count && (dst >= MAX_REGISTERS || carry & (1u64 << dst) == 0)
    });
    let (cache_cpi, data_invariant) =
        loop_cache_plan(program, &program.instructions[body_start..body_end]);
    machine.scratch.cache_cpi = cache_cpi;
    machine.scratch.data_invariant = data_invariant;
    machine.scratch.built = false;
    if passes == 0 {
        finish_loop(machine);
        return Ok((body_end, program.instructions.len()));
    }
    // `run` allocates exactly the header's one-byte count of registers. Sliced by that count
    // rather than by the file's own length, the copy is visibly small and needs no overflow
    // check, which on SBF is a 128-bit multiply.
    let registers = &machine.registers[..program.header.register_count()];
    let snapshot = &mut machine.scratch.snapshot;
    if snapshot.len() == registers.len() {
        snapshot.copy_from_slice(registers);
    } else {
        *snapshot = registers.to_vec();
    }
    // Built in place: moving a finished loop into the slot would copy it with a syscall.
    let active = slot.insert(Loop {
        body_start,
        body_end,
        passes,
        stride,
        carried: [0; MAX_REGISTERS],
        carried_len: 0,
        restore,
        pass: 0,
        row_base,
    });
    for register in 0..register_count.min(MAX_REGISTERS) {
        if carry & (1u64 << register) != 0 {
            active.carried[active.carried_len] = register as u8;
            active.carried_len += 1;
        }
    }
    // The first pass starts from registers equal to the snapshot, so it needs no restore.
    Ok((body_start, body_end))
}

/// The pass count a REPEAT names: register `b`, which must hold a `u64` no greater than the
/// static maximum in `c`.
#[inline(always)]
fn loop_count(registers: &[RuntimeValue<'_>], instruction: &InstructionRecord) -> RunResult<usize> {
    let count = match registers.get(instruction.b as usize) {
        Some(RuntimeValue::U64(count)) => *count,
        Some(RuntimeValue::Unset) | None => return Err(BallistaError::InvalidRegister.into()),
        Some(_) => return Err(BallistaError::TypeMismatch.into()),
    };
    if count > instruction.c as u64 {
        return Err(BallistaError::LoopCountExceeded.into());
    }
    // At most 255, so it fits.
    Ok(count as usize)
}

/// Ends one pass over a loop body: keeps the carried registers, then either restores the
/// snapshot for the next pass and returns its loop context, or restores it for the code after the
/// loop and returns `None`.
#[inline(never)]
fn next_pass(machine: &mut Machine<'_, '_>, active: &mut Loop) -> Option<(usize, usize)> {
    let snapshot = &mut machine.scratch.snapshot;
    for &register in &active.carried[..active.carried_len] {
        let register = register as usize;
        snapshot[register] = machine.registers[register];
    }
    if active.restore {
        machine.registers.copy_from_slice(snapshot);
    }
    // Neither add wraps: a loop makes at most 255 passes, a FOREACH's row base stays below the
    // runtime account count, and a REPEAT's stays `NO_ROWS`, since its stride is zero.
    active.pass = active.pass.wrapping_add(1);
    if active.pass < active.passes {
        // Row `n` starts at `fixed + n * stride`; stepping by the stride avoids a checked
        // multiplication, which SBF implements with a 128-bit multiply routine of about fifty
        // instructions. A REPEAT's stride is zero.
        active.row_base = active.row_base.wrapping_add(active.stride);
        return Some((active.pass, active.row_base));
    }
    finish_loop(machine);
    None
}

/// Clears the loop's invocation cache once the loop is over.
fn finish_loop(machine: &mut Machine<'_, '_>) {
    machine.scratch.cache_cpi = None;
    machine.scratch.built = false;
    machine.scratch.data_invariant = false;
}

/// Executes one instruction from its parts, for the unit tests and the formal specifications. The
/// dispatch loop calls `step` directly. `iterations` is only read by the loop's own batch
/// handling, never by `step`, so zero stands in for it here.
#[cfg(any(test, feature = "spec-api"))]
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn execute_instruction<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    registers: &mut [RuntimeValue<'data>],
    scratch: &mut Scratch<'data>,
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> RunResult<()> {
    let mut machine = Machine {
        program,
        inputs,
        accounts,
        registers,
        scratch,
        iterations: 0,
        template: None,
    };
    step(&mut machine, instruction, loop_context)
}

/// Runs a whole program, loops included, against a register file the caller owns, for the formal
/// specifications (see `kani/`): `run` without its account checks, input decoding and run event,
/// so a proof can read every register afterwards. The dispatch loop is the one `run` uses.
#[cfg(feature = "spec-api")]
pub fn execute_program<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    iterations: usize,
    registers: &mut [RuntimeValue<'data>],
    scratch: &mut Scratch<'data>,
) -> ProgramResult {
    execute_root(program, inputs, accounts, iterations, registers, scratch, None)
}

/// Executes one instruction; `dispatch` inlines it. It takes the whole `Machine` so that its call
/// to `extended_instruction` can pass one pointer: the machine's fields, the instruction and the
/// loop context came to nine words, too many for registers, and five went on the stack.
#[inline(always)]
fn step<'data>(
    machine: &mut Machine<'_, 'data>,
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> RunResult<()> {
    let program = machine.program;
    let inputs = machine.inputs;
    let accounts = machine.accounts;
    let registers = &mut *machine.registers;
    let scratch = &mut *machine.scratch;
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
                // The row count, the input counts, and so the index are all below 256 × 256, so
                // none of this can wrap, and `get` bounds-checks the result regardless. A checked
                // multiplication would call SBF's 128-bit multiply routine on every row input load.
                program
                    .header
                    .input_count()
                    .wrapping_add(iteration.wrapping_mul(program.header.row_input_count()))
                    .wrapping_add(offset)
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
            let account = resolve(program, accounts, instruction.a, loop_context)?;
            set(
                registers,
                dst,
                RuntimeValue::Pubkey(account.address().to_bytes()),
            )?;
        }
        OP_ACCOUNT_OWNER => {
            let account = resolve(program, accounts, instruction.a, loop_context)?;
            set(
                registers,
                dst,
                RuntimeValue::Pubkey(account.owner().to_bytes()),
            )?;
        }
        OP_ACCOUNT_LAMPORTS => {
            let account = resolve(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::U64(account.lamports()))?;
        }
        OP_ACCOUNT_DATA_LEN => {
            let account = resolve(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::U64(account.data_len() as u64))?;
        }
        OP_ACCOUNT_IS_EMPTY => {
            let account = resolve(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::Bool(account.is_data_empty()))?;
        }
        // `OP_READ_I32` is left out of this arm on purpose and reaches `read_account` through `_`.
        // A case of its own reshapes the comparison tree this match compiles to (see `_` below):
        // it cost "run, sum 30 rows" 60 compute units, a template that never reads an i32.
        OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_U128
        | OP_READ_PUBKEY | OP_READ_BOOL => {
            read_account(program, accounts, registers, instruction, loop_context)?
        }
        OP_CLOCK_SLOT => set(registers, dst, RuntimeValue::U64(Clock::get()?.slot))?,
        OP_CLOCK_TIMESTAMP => set(
            registers,
            dst,
            RuntimeValue::I64(Clock::get()?.unix_timestamp),
        )?,
        OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX => {
            let left = operand(registers, instruction.a)?;
            let right = operand(registers, instruction.b)?;
            let value = arithmetic(instruction.opcode, *left, *right)
                .map_err(|error| unset_first(error, left, right))?;
            set(registers, dst, value)?;
        }
        OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => {
            let left = operand(registers, instruction.a)?;
            let right = operand(registers, instruction.b)?;
            let result = compare(instruction.opcode, *left, *right)
                .map_err(|error| unset_first(error, left, right))?;
            set(registers, dst, RuntimeValue::Bool(result))?;
        }
        OP_AND | OP_OR => {
            let left = read_bool(registers, instruction.a)?;
            let right = read_bool(registers, instruction.b)?;
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
            RuntimeValue::Bool(!read_bool(registers, instruction.a)?),
        )?,
        OP_SELECT => {
            let condition = read_bool(registers, instruction.a)?;
            let selected = if condition {
                *read(registers, instruction.b)?
            } else {
                *read(registers, instruction.c)?
            };
            set(registers, dst, selected)?;
        }
        OP_CAST_U64 | OP_CAST_I64 | OP_CAST_U128 => {
            let source = operand(registers, instruction.a)?;
            let value = cast(instruction.opcode, *source)
                .map_err(|error| unset_first(error, source, source))?;
            set(registers, dst, value)?;
        }
        OP_LOOP_INDEX => {
            let (iteration, _) = loop_context.ok_or(BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, RuntimeValue::U64(iteration as u64))?;
        }
        OP_MOVE => {
            let value = *read(registers, instruction.a)?;
            set(registers, dst, value)?;
        }
        OP_DERIVE_PDA | OP_CREATE_PDA => {
            let derived = derive_pda(program, accounts, registers, instruction, loop_context)?;
            set(registers, dst, RuntimeValue::Pubkey(derived))?;
        }
        OP_REQUIRE => {
            if !read_bool(registers, instruction.a)? {
                return Err(BallistaError::RequirementFailed.into());
            }
        }
        OP_RETURN_DATA => read_return_data(scratch, instruction, registers)?,
        OP_INVOKE => {
            let slot = scratch.expanded;
            scratch.expanded = scratch.expanded.saturating_add(1);
            scratch.last_invoked = None;
            if instruction.b != NO_INDEX && !read_bool(registers, instruction.b)? {
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
        // Every opcode from `OP_MUL_DIV` up, and any the executor does not run. SBF has no
        // indirect jump, so this match compiles to a tree of comparisons, and a case added here
        // reshapes the tree for every other opcode. Keep this arm one unconditional call: LLVM
        // folds a guard or an `if` on the opcode back into the match as a case of its own, and
        // that measured exactly as badly as giving the math opcodes an arm. Add new opcodes
        // inside `extended_instruction`, never as an arm here. The commits "Keep the math opcodes
        // out of the dispatch loop's way" and "Route new opcodes through a thin out-of-line
        // helper" measure the alternatives; `fixtures/cu-ceilings.json` and
        // `fixtures/example-ceilings.json` hold the line.
        _ => extended_instruction(machine, instruction, loop_context)?,
    }
    Ok(())
}

/// Runs every opcode from `OP_MUL_DIV` up out of line, the introspection and byte opcodes among
/// them; any other opcode it sees is one the executor does not run, and fails. As arms of the
/// dispatch loop, the math opcodes cost every run compute units whether it used them or not; behind
/// this call they cost only the templates that use them.
///
/// It returns `RunResult<()>` and writes the destination register itself. Returning the value
/// instead would give the dispatch loop a return slot of its own for this call, and LLVM hoists
/// such a slot's address to the loop's entry, where every run pays for it: that is how
/// `#[inline(never)]` on `math::integer` and `math::pow10` cost 12 more units per run. Each arm
/// also stores its own result: a value merged from the arms into one store after the match is
/// written piecewise, which cost each opcode 4 to 18 units.
#[inline(never)]
fn extended_instruction<'data>(
    machine: &mut Machine<'_, 'data>,
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> RunResult<()> {
    let registers = &mut *machine.registers;
    let dst = instruction.dst as usize;
    match instruction.opcode {
        OP_MUL_DIV | OP_MUL_DIV_CEIL => mul_div_instruction(registers, instruction),
        OP_POW10 => {
            let exponent = operand(registers, instruction.a)?;
            let value =
                math::pow10(*exponent).map_err(|error| unset_first(error, exponent, exponent))?;
            set(registers, dst, value)
        }
        OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR => {
            let left = operand(registers, instruction.a)?;
            let right = operand(registers, instruction.b)?;
            let value = math::integer(instruction.opcode, *left, *right)
                .map_err(|error| unset_first(error, left, right))?;
            set(registers, dst, value)
        }
        OP_READ_I32 => read_account(
            machine.program,
            machine.accounts,
            registers,
            instruction,
            loop_context,
        ),
        // Introspection names a fixed account, so it resolves without the loop context. The count
        // and the index are two bytes at either end of the sysvar and need no parsing.
        OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX => {
            let sysvar = resolve(machine.program, machine.accounts, instruction.a, None)?;
            let value = introspect::count_or_index(instruction.opcode, sysvar)?;
            set(registers, dst, RuntimeValue::U64(value))
        }
        // Parsing the sysvar and reading byte ranges run in their own frames. Each helper takes
        // four words, all passed in registers: one that took the loop context as well would take
        // words from the stack, and their loads would run on entry here, for every opcode.
        //
        // The seven reads are listed, not written as a range. rustc tests a range pattern apart
        // from the match's switch, on the way to the fallback arm, where it cost every loop entry
        // and every output 5 compute units.
        OP_INSTRUCTION_PROGRAM
        | OP_INSTRUCTION_ACCOUNT_COUNT
        | OP_INSTRUCTION_ACCOUNT
        | OP_INSTRUCTION_ACCOUNT_FLAGS
        | OP_INSTRUCTION_DATA_LEN
        | OP_READ_INSTRUCTION_DATA
        | OP_READ_INSTRUCTION_BYTES => {
            let sysvar = resolve(machine.program, machine.accounts, instruction.a, None)?;
            introspect::read_instruction(sysvar, registers, instruction)
        }
        OP_READ_ACCOUNT_BYTES => {
            let account = resolve(machine.program, machine.accounts, instruction.a, loop_context)?;
            introspect::read_account_bytes(account, registers, instruction)
        }
        OP_BYTES_LEN => {
            let length = introspect::bytes_len(operand(registers, instruction.a)?)?;
            set(registers, dst, RuntimeValue::U64(length))
        }
        // A registry opcode names its entry by account, and all its work runs in one cold helper
        // that takes the machine and the instruction, two words in registers.
        OP_OPEN_REGISTRY | OP_READ_REGISTRY | OP_WRITE_REGISTRY => {
            registry_instruction(machine, instruction)
        }
        _ => write_output(machine, instruction),
    }
}

/// `EMIT` and `SET_RETURN_DATA`, reached through `extended_instruction`'s fallback arm so that
/// its match keeps its shape. Every opcode the executor does not run arrives here too, and fails
/// before anything is read.
///
/// The output's parts are encoded into the run's output buffer, exactly as invocation data is,
/// then logged as one `Program data:` field or set as the run's return data. The verifier bounds
/// every output at `MAX_RETURN_DATA_LEN` bytes, and admits one `SET_RETURN_DATA`, outside every
/// loop and after the last invoke, because the runtime clears return data whenever a program is
/// invoked. It also requires every `EMIT` to start with a literal tag outside the run event's
/// family, so no log reads as a run event; nothing here checks that again. A `cu-profile` build
/// replaces the return data with the profile record once the run ends; see `profile::report`.
///
/// Cold, so the register allocator charges this call rather than the math arms beside it. Without
/// that, keeping `instruction` alive for the call cost the math opcodes spills, and the math
/// fixture cost 37 compute units more. A loop still passes through here on entry, as an opcode
/// the executor does not run, which costs each loop 8 compute units.
#[cold]
#[inline(never)]
fn write_output(machine: &mut Machine<'_, '_>, instruction: &InstructionRecord) -> RunResult<()> {
    let emit = match instruction.opcode {
        OP_EMIT => true,
        OP_SET_RETURN_DATA => false,
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    };
    let (start, count) = instruction.blob_range();
    let segments = start
        .checked_add(count)
        .and_then(|end| machine.program.data_segments.get(start..end))
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    let buffer = machine
        .scratch
        .output
        .get_or_insert_with(|| Box::leak(Box::new(Vec::with_capacity(MAX_RETURN_DATA_LEN))));
    buffer.clear();
    let mut sink = OutputSink(buffer);
    for segment in segments {
        encode_segment(machine.program, machine.registers, segment, &mut sink)?;
    }
    let bytes = sink.0.as_slice();
    if bytes.len() > MAX_RETURN_DATA_LEN {
        return Err(BallistaError::InvalidTemplateProgram.into());
    }
    if emit {
        emit_event(bytes);
    } else {
        pinocchio::cpi::set_return_data(bytes);
    }
    Ok(())
}

/// `OPEN_REGISTRY`, `READ_REGISTRY` and `WRITE_REGISTRY`, reached through `extended_instruction`.
/// Cold and out of line, as `write_output` is: measured on `65c9b45`, the arm and this helper
/// cost no fixed case and no cookbook example anything.
///
/// An open checks or creates its entry and marks it open (see `registry::open`). A read or a write
/// requires that mark, which only an open in this run sets, then touches only its field. The
/// verifier guarantees the open before it, the field's range and the value's type; the checks
/// here keep memory safe regardless.
#[cold]
#[inline(never)]
fn registry_instruction(
    machine: &mut Machine<'_, '_>,
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let immediate = instruction.immediate();
    if instruction.opcode == OP_OPEN_REGISTRY {
        let template = machine
            .template
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        let open = RegistryOpen::decode(immediate).ok_or(BallistaError::InvalidTemplateProgram)?;
        let entry = resolve(machine.program, machine.accounts, instruction.a, None)?;
        let payer = resolve(machine.program, machine.accounts, instruction.c, None)?;
        let key = if instruction.b == NO_INDEX {
            [0; 32]
        } else {
            match get(machine.registers, instruction.b)? {
                RuntimeValue::Pubkey(key) => key,
                _ => return Err(BallistaError::TypeMismatch.into()),
            }
        };
        let id = registry::EntryId {
            template: template.address(),
            index: open.index,
            key,
        };
        return registry::open(entry, payer, &id, usize::from(open.size));
    }
    let account = if instruction.opcode == OP_READ_REGISTRY {
        instruction.a
    } else {
        instruction.b
    };
    let entry = resolve(machine.program, machine.accounts, account, None)?;
    if !entry.is_borrowed_mut() {
        return Err(BallistaError::InvalidTemplateProgram.into());
    }
    let offset = REGISTRY_ENTRY_HEADER_LEN + usize::from(immediate as u16);
    let selector = (immediate >> 16) as u8;
    if instruction.opcode == OP_READ_REGISTRY {
        // SAFETY: no reference into the entry's data outlives the opcode that took it.
        let data = unsafe { entry.borrow_unchecked() };
        let value = read_value(selector, data, offset)?;
        return set(machine.registers, instruction.dst as usize, value);
    }
    let value = operand(machine.registers, instruction.a)?;
    registry::write_field(entry, offset, selector, value)
}

/// `MUL_DIV` and `MUL_DIV_CEIL`, out of line from `extended_instruction`. Inside it, their three
/// operands and the call to `math::mul_div` nearly doubled the helper's stack use, 160 bytes to
/// 312, and each multiply-divide cost about 25 units more than it does from here.
#[inline(never)]
fn mul_div_instruction<'data>(
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let a = operand(registers, instruction.a)?;
    let b = operand(registers, instruction.b)?;
    let c = operand(registers, instruction.c)?;
    // `c` fills both slots: `unset_first` only needs to know whether it is `Unset`.
    let value = math::mul_div(instruction.opcode == OP_MUL_DIV_CEIL, *a, *b, *c)
        .map_err(|error| unset_first(unset_first(error, a, b), c, c))?;
    set(registers, instruction.dst as usize, value)
}

/// Reads a typed value from an account's data into the destination register, for the read arm of
/// the dispatch loop and for `READ_I32` in `extended_instruction`. Inlined into both: out of line,
/// it would add a call to every read in every template.
#[inline(always)]
fn read_account<'data>(
    program: &ProgramView<'data>,
    accounts: &'data [AccountView],
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> RunResult<()> {
    let account = resolve(program, accounts, instruction.a, loop_context)?;
    let offset = if instruction.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0 {
        match registers.get(instruction.b as usize) {
            Some(RuntimeValue::U64(value)) => {
                usize::try_from(*value).map_err(|_| BallistaError::InvalidRuntimeAccount)?
            }
            Some(RuntimeValue::Unset) | None => return Err(BallistaError::InvalidRegister.into()),
            Some(_) => return Err(BallistaError::TypeMismatch.into()),
        }
    } else {
        instruction.immediate() as usize
    };
    let data = account.try_borrow()?;
    decode_value(instruction.opcode, &data, offset, |value| {
        set(registers, instruction.dst as usize, value)
    })
}

/// Derives the address a `DERIVE_PDA` or `CREATE_PDA` names. Out of line: its seed buffers are
/// most of a kilobyte, and inlined into the dispatch loop they cost every other instruction a
/// larger frame to set up.
#[inline(never)]
fn derive_pda<'data>(
    program: &ProgramView<'data>,
    accounts: &'data [AccountView],
    registers: &[RuntimeValue<'data>],
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> RunResult<[u8; 32]> {
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
    // One extra slot holds the caller-supplied bump for CREATE_PDA.
    let mut bump = [0u8; 1];
    let mut seeds: [&[u8]; MAX_PDA_SEEDS + 1] = [&[]; MAX_PDA_SEEDS + 1];
    let derived = if instruction.opcode == OP_CREATE_PDA {
        let RuntimeValue::U64(value) = get(registers, instruction.b)? else {
            return Err(BallistaError::TypeMismatch.into());
        };
        bump[0] = u8::try_from(value).map_err(|_| BallistaError::InvalidPdaDerivation)?;
        for slot in 0..count {
            seeds[slot] = &storage[slot][..lengths[slot]];
        }
        seeds[count] = &bump;
        pda::create_program_address(&seeds[..count + 1], program_account.address())
            .ok_or(BallistaError::InvalidPdaDerivation)?
    } else {
        for slot in 0..count {
            seeds[slot] = &storage[slot][..lengths[slot]];
        }
        let (derived, _) =
            pda::try_find_program_address(&seeds[..count], program_account.address())
                .ok_or(BallistaError::InvalidPdaDerivation)?;
        derived
    };
    Ok(derived.to_bytes())
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
    if &program_id != expected_program.as_array() {
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

#[inline(always)]
pub fn read_value<'data>(opcode: u8, data: &[u8], offset: usize) -> RunResult<RuntimeValue<'data>> {
    decode_value(opcode, data, offset, Ok)
}

/// Decodes the value a read opcode names at `offset` and hands it to `sink`. Each opcode calls
/// `sink` with its own variant, so a sink that stores to a register writes only that variant's
/// bytes instead of a whole value merged from all eight.
#[inline(always)]
fn decode_value<'data, T>(
    opcode: u8,
    data: &[u8],
    offset: usize,
    sink: impl FnOnce(RuntimeValue<'data>) -> RunResult<T>,
) -> RunResult<T> {
    match opcode {
        OP_READ_U8 => sink(RuntimeValue::U64(read_array::<1>(data, offset)?[0] as u64)),
        OP_READ_U16 => sink(RuntimeValue::U64(
            u16::from_le_bytes(*read_array(data, offset)?) as u64,
        )),
        OP_READ_U32 => sink(RuntimeValue::U64(
            u32::from_le_bytes(*read_array(data, offset)?) as u64,
        )),
        OP_READ_U64 => sink(RuntimeValue::U64(u64::from_le_bytes(*read_array(data, offset)?))),
        OP_READ_I64 => sink(RuntimeValue::I64(i64::from_le_bytes(*read_array(data, offset)?))),
        OP_READ_I32 => sink(RuntimeValue::I64(
            i32::from_le_bytes(*read_array(data, offset)?) as i64,
        )),
        OP_READ_U128 => sink(RuntimeValue::U128(*read_array(data, offset)?)),
        OP_READ_PUBKEY => sink(RuntimeValue::Pubkey(*read_array(data, offset)?)),
        OP_READ_BOOL => match read_array::<1>(data, offset)?[0] {
            0 => sink(RuntimeValue::Bool(false)),
            1 => sink(RuntimeValue::Bool(true)),
            _ => Err(BallistaError::TypeMismatch.into()),
        },
        _ => Err(BallistaError::InvalidTemplateProgram.into()),
    }
}

// Out of line so the invocation's setup is never inlined into the dispatch loop, where its locals
// and register pressure would weigh on every other instruction.
#[inline(never)]
fn invoke_cpi<'data>(
    program: &ProgramView<'data>,
    accounts: &'data [AccountView],
    registers: &[RuntimeValue<'data>],
    cpi_index: usize,
    loop_context: Option<(usize, usize)>,
    scratch: &mut Scratch<'data>,
) -> RunResult<()> {
    crate::profile::setup_begin();
    // A batch invoking the same descriptor every row rebuilds an account list whose fixed entries
    // are identical each time. Resolving one costs about 93 compute units, so the repeat pass
    // touches only the slots that name a row account, and takes what it needs from the descriptor
    // from the pass that built the list, where the lookups were already checked.
    let cacheable = scratch.cache_cpi == Some(cpi_index);
    // Set together with `built`, once the list is complete, so present exactly when it can be
    // reused.
    let built_call = if cacheable && scratch.built {
        scratch.built_call
    } else {
        None
    };
    let reused = built_call.is_some();
    let (records, segments, max_data_len, program_reference, known_program) =
        if let Some(call) = built_call {
            rebind_row_accounts(program, accounts, call.records, loop_context, scratch)?;
            (
                call.records,
                call.segments,
                call.max_data_len,
                call.program_account,
                call.program,
            )
        } else {
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

            scratch.built = false;
            scratch.metas.clear();
            scratch.views.clear();
            // Both lists hold `MAX_CPI_ACCOUNTS` and `account_len` is at most that, so the records
            // fit; writing the slots directly skips a capacity check and a length update per push.
            let mut count = 0;
            let slots = records
                .iter()
                .zip(scratch.metas.spare_capacity_mut())
                .zip(scratch.views.spare_capacity_mut());
            for ((record, meta), view) in slots {
                let account = resolve_account(program, accounts, record.account, loop_context)?;
                meta.write(InstructionAccount::new(
                    account.address(),
                    record.flags & ACCOUNT_WRITABLE != 0,
                    record.flags & ACCOUNT_SIGNER != 0,
                ));
                view.write(account);
                count += 1;
            }
            if count < records.len() {
                return Err(BallistaError::InvalidTemplateProgram.into());
            }
            // SAFETY: the loop initialized the first `count` slots of both lists, which were empty.
            unsafe {
                scratch.metas.set_len(count);
                scratch.views.set_len(count);
            }
            if let Some(group) = descriptor.account_group() {
                // Group accounts are forwarded with the transaction's writable flag and never as
                // signers: a template delegates signatures only through declared slots. Their range
                // comes from the run layout, so it is the same on every iteration.
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
            (
                records,
                segments,
                descriptor.max_data_len(),
                descriptor.program_account,
                None,
            )
        };

    if !(reused && scratch.data_invariant) {
        scratch.data.clear();
        for segment in segments {
            encode_segment(program, registers, segment, &mut scratch.data)?;
        }
        if scratch.data.len() > max_data_len || scratch.data.len() > MAX_CPI_DATA_LEN {
            return Err(BallistaError::CpiDataTooLarge.into());
        }
    }

    // The program account is resolved on every invocation except a batch's repeat pass, which
    // remembers it when it is not a row account.
    let program_account = match known_program {
        Some(account) => account,
        None => resolve_account(program, accounts, program_reference, loop_context)?,
    };
    if cacheable && !reused {
        scratch.built = true;
        scratch.built_call = Some(BuiltCall {
            records,
            segments,
            max_data_len,
            program_account: program_reference,
            program: (program_reference & ITERATION_ACCOUNT_BIT == 0).then_some(program_account),
        });
    }
    let instruction = InstructionView {
        program_id: program_account.address(),
        accounts: scratch.metas.as_slice(),
        data: scratch.data.as_slice(),
    };
    crate::profile::setup_end();
    crate::profile::cpi_begin();
    let invoked = bounded_invoke(&instruction, scratch.views.as_slice());
    crate::profile::cpi_end();
    invoked.map_err(|error| reentry_or(error, scratch.views.as_slice()))?;
    scratch.last_invoked = Some(program_account.address());
    Ok(())
}

/// What a CPI that `bounded_invoke` refused reports. It refuses a writable account whose data is
/// borrowed, and during a run only a registry open marks an account's data exclusively borrowed,
/// so a refusal of a CPI that passes such an account is one that could let a nested run write an
/// open entry: `RegistryReentry`. Anything else passes through. (A CPI that passes an open entry
/// read-only and some other borrowed account writable, which only the template's own account can
/// be, reports `RegistryReentry` too; both fail the run.)
///
/// This is the whole reentry check. It runs only when an invocation fails, so a CPI that succeeds
/// pays nothing for it; a check of the invoked program before every CPI measured 3 compute units a
/// CPI, 89 on `run, payroll 30 rows`. It takes the views alone: given the whole `Scratch`, to
/// look at the metas' writable flags as well, the invocation path cost 14 units more per CPI.
#[cold]
#[inline(never)]
fn reentry_or(error: ProgramError, views: &[&AccountView]) -> RunError {
    if error == ProgramError::AccountBorrowFailed && views.iter().any(|view| view.is_borrowed_mut()) {
        return BallistaError::RegistryReentry.into();
    }
    error.into()
}

/// The descriptor a loop body invokes, when every invoke in it names the same one. A body with
/// two invocations would rebuild the shared account list on each, so caching would cost without
/// ever paying back.
fn sole_invoked_cpi(body: &[InstructionRecord]) -> Option<usize> {
    let mut only = None;
    for record in body {
        if record.opcode != OP_INVOKE {
            continue;
        }
        match only {
            None => only = Some(record.a as usize),
            Some(index) if index == record.a as usize => {}
            Some(_) => return None,
        }
    }
    only
}

/// What a loop can reuse between passes: the descriptor every invoke in the body names, if they
/// all name the same one, and whether its data bytes are the same on every pass. Decided once, at
/// loop entry, and out of line so the decision does not bloat the interpreter's own loop.
#[inline(never)]
fn loop_cache_plan(program: &ProgramView<'_>, body: &[InstructionRecord]) -> (Option<usize>, bool) {
    let Some(index) = sole_invoked_cpi(body) else {
        return (None, false);
    };
    let invariant = program
        .cpis
        .get(index)
        .is_some_and(|descriptor| {
            cpi_data_is_loop_invariant(program, descriptor, registers_written(body))
        });
    (Some(index), invariant)
}

/// The registers a loop body can write. Every other register is restored from the pre-loop
/// snapshot after each pass, so its value is the same on every pass.
fn registers_written(body: &[InstructionRecord]) -> u64 {
    let mut written = 0u64;
    for record in body {
        if record.dst != NO_INDEX && (record.dst as usize) < MAX_REGISTERS {
            written |= 1u64 << record.dst;
        }
    }
    written
}

/// Whether a descriptor's data bytes are identical on every pass: each segment is either a
/// literal, or reads a register the body never writes.
fn cpi_data_is_loop_invariant(
    program: &ProgramView<'_>,
    descriptor: &CpiDescriptor,
    written: u64,
) -> bool {
    let Some(end) = descriptor
        .segment_start()
        .checked_add(descriptor.segment_len as usize)
    else {
        return false;
    };
    let Some(segments) = program.data_segments.get(descriptor.segment_start()..end) else {
        return false;
    };
    segments.iter().all(|segment| {
        segment.kind == DATA_LITERAL
            || ((segment.register as usize) < MAX_REGISTERS
                && written & (1u64 << segment.register) == 0)
    })
}

/// The repeat pass over a cached account list: only the slots naming a row account can differ
/// from the previous iteration.
#[inline(always)]
fn rebind_row_accounts<'data>(
    program: &ProgramView<'data>,
    accounts: &'data [AccountView],
    records: &[CpiAccountRecord],
    loop_context: Option<(usize, usize)>,
    scratch: &mut Scratch<'data>,
) -> RunResult<()> {
    // The list was built from these same records, one entry each, so the three walk in step. A
    // slot's flags come from its record and do not change between rows; only the account does.
    let slots = records
        .iter()
        .zip(scratch.metas.iter_mut())
        .zip(scratch.views.iter_mut());
    for ((record, meta), view) in slots {
        if record.account & ITERATION_ACCOUNT_BIT == 0 {
            continue;
        }
        let account = resolve_account(program, accounts, record.account, loop_context)?;
        meta.address = account.address();
        *view = account;
    }
    Ok(())
}

/// Performs the CPI in its own stack frame. The call needs a `MAX_CPI_ACCOUNTS`-slot account
/// array on the stack; combined with the caller's locals that exceeded the fixed 4 KiB frame of
/// SBPF version 0, so the array gets a frame to itself.
///
/// This is `invoke_with_bounds` without its first check on each account: that the view's address
/// matches the meta's. Ballista writes each meta from the very view beside it, so the pair always
/// matches. A writable account whose data is borrowed is still refused, with the same error.
#[inline(never)]
fn bounded_invoke(instruction: &InstructionView, views: &[&AccountView]) -> ProgramResult {
    let count = instruction.accounts.len();
    if count > MAX_CPI_ACCOUNTS {
        return Err(ProgramError::InvalidArgument);
    }
    if views.len() < count {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let mut infos = [const { MaybeUninit::<CpiAccount>::uninit() }; MAX_CPI_ACCOUNTS];
    for ((info, meta), view) in infos.iter_mut().zip(instruction.accounts).zip(views) {
        if meta.is_writable {
            view.check_borrow_mut()?;
        }
        CpiAccount::init_from_account_view(view, info);
    }
    // SAFETY: the loop initialized the first `count` slots, since `count` is at most the length
    // of all three sequences, and no writable account's data is borrowed.
    unsafe {
        invoke_signed_unchecked(
            instruction,
            core::slice::from_raw_parts(infos.as_ptr().cast::<CpiAccount>(), count),
            &[],
        );
    }
    Ok(())
}

/// Destination for encoded segment bytes: a `Vec` for CPI data, the output buffer for `EMIT` and
/// `SET_RETURN_DATA`, or a fixed stack buffer for PDA seeds. Spec builds keep `push_bytes` out of
/// line so the prover can summarize the copy inside.
pub trait ByteSink {
    fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()>;
}

impl ByteSink for Vec<u8> {
    #[cfg_attr(feature = "spec-api", inline(never))]
    #[cfg_attr(not(feature = "spec-api"), inline(always))]
    fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

/// The output buffer as a byte sink. A type of its own rather than the `Vec<u8>` sink, so the
/// output opcodes get their own copy of the segment encoder: sharing the invocation's copy made
/// the compiler stop inlining it into `invoke_cpi`, which cost every invocation about 50 compute
/// units.
struct OutputSink<'buffer>(&'buffer mut Vec<u8>);

impl ByteSink for OutputSink<'_> {
    #[cfg_attr(feature = "spec-api", inline(never))]
    #[cfg_attr(not(feature = "spec-api"), inline(always))]
    fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()> {
        self.0.extend_from_slice(bytes);
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
    resolve(program, accounts, reference, loop_context)
}

/// `resolve_account`, always inlined, for the dispatch loop's account instructions. The
/// invocation builder keeps calling `resolve_account` and its own inlining.
#[inline(always)]
fn resolve<'data>(
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
        // In a FOREACH a row starts below the runtime account count, which is at most 120; in a
        // REPEAT the base is `NO_ROWS`, far below `usize::MAX`. The offset is below the stride, so
        // neither add wraps, and the lookup below bounds-checks the index regardless.
        row_base.wrapping_add(offset)
    };
    accounts
        .get(index)
        .ok_or_else(|| BallistaError::InvalidRuntimeAccount.into())
}

#[inline(always)]
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

#[inline(always)]
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

#[inline(always)]
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

#[inline(always)]
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

/// The initialized register at `index`, borrowed where it lives. Handlers match on the reference,
/// so they load only the tag and the payload they use instead of copying a whole value out.
#[inline(always)]
fn read<'registers, 'data>(
    registers: &'registers [RuntimeValue<'data>],
    index: u8,
) -> RunResult<&'registers RuntimeValue<'data>> {
    match registers.get(index as usize) {
        Some(RuntimeValue::Unset) | None => Err(BallistaError::InvalidRegister.into()),
        Some(value) => Ok(value),
    }
}

/// The boolean in register `index`: `InvalidRegister` if it is unset or out of range, then
/// `TypeMismatch` if it holds another type, the same order `as_bool(get(..)?)` reports them in.
#[inline(always)]
fn read_bool(registers: &[RuntimeValue<'_>], index: u8) -> RunResult<bool> {
    match registers.get(index as usize) {
        Some(RuntimeValue::Bool(value)) => Ok(*value),
        other => Err(not_a_bool(other)),
    }
}

/// Why a register that `read_bool` expected to hold a boolean does not.
#[cold]
#[inline(never)]
fn not_a_bool(register: Option<&RuntimeValue<'_>>) -> RunError {
    match register {
        Some(RuntimeValue::Unset) | None => BallistaError::InvalidRegister.into(),
        Some(_) => BallistaError::TypeMismatch.into(),
    }
}

/// The register at `index`, unset or not; out of range is `InvalidRegister`. For handlers whose
/// type match already rejects an unset value: they test the type they expect first and sort out
/// an unset operand only once that test fails, through `unset_first`.
#[inline(always)]
fn operand<'registers, 'data>(
    registers: &'registers [RuntimeValue<'data>],
    index: u8,
) -> RunResult<&'registers RuntimeValue<'data>> {
    registers
        .get(index as usize)
        .ok_or_else(|| BallistaError::InvalidRegister.into())
}

/// The error an operation on `left` and `right` reports once it has failed. Reading an unset
/// operand fails before the operation runs, with `InvalidRegister`, exactly as `get` would have.
#[cold]
#[inline(never)]
fn unset_first(error: RunError, left: &RuntimeValue<'_>, right: &RuntimeValue<'_>) -> RunError {
    if matches!(left, RuntimeValue::Unset) || matches!(right, RuntimeValue::Unset) {
        BallistaError::InvalidRegister.into()
    } else {
        error
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

#[inline(always)]
pub fn read_array<const N: usize>(data: &[u8], offset: usize) -> RunResult<&[u8; N]> {
    offset
        .checked_add(N)
        .and_then(|end| data.get(offset..end))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| BallistaError::InvalidRuntimeAccount.into())
}

#[cfg(test)]
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
        assert!(
            EVENT_MAGIC.starts_with(&RUN_EVENT_TAG_FAMILY),
            "the verifier reserves this family, so no EMIT can log a line that reads as the event"
        );
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

    /// The typed handlers test the type they expect before looking for an unset operand, so this
    /// pins what they report: an unset or out-of-range operand is `InvalidRegister`, as reading it
    /// with `get` first always was, and only a set operand of the wrong type is `TypeMismatch`.
    #[test]
    fn unset_operands_report_invalid_register_before_type_mismatch() {
        let mut builder = ProgramBuilder::new();
        builder.account(0, None, None, 0);
        for _ in 0..4 {
            builder.register();
        }
        builder.const_bool(true);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut scratch = Scratch::new(&program);
        let accounts = [];
        // r0 unset, r1 u64, r2 bool, r3 i64; register 4 is out of range.
        let mut registers = vec![Unset, U64(7), Bool(true), I64(-1)];
        let mut run = |record: InstructionRecord| {
            execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &record, None)
        };
        let invalid = Err(err(BallistaError::InvalidRegister));
        let mismatch = Err(err(BallistaError::TypeMismatch));
        for opcode in [
            OP_ADD, OP_EQ, OP_LT, OP_REM, OP_SHL, OP_SHR, OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR,
        ] {
            assert_eq!(run(record(opcode, 1, 0, 1, NO_INDEX, 0, 0)), invalid, "unset left, {opcode}");
            assert_eq!(run(record(opcode, 1, 1, 0, NO_INDEX, 0, 0)), invalid, "unset right, {opcode}");
            assert_eq!(run(record(opcode, 1, 0, 2, NO_INDEX, 0, 0)), invalid, "unset beside a bool, {opcode}");
            assert_eq!(run(record(opcode, 1, 3, 0, NO_INDEX, 0, 0)), invalid, "a mismatched left does not win, {opcode}");
            assert_eq!(run(record(opcode, 1, 4, 1, NO_INDEX, 0, 0)), invalid, "out of range, {opcode}");
            assert_eq!(run(record(opcode, 1, 1, 3, NO_INDEX, 0, 0)), mismatch, "u64 against i64, {opcode}");
        }
        assert_eq!(run(record(OP_POW10, 1, 0, NO_INDEX, NO_INDEX, 0, 0)), invalid);
        assert_eq!(run(record(OP_POW10, 1, 2, NO_INDEX, NO_INDEX, 0, 0)), mismatch);
        for opcode in [OP_MUL_DIV, OP_MUL_DIV_CEIL] {
            assert_eq!(run(record(opcode, 1, 0, 1, 1, 0, 0)), invalid, "unset a, {opcode}");
            assert_eq!(run(record(opcode, 1, 1, 0, 1, 0, 0)), invalid, "unset b, {opcode}");
            assert_eq!(run(record(opcode, 1, 1, 1, 0, 0, 0)), invalid, "unset c, {opcode}");
            assert_eq!(run(record(opcode, 1, 1, 1, 3, 0, 0)), mismatch, "u64 against i64 in c, {opcode}");
        }
        assert_eq!(run(record(OP_CAST_U128, 1, 0, NO_INDEX, NO_INDEX, 0, 0)), invalid);
        assert_eq!(run(record(OP_CAST_U128, 1, 2, NO_INDEX, NO_INDEX, 0, 0)), mismatch);
        for opcode in [OP_AND, OP_OR] {
            assert_eq!(run(record(opcode, 1, 0, 2, NO_INDEX, 0, 0)), invalid);
            assert_eq!(run(record(opcode, 1, 2, 0, NO_INDEX, 0, 0)), invalid);
            assert_eq!(run(record(opcode, 1, 1, 0, NO_INDEX, 0, 0)), mismatch, "the left operand is read first");
        }
        assert_eq!(run(record(OP_NOT, 1, 0, NO_INDEX, NO_INDEX, 0, 0)), invalid);
        assert_eq!(run(record(OP_NOT, 1, 1, NO_INDEX, NO_INDEX, 0, 0)), mismatch);
        assert_eq!(run(record(OP_REQUIRE, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, 0)), invalid);
        assert_eq!(run(record(OP_REQUIRE, NO_INDEX, 3, NO_INDEX, NO_INDEX, 0, 0)), mismatch);
        assert_eq!(run(record(OP_SELECT, 1, 0, 1, 1, 0, 0)), invalid);
        assert_eq!(run(record(OP_SELECT, 1, 2, 0, 1, 0, 0)), invalid, "the selected operand is unset");
        assert_eq!(run(record(OP_MOVE, 1, 0, NO_INDEX, NO_INDEX, 0, 0)), invalid);
        assert_eq!(registers[1], U64(7), "no failed instruction wrote its destination");
    }

    /// An opcode the executor does not run reaches `extended_instruction` through the fallback
    /// arm, as every opcode from `OP_MUL_DIV` up does. It must still fail as an invalid program
    /// before anything else is checked: an operand or destination out of range must not turn the
    /// failure into `InvalidRegister`, and nothing may be written.
    #[test]
    fn opcodes_the_executor_does_not_run_fail_before_reading_operands() {
        let mut builder = ProgramBuilder::new();
        builder.register();
        builder.const_bool(true);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut scratch = Scratch::new(&program);
        let mut registers = vec![U64(7)];
        // 39 is unassigned, and so is every number after `OP_WRITE_REGISTRY`, 77, the last opcode
        // the registry takes. FOREACH and REPEAT fail here too: at the root, the dispatch loop
        // starts a loop from that failure, and inside a body the failure stands.
        for opcode in [0, 39, OP_FOREACH, OP_REPEAT, OP_WRITE_REGISTRY + 1, 0xfe, u8::MAX] {
            for dst in [0, 9] {
                assert_eq!(
                    execute_instruction(
                        &program,
                        &[],
                        &[],
                        &mut registers,
                        &mut scratch,
                        &record(opcode, dst, 9, 9, 9, 0, 0),
                        None,
                    ),
                    Err(err(BallistaError::InvalidTemplateProgram)),
                    "opcode {opcode}, destination {dst}"
                );
            }
        }
        // An open run on its own has no template to key its entry by.
        assert_eq!(
            execute_instruction(
                &program,
                &[],
                &[],
                &mut registers,
                &mut scratch,
                &record(OP_OPEN_REGISTRY, NO_INDEX, 0, NO_INDEX, 0, 0, 1 << 8),
                None,
            ),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
        assert_eq!(registers[0], U64(7));
        assert!(
            scratch.output.is_none(),
            "the reject path, which every loop entry takes, allocates no output buffer"
        );
    }

    /// `EMIT` and `SET_RETURN_DATA` encode into their own buffer. `data` may hold an invocation
    /// payload that a batch sends again without encoding it, so an output must never write there.
    #[test]
    fn outputs_encode_into_their_own_buffer() {
        let mut builder = ProgramBuilder::new();
        let amount = builder.const_u64(0x0102);
        let key = builder.const_pubkey([7; 32]);
        let tag = builder.blob(b"OUT1");
        let logged = builder.emit_data(&[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_U16, amount),
            Segment::Register(DATA_REG_PUBKEY, key),
        ]);
        let returned = builder.set_return_data(&[Segment::Register(DATA_REG_U64, amount)]);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut scratch = Scratch::new(&program);
        assert!(
            scratch.output.is_none(),
            "no buffer before the run's first output"
        );
        scratch.data.extend_from_slice(b"cached payload");
        let mut registers = vec![U64(0x0102), Pubkey([7; 32])];

        execute_instruction(
            &program,
            &[],
            &[],
            &mut registers,
            &mut scratch,
            &program.instructions[logged],
            None,
        )
        .unwrap();
        let mut expected = b"OUT1".to_vec();
        expected.extend_from_slice(&0x0102u16.to_le_bytes());
        expected.extend_from_slice(&[7; 32]);
        assert_eq!(scratch.output.as_deref(), Some(&expected));
        assert_eq!(scratch.data, b"cached payload", "the invocation buffer is untouched");
        // The first output allocates the most any output encodes; later ones reuse it.
        let buffer = scratch.output.as_ref().unwrap();
        assert_eq!(buffer.capacity(), MAX_RETURN_DATA_LEN);
        let allocated = buffer.as_ptr();

        execute_instruction(
            &program,
            &[],
            &[],
            &mut registers,
            &mut scratch,
            &program.instructions[returned],
            None,
        )
        .unwrap();
        assert_eq!(scratch.output.as_deref(), Some(&0x0102u64.to_le_bytes().to_vec()));
        assert_eq!(scratch.output.as_ref().unwrap().as_ptr(), allocated, "no second allocation");
        assert_eq!(scratch.data, b"cached payload");
        assert_eq!(registers, vec![U64(0x0102), Pubkey([7; 32])], "outputs write no register");
    }

    /// Outputs fail like invocation data does on a bad register, and as an invalid program on a
    /// shape the verifier rules out.
    #[test]
    fn outputs_reject_bad_operands_and_unverified_shapes() {
        let mut builder = ProgramBuilder::new();
        for _ in 0..3 {
            builder.register();
        }
        // r0 is unset, r1 holds a u64 too wide for a u8, r2 holds an i64.
        let unset = builder.emit_data(&[Segment::Register(DATA_REG_U64, 0)]);
        let narrow = builder.emit_data(&[Segment::Register(DATA_REG_U8, 1)]);
        let mistyped = builder.set_return_data(&[Segment::Register(DATA_REG_U64, 2)]);
        let long = builder.blob(&[0; MAX_RETURN_DATA_LEN + 1]);
        let too_long = builder.emit_data(&[Segment::Literal(long)]);
        let past_the_table = builder.emit(record(
            OP_EMIT,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            0,
            range_immediate(4, 1),
        ));
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut scratch = Scratch::new(&program);
        let mut registers = vec![Unset, U64(256), I64(-1)];
        let mut run = |index: usize| {
            execute_instruction(
                &program,
                &[],
                &[],
                &mut registers,
                &mut scratch,
                &program.instructions[index],
                None,
            )
        };
        assert_eq!(run(unset), Err(err(BallistaError::InvalidRegister)));
        assert_eq!(run(narrow), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(run(mistyped), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(run(too_long), Err(err(BallistaError::InvalidTemplateProgram)));
        assert_eq!(run(past_the_table), Err(err(BallistaError::InvalidTemplateProgram)));
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

    /// A REPEAT runs its body as many times as its count register held when the loop started, and
    /// a carried register leaves the loop with the last pass's value.
    #[test]
    fn count_loops_run_their_body_count_times_and_carry_values_out() {
        let mut builder = ProgramBuilder::new();
        let rounds_input = builder.input(VALUE_U64, 0);
        let total = builder.const_u64(0);
        let rounds = builder.load_input(rounds_input);
        let repeat = builder.repeat(rounds, 4, 1 << total, |body| {
            let index = body.loop_index();
            let sum = body.binary(OP_ADD, total, index);
            body.mov(total, sum);
        });
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let run = |rounds: RuntimeValue<'static>| {
            let inputs = [rounds];
            let mut registers = vec![Unset; program.header.register_count()];
            let mut scratch = Scratch::new(&program);
            let result = execute_root(&program, &inputs, &[], 0, &mut registers, &mut scratch, None);
            (result, registers[total as usize])
        };
        // 0 + 1 + … + (rounds − 1), and no pass at all for zero rounds.
        for (rounds, expected) in [(0u64, 0u64), (1, 0), (3, 3), (4, 6)] {
            assert_eq!(run(U64(rounds)), (Ok(()), U64(expected)), "{rounds} rounds");
        }
        let at_repeat = |kind: BallistaError| {
            Err(ProgramError::Custom(encode_error(kind.code(), repeat as u16)))
        };
        // A count above the maximum fails at the REPEAT, before its body runs.
        assert_eq!(run(U64(5)).0, at_repeat(BallistaError::LoopCountExceeded));
        assert_eq!(run(U64(u64::MAX)).0, at_repeat(BallistaError::LoopCountExceeded));
        // The count is an operand like any other: it must be set and be a u64.
        assert_eq!(run(I64(1)).0, at_repeat(BallistaError::TypeMismatch));
        assert_eq!(run(Unset).0, at_repeat(BallistaError::InvalidRegister));
    }

    /// A REPEAT reads its count once, when the loop starts. A body that carries the count register
    /// and rewrites it, up or down, still makes as many passes as the count held then, and the
    /// rewritten value leaves the loop.
    #[test]
    fn rewriting_the_count_in_the_body_leaves_the_passes_unchanged() {
        for (raise, count_after) in [(true, 6), (false, 0)] {
            let mut builder = ProgramBuilder::new();
            let count = builder.const_u64(3);
            let passes = builder.const_u64(0);
            builder.repeat(count, 8, (1 << count) | (1 << passes), |body| {
                let one = body.const_u64(1);
                let next = body.binary(OP_ADD, passes, one);
                body.mov(passes, next);
                // One more on every pass, or zero.
                let rewritten = if raise {
                    body.binary(OP_ADD, count, one)
                } else {
                    body.const_u64(0)
                };
                body.mov(count, rewritten);
            });
            let bytes = builder.build().unwrap();
            let program = ProgramView::parse(&bytes).unwrap();
            assert!(program.verify().is_ok());
            let mut registers = vec![Unset; program.header.register_count()];
            let mut scratch = Scratch::new(&program);
            assert_eq!(
                execute_root(&program, &[], &[], 0, &mut registers, &mut scratch, None),
                Ok(())
            );
            assert_eq!(registers[passes as usize], U64(3), "raise {raise}");
            assert_eq!(registers[count as usize], U64(count_after), "raise {raise}");
        }
    }

    /// Loops run one after another, and each FOREACH starts again at the first row.
    #[test]
    fn loops_run_in_sequence_and_every_foreach_starts_at_the_first_row() {
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let amount = builder.row_input(VALUE_U64, 0);
        let first = builder.const_u64(0);
        let passes = builder.const_u64(0);
        let second = builder.const_u64(0);
        let three = builder.const_u64(3);
        builder.for_each(1 << first, |body| {
            let value = body.load_input(amount);
            let sum = body.binary(OP_ADD, first, value);
            body.mov(first, sum);
        });
        builder.repeat(three, 3, 1 << passes, |body| {
            let one = body.const_u64(1);
            let sum = body.binary(OP_ADD, passes, one);
            body.mov(passes, sum);
        });
        builder.for_each(1 << second, |body| {
            let value = body.load_input(amount);
            let index = body.loop_index();
            let weighted = body.binary(OP_MUL, value, index);
            let sum = body.binary(OP_ADD, second, weighted);
            body.mov(second, sum);
        });
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let inputs = [U64(10), U64(20), U64(30)];
        let mut registers = vec![Unset; program.header.register_count()];
        let mut scratch = Scratch::new(&program);
        execute_root(&program, &inputs, &[], 3, &mut registers, &mut scratch, None).unwrap();
        assert_eq!(registers[first as usize], U64(60));
        assert_eq!(registers[passes as usize], U64(3));
        // 10 × 0 + 20 × 1 + 30 × 2: the second FOREACH read every row again, with its index.
        assert_eq!(registers[second as usize], U64(80));
    }

    /// The bump heap never frees, so every loop of a run snapshots its registers into the buffer
    /// the first loop allocated.
    #[test]
    fn every_loop_of_a_run_snapshots_into_one_buffer() {
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(2);
        let first = builder.repeat(count, 2, 0, |body| {
            body.loop_index();
        });
        let second = builder.repeat(count, 2, 0, |body| {
            body.loop_index();
        });
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut registers = vec![Unset; program.header.register_count()];
        registers[count as usize] = U64(2);
        let mut scratch = Scratch::new(&program);
        let mut machine = Machine {
            program: &program,
            inputs: &[],
            accounts: &[],
            registers: &mut registers,
            scratch: &mut scratch,
            iterations: 0,
            template: None,
        };
        let mut slot = None;
        let body = enter_loop(&mut machine, first, &program.instructions[first], &mut slot);
        assert_eq!(body, Ok((first + 1, first + 2)));
        // A count loop has an index and no rows.
        assert_eq!(slot.as_ref().map(|active| (active.pass, active.row_base)), Some((0, NO_ROWS)));
        let buffer = machine.scratch.snapshot.as_ptr();
        slot = None;
        enter_loop(&mut machine, second, &program.instructions[second], &mut slot).unwrap();
        assert_eq!(machine.scratch.snapshot.as_ptr(), buffer, "the second loop reuses the first one's buffer");
    }

    /// Loops never nest: a loop instruction inside a body is an opcode the executor does not run.
    #[test]
    fn a_loop_inside_a_loop_body_fails_as_an_invalid_program() {
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let mut inner = 0;
        builder.repeat(count, 1, 0, |body| {
            inner = body.repeat(count, 1, 0, |inner_body| {
                inner_body.loop_index();
            });
        });
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut registers = vec![Unset; program.header.register_count()];
        let mut scratch = Scratch::new(&program);
        assert_eq!(
            execute_root(&program, &[], &[], 0, &mut registers, &mut scratch, None),
            Err(ProgramError::Custom(encode_error(
                BallistaError::InvalidTemplateProgram.code(),
                inner as u16
            )))
        );
    }

    #[test]
    fn i32_reads_sign_extend() {
        let data = [0xf8, 0xff, 0xff, 0xff, 0x2a, 0x00, 0x00, 0x00];
        assert_eq!(read_value(OP_READ_I32, &data, 0), Ok(RuntimeValue::I64(-8)));
        assert_eq!(read_value(OP_READ_I32, &data, 4), Ok(RuntimeValue::I64(42)));
        assert!(read_value(OP_READ_I32, &data, 5).is_err());
    }
}
