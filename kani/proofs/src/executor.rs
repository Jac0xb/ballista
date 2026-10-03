//! Executor bookkeeping (`programs/ballista/src/processor/execute.rs`), through the `spec-api`
//! feature: what one instruction may write, the loop's carry and restore, row indexing, and the
//! account checks that run before the first instruction.
//!
//! Programs here are `ProgramView`s built directly from symbolic records, not parsed, so the
//! solver spends its effort on the executor rather than the parser (which `wire.rs` covers).
//!
//! Off-chain stubs these harnesses cross, and what they cannot show:
//! - `RETURN_DATA` reads `fetch_return_data`, which returns no data off-chain, so it is only ever
//!   seen failing (`MissingReturnData`); its success path is unproved.
//! - `CLOCK_SLOT` and `CLOCK_TIMESTAMP` read the Clock sysvar, which does not exist off-chain.
//! - `EMIT` and `SET_RETURN_DATA` encode their bytes for real, but the log and the return-data
//!   syscall are no-ops.
//! - `INVOKE` with no descriptors fails before the syscall; `invoke.rs` covers real calls.

use ballista::error::{vm_error, BallistaError};
use ballista::processor::execute::{
    execute_instruction, execute_program, resolve_account, validate_runtime_accounts, RunError,
    RuntimeValue, Scratch, NO_ROWS,
};
use ballista_common::template::*;
use pinocchio::error::ProgramError;
use pinocchio::AccountView;

use crate::accounts::{AccountMemory, Fields};
use crate::util::{any, any_runtime_value, eq32, one_of, same_value};

/// How `dispatch` reports a failure at `pc` (`RunError::at`, which is private).
pub(crate) fn failure_at(error: RunError, pc: usize) -> ProgramError {
    match error {
        RunError::Program(error) => error,
        RunError::Vm(kind) => vm_error(kind, u16::try_from(pc).unwrap_or(u16::MAX)),
        RunError::VmAt(kind, context) => vm_error(kind, context),
    }
}

/// Registers in the per-instruction proofs.
const REGISTERS: usize = 4;

/// Any loop context: none, or any pass and row base.
fn any_loop_context() -> Option<(usize, usize)> {
    if kani::any() { Some((kani::any(), kani::any())) } else { None }
}

/// Runs `opcode` once, every other field of the record symbolic, against 4 registers holding any
/// values (`bytes` up to 4 bytes), with or without a loop context, in a program with one fixed and
/// one row input (3 input values), an 8-byte blob, one pubkey, two symbolic data segments and no
/// runtime accounts. Checks that it never panics or indexes out of bounds, that every register
/// but `dst` is unchanged, and that a failure changes no register. Returns whether it succeeded.
fn frame_without_accounts(opcode: u8) -> bool {
    frame_without_accounts_using(opcode, any_runtime_value)
}

/// `frame_without_accounts` with register and input values drawn by `value`.
fn frame_without_accounts_using(opcode: u8, value: fn(&[u8]) -> RuntimeValue<'_>) -> bool {
    let header = ProgramHeader::new(0, 0, 0, 0, 1, REGISTERS as u8, 1, 0, 0, 2, 1, 0, 8, 1, 0);
    let segments = [any::segment(), any::segment()];
    let pubkeys = [PubkeyRecord { bytes: kani::any() }];
    let blob: [u8; 8] = kani::any();
    let inputs_table = [any::input(), any::input()];
    let mut instruction = any::instruction();
    instruction.opcode = opcode;
    let program = ProgramView {
        header: &header,
        accounts: &[],
        inputs: &inputs_table,
        instructions: core::slice::from_ref(&instruction),
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &segments,
        pubkeys: &pubkeys,
        blob: &blob,
    };
    let bytes: [u8; 4] = kani::any();
    let inputs = [value(&bytes), value(&bytes), value(&bytes)];
    let mut registers: [RuntimeValue<'_>; REGISTERS] = core::array::from_fn(|_| value(&bytes));
    let before = registers;
    let mut scratch = Scratch::new(&program);
    let result = execute_instruction(&program, &inputs, &[], &mut registers, &mut scratch, &instruction, any_loop_context());
    for register in 0..REGISTERS {
        if result.is_err() || register != instruction.dst as usize {
            assert!(same_value(&registers[register], &before[register]));
        }
    }
    result.is_ok()
}

/// One frame check per opcode, each with a cover showing the opcode can succeed.
macro_rules! frames {
    ($check:ident: $($opcode:expr => $succeeds:literal),* $(,)?) => {{
        $(
            let ok = $check($opcode);
            kani::cover!(ok, $succeeds);
        )*
    }};
}

/// Every instruction writes at most its destination: for each loading, constant, move and loop
/// index opcode, `frame_without_accounts` (no other register changes; a failure changes none).
/// This is the fact the loop's restore skip rests on. Bound: as `frame_without_accounts`.
#[kani::proof]
#[kani::unwind(5)]
fn frame_loads_and_constants() {
    frames!(frame_without_accounts:
        OP_LOAD_INPUT => "LOAD_INPUT succeeds",
        OP_CONST_BOOL => "CONST_BOOL succeeds",
        OP_CONST_U64 => "CONST_U64 succeeds",
        OP_CONST_I64 => "CONST_I64 succeeds",
        OP_CONST_U128 => "CONST_U128 succeeds",
        OP_CONST_PUBKEY => "CONST_PUBKEY succeeds",
        OP_CONST_BYTES => "CONST_BYTES succeeds",
        OP_LOOP_INDEX => "LOOP_INDEX succeeds",
        OP_MOVE => "MOVE succeeds",
    );
}

/// As `frame_loads_and_constants`, for arithmetic, comparisons, boolean logic, `SELECT`, casts and
/// `REQUIRE`. Bound: as `frame_without_accounts`.
#[kani::proof]
#[kani::unwind(33)]
fn frame_arithmetic_and_logic() {
    frames!(frame_without_accounts:
        OP_ADD => "ADD succeeds",
        OP_SUB => "SUB succeeds",
        OP_MUL => "MUL succeeds",
        OP_DIV => "DIV succeeds",
        OP_MIN => "MIN succeeds",
        OP_MAX => "MAX succeeds",
        OP_EQ => "EQ succeeds",
        OP_NE => "NE succeeds",
        OP_LT => "LT succeeds",
        OP_LTE => "LTE succeeds",
        OP_GT => "GT succeeds",
        OP_GTE => "GTE succeeds",
        OP_AND => "AND succeeds",
        OP_OR => "OR succeeds",
        OP_NOT => "NOT succeeds",
        OP_SELECT => "SELECT succeeds",
        OP_CAST_U64 => "CAST_U64 succeeds",
        OP_CAST_I64 => "CAST_I64 succeeds",
        OP_CAST_U128 => "CAST_U128 succeeds",
        OP_REQUIRE => "REQUIRE succeeds",
    );
}

/// As `frame_loads_and_constants`, for the opcodes from `MUL_DIV` up that touch only registers,
/// except the multiply-divides (see `frame_mul_div`). Bound: as `frame_without_accounts`.
#[kani::proof]
#[kani::unwind(5)]
fn frame_math() {
    frames!(frame_without_accounts:
        OP_POW10 => "POW10 succeeds",
        OP_REM => "REM succeeds",
        OP_SHL => "SHL succeeds",
        OP_SHR => "SHR succeeds",
        OP_BIT_AND => "BIT_AND succeeds",
        OP_BIT_OR => "BIT_OR succeeds",
        OP_BIT_XOR => "BIT_XOR succeeds",
        OP_BYTES_LEN => "BYTES_LEN succeeds",
    );
}

/// A register value as `any_runtime_value` gives, but a `u64` or `u128` is windowed (12 symbolic
/// bits; see `util::windowed_u64`).
fn windowed_value(bytes: &[u8]) -> RuntimeValue<'_> {
    match any_runtime_value(bytes) {
        RuntimeValue::U64(_) => RuntimeValue::U64(crate::util::windowed_u64(4)),
        RuntimeValue::U128(_) => RuntimeValue::U128(crate::util::windowed_u128(4).to_le_bytes()),
        other => other,
    }
}

/// As `frame_loads_and_constants`, for `MUL_DIV` and `MUL_DIV_CEIL`. Their frame is the same as
/// every other opcode's, but at full width the solver would also re-prove `mul_div`'s absence of
/// panics, which `muldiv.rs` covers. Bound: as `frame_without_accounts`, with every `u64` and
/// `u128` register windowed to 12 symbolic bits.
#[kani::proof]
#[kani::unwind(5)]
fn frame_mul_div() {
    let ok = frame_without_accounts_using(OP_MUL_DIV, windowed_value);
    kani::cover!(ok, "MUL_DIV succeeds");
    let ok = frame_without_accounts_using(OP_MUL_DIV_CEIL, windowed_value);
    kani::cover!(ok, "MUL_DIV_CEIL succeeds");
}

/// As `frame_loads_and_constants`, for the outputs, `INVOKE`, `RETURN_DATA` and the clock. See the
/// module notes: off-chain, `RETURN_DATA` and the clock only fail and `INVOKE` (with no
/// descriptors) succeeds only when its guard skips it, so their covers are expected to be
/// unsatisfiable or to show only the skip; the outputs encode for real. Bound: as
/// `frame_without_accounts`.
#[kani::proof]
#[kani::unwind(5)]
fn frame_outputs_calls_and_clock() {
    frames!(frame_without_accounts:
        OP_EMIT => "EMIT succeeds",
        OP_SET_RETURN_DATA => "SET_RETURN_DATA succeeds",
        OP_INVOKE => "INVOKE skipped by its guard succeeds",
        OP_RETURN_DATA => "RETURN_DATA succeeds (expected unsatisfiable off-chain)",
        OP_CLOCK_SLOT => "CLOCK_SLOT succeeds (expected unsatisfiable off-chain)",
        OP_CLOCK_TIMESTAMP => "CLOCK_TIMESTAMP succeeds (expected unsatisfiable off-chain)",
    );
}

/// Every opcode value the executor does not run, the two loop opcodes among them (only `dispatch`
/// starts a loop), fails and changes no register: all 182 of 0, 39, `FOREACH`, `REPEAT` and 78 to
/// 255, each with every operand symbolic. Bound: as `frame_without_accounts`.
#[kani::proof]
#[kani::unwind(179)]
fn frame_unknown_opcodes() {
    let mut fails = true;
    for opcode in [0u8, 39, OP_FOREACH, OP_REPEAT] {
        fails &= !frame_without_accounts(opcode);
    }
    for opcode in 78..=255u8 {
        fails &= !frame_without_accounts(opcode);
    }
    assert!(fails);
}

/// Accounts' data bytes in `frame_with_accounts`: enough for a registry field after the 72-byte
/// entry header.
const ACCOUNT_DATA: usize = 80;

/// As `frame_without_accounts`, in a program with two fixed accounts (any constraints), two
/// symbolic data segments, and two runtime accounts of any owner, lamports, flags, borrow state
/// and data (up to 80 bytes). Also
/// checks that account data changes only by `WRITE_REGISTRY` into an entry this run has open (its
/// data exclusively borrowed), and only in the account its `b` names. The accounts' addresses are
/// concrete and not the Instructions sysvar's. The calling harness stubs the PDA search and
/// creation where it runs them. Returns whether it succeeded.
fn frame_with_accounts(opcode: u8) -> bool {
    let header = ProgramHeader::new(2, 0, 0, 0, 0, REGISTERS as u8, 1, 0, 0, 2, 1, 0, 8, 0, 0);
    let constraints = [any::constraint(), any::constraint()];
    let segments = [any::segment(), any::segment()];
    let pubkeys = [PubkeyRecord { bytes: kani::any() }];
    let blob: [u8; 8] = kani::any();
    let mut instruction = any::instruction();
    instruction.opcode = opcode;
    let program = ProgramView {
        header: &header,
        accounts: &constraints,
        inputs: &[],
        instructions: core::slice::from_ref(&instruction),
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &segments,
        pubkeys: &pubkeys,
        blob: &blob,
    };
    let account_fields = |address: u8| {
        let mut fields = Fields::plain([address; 32], kani::any());
        fields.lamports = kani::any();
        fields.signer = kani::any();
        fields.writable = kani::any();
        fields.executable = kani::any();
        fields.borrow_state = kani::any();
        fields
    };
    let (data_a, data_b): ([u8; ACCOUNT_DATA], [u8; ACCOUNT_DATA]) = (kani::any(), kani::any());
    let len_a: usize = kani::any_where(|n: &usize| *n <= ACCOUNT_DATA);
    let len_b: usize = kani::any_where(|n: &usize| *n <= ACCOUNT_DATA);
    let mut memory_a = AccountMemory::new(account_fields(1), data_a, len_a);
    let mut memory_b = AccountMemory::new(account_fields(2), data_b, len_b);
    let open = [memory_a.header.borrow_state == 0, memory_b.header.borrow_state == 0];
    let accounts = [memory_a.view(), memory_b.view()];

    let bytes: [u8; 4] = kani::any();
    let mut registers: [RuntimeValue<'_>; REGISTERS] = core::array::from_fn(|_| any_runtime_value(&bytes));
    let before = registers;
    let mut scratch = Scratch::new(&program);
    let result = execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &instruction, any_loop_context());

    for register in 0..REGISTERS {
        if result.is_err() || register != instruction.dst as usize {
            assert!(same_value(&registers[register], &before[register]));
        }
    }
    let writer = opcode == OP_WRITE_REGISTRY;
    let (may_write_a, may_write_b) = (writer && open[0] && instruction.b == 0, writer && open[1] && instruction.b == 1);
    for i in 0..ACCOUNT_DATA {
        assert!(memory_a.data[i] == data_a[i] || may_write_a);
        assert!(memory_b.data[i] == data_b[i] || may_write_b);
    }
    result.is_ok()
}

/// The frame property with real accounts, for the account fields, typed reads and byte reads.
/// Bound: as `frame_with_accounts`.
#[kani::proof]
#[kani::unwind(81)]
fn frame_account_reads() {
    frames!(frame_with_accounts:
        OP_ACCOUNT_KEY => "ACCOUNT_KEY succeeds",
        OP_ACCOUNT_OWNER => "ACCOUNT_OWNER succeeds",
        OP_ACCOUNT_LAMPORTS => "ACCOUNT_LAMPORTS succeeds",
        OP_ACCOUNT_DATA_LEN => "ACCOUNT_DATA_LEN succeeds",
        OP_ACCOUNT_IS_EMPTY => "ACCOUNT_IS_EMPTY succeeds",
        OP_READ_U8 => "READ_U8 succeeds",
        OP_READ_U16 => "READ_U16 succeeds",
        OP_READ_U32 => "READ_U32 succeeds",
        OP_READ_U64 => "READ_U64 succeeds",
        OP_READ_I64 => "READ_I64 succeeds",
        OP_READ_I32 => "READ_I32 succeeds",
        OP_READ_U128 => "READ_U128 succeeds",
        OP_READ_PUBKEY => "READ_PUBKEY succeeds",
        OP_READ_BOOL => "READ_BOOL succeeds",
        OP_READ_ACCOUNT_BYTES => "READ_ACCOUNT_BYTES succeeds",
    );
}

/// The frame property with real accounts for the registry field opcodes, the only ones that write
/// account data, and the registry open (which, run outside `run`, has no template and fails).
/// Bound: as `frame_with_accounts`.
#[kani::proof]
#[kani::unwind(81)]
fn frame_registry_fields() {
    frames!(frame_with_accounts:
        OP_READ_REGISTRY => "READ_REGISTRY succeeds",
        OP_WRITE_REGISTRY => "WRITE_REGISTRY succeeds",
        OP_OPEN_REGISTRY => "OPEN_REGISTRY succeeds (expected unsatisfiable: no template outside run)",
    );
}

/// The frame property with real accounts for the PDA opcodes (the search and creation stubbed to
/// return any address or none) and the introspection opcodes (which fail at their pin check: the
/// accounts are not the Instructions sysvar, whose parser trusts that layout by design). Bound: as
/// `frame_with_accounts`.
#[kani::proof]
#[kani::unwind(81)]
#[kani::stub(ballista::utils::pda::try_find_program_address, crate::stubs::try_find_program_address)]
#[kani::stub(ballista::utils::pda::create_program_address, crate::stubs::create_program_address)]
fn frame_pdas_and_introspection() {
    frames!(frame_with_accounts:
        OP_DERIVE_PDA => "DERIVE_PDA succeeds",
        OP_CREATE_PDA => "CREATE_PDA succeeds",
        OP_INSTRUCTION_COUNT => "INSTRUCTION_COUNT succeeds (expected unsatisfiable: no sysvar)",
        OP_INSTRUCTION_INDEX => "INSTRUCTION_INDEX succeeds (expected unsatisfiable: no sysvar)",
        OP_INSTRUCTION_PROGRAM => "INSTRUCTION_PROGRAM succeeds (expected unsatisfiable: no sysvar)",
        OP_INSTRUCTION_ACCOUNT_COUNT => "INSTRUCTION_ACCOUNT_COUNT succeeds (expected unsatisfiable: no sysvar)",
        OP_INSTRUCTION_ACCOUNT => "INSTRUCTION_ACCOUNT succeeds (expected unsatisfiable: no sysvar)",
        OP_INSTRUCTION_ACCOUNT_FLAGS => "INSTRUCTION_ACCOUNT_FLAGS succeeds (expected unsatisfiable: no sysvar)",
        OP_INSTRUCTION_DATA_LEN => "INSTRUCTION_DATA_LEN succeeds (expected unsatisfiable: no sysvar)",
        OP_READ_INSTRUCTION_DATA => "READ_INSTRUCTION_DATA succeeds (expected unsatisfiable: no sysvar)",
        OP_READ_INSTRUCTION_BYTES => "READ_INSTRUCTION_BYTES succeeds (expected unsatisfiable: no sysvar)",
    );
}

/// Registers in the loop proofs.
const LOOP_REGISTERS: usize = 3;

/// Lamports of loop account `i`, so a lamports read names the account it came from.
const fn lamports_of(index: usize) -> u64 {
    1000 + index as u64
}

/// A body instruction for the loop proofs: a constant, a move, an add, the loop index, a row or
/// fixed account's lamports, a row or fixed input, or a nested loop (which must fail), with
/// operands that can also name a register past the file.
fn body_instruction() -> InstructionRecord {
    let opcode = one_of(&[OP_CONST_U64, OP_MOVE, OP_ADD, OP_LOOP_INDEX, OP_ACCOUNT_LAMPORTS, OP_LOAD_INPUT, OP_REPEAT]);
    let small = || kani::any_where(|n: &u8| (*n as usize) <= LOOP_REGISTERS);
    let reference = || {
        let offset = kani::any_where(|n: &u8| *n <= 2);
        if kani::any() { offset | ITERATION_ACCOUNT_BIT } else { offset }
    };
    let (a, b) = match opcode {
        OP_ACCOUNT_LAMPORTS | OP_LOAD_INPUT => (reference(), small()),
        _ => (small(), small()),
    };
    record(opcode, small(), a, b, 1, 0, kani::any_where(|n: &u64| *n <= 3))
}

/// The registers a loop leaves behind, computed from the documented rule rather than the
/// executor's snapshot bookkeeping: each pass starts from the pre-loop registers with the carried
/// ones replaced by their value at the end of the previous pass; after the last pass the carried
/// registers keep their final value and every other register is back to its pre-loop value. Each
/// body instruction runs through `execute_instruction`, with the pass and the row base a FOREACH
/// gives it (`fixed + pass·stride`, by multiplication); a failure at body position `k` is
/// reported at program counter `body_start + k`, as `dispatch` reports it.
#[allow(clippy::too_many_arguments)]
fn loop_model<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    body_start: usize,
    body: &[InstructionRecord],
    passes: usize,
    rows: Option<(usize, usize)>,
    carry: u64,
    registers: &mut [RuntimeValue<'data>; LOOP_REGISTERS],
) -> Result<(), ProgramError> {
    let carried = |register: usize| carry & (1u64 << register) != 0;
    let snapshot = *registers;
    let mut state = *registers;
    let mut scratch = Scratch::new(program);
    for pass in 0..passes {
        let mut file: [RuntimeValue<'data>; LOOP_REGISTERS] =
            core::array::from_fn(|r| if carried(r) { state[r] } else { snapshot[r] });
        let row_base = match rows {
            Some((fixed, stride)) => fixed + pass * stride,
            None => NO_ROWS,
        };
        for (k, instruction) in body.iter().enumerate() {
            execute_instruction(program, inputs, accounts, &mut file, &mut scratch, instruction, Some((pass, row_base)))
                .map_err(|error| failure_at(error, body_start + k))?;
        }
        state = file;
    }
    if passes > 0 {
        *registers = core::array::from_fn(|r| if carried(r) { state[r] } else { snapshot[r] });
    }
    Ok(())
}

/// Loop accounts: account `i` is at `[i + 1; 32]` with `lamports_of(i)` lamports.
fn loop_accounts<const N: usize>() -> [AccountMemory<0>; N] {
    core::array::from_fn(|i| {
        let mut fields = Fields::plain([i as u8 + 1; 32], [0; 32]);
        fields.lamports = lamports_of(i);
        AccountMemory::new(fields, [], 0)
    })
}

/// A loop (`REPEAT` or `FOREACH`) followed by one more instruction runs exactly as `loop_model`
/// says: the same outcome, the same error at the same program counter, and the same final
/// registers. That covers the carry mask (any `u64`, bits past the register file included), the
/// restore and its skip when the body writes only carried registers, the pass count (a `REPEAT`
/// reads its count register once; a count above `c` fails with `LoopCountExceeded`, a non-`u64`
/// count with `TypeMismatch` or `InvalidRegister`), `LOOP_INDEX`, the row base each pass's row
/// accounts and row inputs resolve against, a loop inside a body failing, and execution resuming
/// after the body. Bound: 3 registers, a 2-instruction body, at most 3 passes (a `REPEAT`'s `c` is
/// 1 to 3, a batch has at most 3 rows), up to 2 fixed accounts, a batch stride of 1 or 2.
#[kani::proof]
#[kani::unwind(14)]
fn loops_carry_exactly_the_masked_registers() {
    let foreach: bool = kani::any();
    let fixed: u8 = kani::any_where(|n: &u8| *n <= 2);
    let stride: u8 = if foreach { kani::any_where(|n: &u8| (1..=2).contains(n)) } else { 0 };
    let iterations: usize = if foreach { kani::any_where(|n: &usize| *n <= 3) } else { 0 };
    let row_inputs: u8 = if foreach { 1 } else { 0 };
    let header = ProgramHeader::new(fixed, stride, 3, 0, 1, LOOP_REGISTERS as u8, 4, 0, 0, 0, 0, 0, 0, row_inputs, 0);
    let carry: u64 = kani::any();
    let count_register = kani::any_where(|n: &u8| (*n as usize) <= LOOP_REGISTERS);
    let max = kani::any_where(|n: &u8| (1..=3).contains(n));
    let entry = if foreach {
        record(OP_FOREACH, NO_INDEX, 2, 0, 0, 0, carry)
    } else {
        record(OP_REPEAT, NO_INDEX, 2, count_register, max, 0, carry)
    };
    let after = body_instruction();
    // A loop at the root after the loop would start one of its own; this checks only the first.
    kani::assume(after.opcode != OP_REPEAT);
    let instructions = [entry, body_instruction(), body_instruction(), after];
    let constraints: [AccountConstraint; 4] = core::array::from_fn(|_| any::constraint());
    let inputs_table = [any::input(), any::input()];
    let program = ProgramView {
        header: &header,
        accounts: &constraints[..(fixed + stride) as usize],
        inputs: &inputs_table[..header.total_input_count()],
        instructions: &instructions,
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &[],
        pubkeys: &[],
        blob: &[],
    };
    let mut memory = loop_accounts::<8>();
    let accounts: [AccountView; 8] = {
        let base = memory.as_mut_ptr();
        core::array::from_fn(|i| unsafe { (*base.add(i)).view() })
    };
    let runtime = &accounts[..fixed as usize + stride as usize * iterations];
    // One fixed input, then one row input per row: value 100 + its index.
    let inputs: [RuntimeValue<'_>; 4] = core::array::from_fn(|i| RuntimeValue::U64(100 + i as u64));
    let inputs = &inputs[..1 + row_inputs as usize * iterations];
    let initial: [RuntimeValue<'_>; LOOP_REGISTERS] = core::array::from_fn(|_| match kani::any_where(|v: &u8| *v < 3) {
        0 => RuntimeValue::Unset,
        1 => RuntimeValue::U64(kani::any_where(|n: &u64| *n <= 4)),
        _ => RuntimeValue::Bool(kani::any()),
    });

    let mut real = initial;
    let mut scratch = Scratch::new(&program);
    let outcome = execute_program(&program, inputs, runtime, iterations, &mut real, &mut scratch);

    let mut expected = initial;
    let modelled = (|| {
        let passes = if foreach {
            iterations
        } else {
            match initial.get(count_register as usize) {
                Some(RuntimeValue::U64(count)) if *count <= max as u64 => *count as usize,
                Some(RuntimeValue::U64(_)) => return Err(vm_error(BallistaError::LoopCountExceeded, 0)),
                Some(RuntimeValue::Unset) | None => return Err(vm_error(BallistaError::InvalidRegister, 0)),
                Some(_) => return Err(vm_error(BallistaError::TypeMismatch, 0)),
            }
        };
        let rows = foreach.then_some((fixed as usize, stride as usize));
        loop_model(&program, inputs, runtime, 1, &instructions[1..3], passes, rows, carry, &mut expected)?;
        let mut scratch = Scratch::new(&program);
        execute_instruction(&program, inputs, runtime, &mut expected, &mut scratch, &after, None)
            .map_err(|error| failure_at(error, 3))
    })();

    assert_eq!(outcome, modelled);
    if outcome.is_ok() {
        for register in 0..LOOP_REGISTERS {
            assert!(same_value(&real[register], &expected[register]));
        }
    }
    let body_writes_carried_only = instructions[1..3]
        .iter()
        .all(|record| (record.dst as usize) >= LOOP_REGISTERS || carry & (1u64 << record.dst) != 0);
    kani::cover!(outcome.is_ok() && foreach && iterations == 3, "a FOREACH runs three rows");
    kani::cover!(
        outcome.is_ok() && !foreach && matches!(initial.get(count_register as usize), Some(RuntimeValue::U64(3))),
        "a REPEAT runs three passes"
    );
    kani::cover!(outcome.is_ok() && body_writes_carried_only && iterations > 1, "the restore is skipped");
    kani::cover!(outcome == Err(vm_error(BallistaError::LoopCountExceeded, 0)), "a count above its maximum");
    kani::cover!(
        outcome.is_ok() && iterations == 2 && instructions[1].opcode == OP_ACCOUNT_LAMPORTS && instructions[1].a & ITERATION_ACCOUNT_BIT != 0,
        "a row account read on every row"
    );
}

/// Two loops in a row, each with a one-instruction body: the second starts from the registers the
/// first left behind (not from the first loop's snapshot, whose buffer it reuses), and every
/// FOREACH starts again at the first row. Bound: 3 registers, at most 2 passes per loop, 2 fixed
/// accounts, stride 1.
#[kani::proof]
#[kani::unwind(10)]
fn consecutive_loops_start_from_the_registers_the_last_one_left() {
    let header = ProgramHeader::new(2, 1, 2, 0, 1, LOOP_REGISTERS as u8, 4, 0, 0, 0, 0, 0, 0, 1, 0);
    let iterations: usize = kani::any_where(|n: &usize| *n <= 2);
    let make_loop = || {
        if kani::any() {
            record(OP_FOREACH, NO_INDEX, 1, 0, 0, 0, kani::any())
        } else {
            let count = kani::any_where(|n: &u8| (*n as usize) < LOOP_REGISTERS);
            record(OP_REPEAT, NO_INDEX, 1, count, 2, 0, kani::any())
        }
    };
    let (first, second) = (make_loop(), make_loop());
    let instructions = [first, body_instruction(), second, body_instruction()];
    let constraints: [AccountConstraint; 3] = core::array::from_fn(|_| any::constraint());
    let inputs_table = [any::input(), any::input()];
    let program = ProgramView {
        header: &header,
        accounts: &constraints,
        inputs: &inputs_table,
        instructions: &instructions,
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &[],
        pubkeys: &[],
        blob: &[],
    };
    let mut memory = loop_accounts::<4>();
    let accounts: [AccountView; 4] = {
        let base = memory.as_mut_ptr();
        core::array::from_fn(|i| unsafe { (*base.add(i)).view() })
    };
    let runtime = &accounts[..2 + iterations];
    let inputs: [RuntimeValue<'_>; 3] = core::array::from_fn(|i| RuntimeValue::U64(100 + i as u64));
    let inputs = &inputs[..1 + iterations];
    let initial: [RuntimeValue<'_>; LOOP_REGISTERS] =
        core::array::from_fn(|_| RuntimeValue::U64(kani::any_where(|n: &u64| *n <= 2)));

    let mut real = initial;
    let mut scratch = Scratch::new(&program);
    let outcome = execute_program(&program, inputs, runtime, iterations, &mut real, &mut scratch);

    let mut expected = initial;
    let modelled = (|| {
        for (pc, entry) in [(0usize, first), (2, second)] {
            let passes = if entry.opcode == OP_FOREACH {
                iterations
            } else {
                match expected[entry.b as usize] {
                    RuntimeValue::U64(count) if count <= 2 => count as usize,
                    RuntimeValue::U64(_) => return Err(vm_error(BallistaError::LoopCountExceeded, pc as u16)),
                    RuntimeValue::Unset => return Err(vm_error(BallistaError::InvalidRegister, pc as u16)),
                    _ => return Err(vm_error(BallistaError::TypeMismatch, pc as u16)),
                }
            };
            let rows = (entry.opcode == OP_FOREACH).then_some((2, 1));
            let body = &instructions[pc + 1..pc + 2];
            loop_model(&program, inputs, runtime, pc + 1, body, passes, rows, entry.immediate(), &mut expected)?;
        }
        Ok(())
    })();
    assert_eq!(outcome, modelled);
    if outcome.is_ok() {
        for register in 0..LOOP_REGISTERS {
            assert!(same_value(&real[register], &expected[register]));
        }
    }
    kani::cover!(
        outcome.is_ok() && first.opcode == OP_FOREACH && second.opcode == OP_FOREACH && iterations == 2,
        "two FOREACH loops over two rows"
    );
    kani::cover!(outcome.is_ok() && first.opcode == OP_REPEAT && second.opcode == OP_FOREACH, "a REPEAT then a FOREACH");
}

/// Accounts in the account-check proof.
const RUNTIME_ACCOUNTS: usize = 5;

/// `validate_runtime_accounts` accepts exactly the account lists the schema allows, and describes
/// them exactly. It succeeds if and only if: one group length is given per declared group; the
/// accounts after the fixed ones and before the groups fill whole batch rows (none without a
/// batch), between the minimum and maximum row counts; and each fixed account meets its own
/// constraint and each row account its slot's row constraint (signer, writable, executable, pinned
/// address, pinned owner, minimum data length). On success the layout has that row count and the
/// groups start right after the last row, one after another. Bound: up to 5 accounts of any flags,
/// one of two addresses, one of two owners, data length up to 3; up to 2 fixed accounts, a stride
/// up to 2, up to 2 groups of up to 3 accounts; every constraint field, row bound and group length
/// symbolic.
#[kani::proof]
#[kani::unwind(7)]
fn account_checks_enforce_the_schema_exactly() {
    let fixed: u8 = kani::any_where(|n: &u8| *n <= 2);
    let stride: u8 = kani::any_where(|n: &u8| *n <= 2);
    let groups: u8 = kani::any_where(|n: &u8| *n <= 2);
    let (min_rows, max_rows): (u8, u8) = (kani::any_where(|n: &u8| *n <= 3), kani::any_where(|n: &u8| *n <= 3));
    let header = ProgramHeader::new(fixed, stride, max_rows, min_rows, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, groups);
    let keys = [[7u8; 32], [9u8; 32]];
    let constraint = || {
        let constraint = any::constraint();
        kani::assume(constraint.address_index == NO_INDEX || constraint.address_index < 2);
        kani::assume(constraint.owner_index == NO_INDEX || constraint.owner_index < 2);
        kani::assume(constraint.min_data_len() <= 4);
        constraint
    };
    let constraints: [AccountConstraint; 4] = core::array::from_fn(|_| constraint());
    let pubkeys = [PubkeyRecord { bytes: keys[0] }, PubkeyRecord { bytes: keys[1] }];
    let program = ProgramView {
        header: &header,
        accounts: &constraints[..(fixed + stride) as usize],
        inputs: &[],
        instructions: &[],
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &[],
        pubkeys: &pubkeys,
        blob: &[],
    };
    let mut memory: [AccountMemory<0>; RUNTIME_ACCOUNTS] = core::array::from_fn(|_| {
        let mut fields = Fields::plain(keys[kani::any::<bool>() as usize], keys[kani::any::<bool>() as usize]);
        fields.signer = kani::any();
        fields.writable = kani::any();
        fields.executable = kani::any();
        let mut account = AccountMemory::new(fields, [], 0);
        account.header.data_len = kani::any_where(|n: &u64| *n <= 3);
        account
    });
    let count: usize = kani::any_where(|n: &usize| *n <= RUNTIME_ACCOUNTS);
    let accounts: [AccountView; RUNTIME_ACCOUNTS] = {
        let base = memory.as_mut_ptr();
        core::array::from_fn(|i| unsafe { (*base.add(i)).view() })
    };
    let accounts = &accounts[..count];
    let lengths: [u8; 3] = core::array::from_fn(|_| kani::any_where(|n: &u8| *n <= 3));
    let given: usize = kani::any_where(|n: &usize| *n <= 3);
    let group_lengths = &lengths[..given];

    let result = validate_runtime_accounts(&program, accounts, group_lengths);

    // The schema, stated directly.
    let (fixed, stride) = (fixed as usize, stride as usize);
    let group_total: usize = group_lengths.iter().map(|len| *len as usize).sum();
    let rows = count.checked_sub(fixed).and_then(|rest| rest.checked_sub(group_total));
    let iterations = match rows {
        Some(0) if stride == 0 => Some(0),
        Some(rows) if stride != 0 && rows % stride == 0 => Some(rows / stride),
        _ => None,
    };
    let shape_ok = given == groups as usize
        && iterations.is_some_and(|n| stride == 0 || (min_rows as usize <= n && n <= max_rows as usize));
    let meets = |account: &AccountView, constraint: &AccountConstraint| {
        (constraint.flags & ACCOUNT_SIGNER == 0 || account.is_signer())
            && (constraint.flags & ACCOUNT_WRITABLE == 0 || account.is_writable())
            && (constraint.flags & ACCOUNT_EXECUTABLE == 0 || account.executable())
            && (constraint.address_index == NO_INDEX
                || eq32(account.address().as_array(), &keys[constraint.address_index as usize]))
            && (constraint.owner_index == NO_INDEX
                || eq32(account.owner().as_array(), &keys[constraint.owner_index as usize]))
            && account.data_len() >= constraint.min_data_len()
    };
    let declared = rows.map(|rows| fixed + rows).unwrap_or(0);
    let constraints_ok = shape_ok
        && (0..declared).all(|index| {
            let slot = if index < fixed { index } else { fixed + (index - fixed) % stride };
            meets(&accounts[index], &constraints[slot])
        });

    assert_eq!(result.is_ok(), shape_ok && constraints_ok);
    if let Ok(layout) = result {
        assert_eq!(layout.iterations, iterations.unwrap());
        assert_eq!(layout.declared, declared);
        let mut start = declared;
        for group in 0..MAX_ACCOUNT_GROUPS {
            if group < given {
                assert_eq!(layout.groups[group], (start as u8, group_lengths[group]));
                start += group_lengths[group] as usize;
            } else {
                assert_eq!(layout.groups[group], (0, 0));
            }
        }
        assert_eq!(start, count);
        kani::cover!(layout.iterations == 2 && given == 1, "two rows and a group");
    }
    kani::cover!(shape_ok && !constraints_ok, "a constraint fails on a well-shaped list");
}

/// In a FOREACH pass over row `n`, a row account reference `0x80 | o` resolves to the runtime
/// account at `fixed + n·stride + o`: the account `validate_runtime_accounts` checked against row
/// constraint `o`, which is the constraint the verifier's `account_constraint` names for the same
/// reference; a fixed reference resolves to its own account. In a REPEAT (row base `NO_ROWS`) or
/// outside a loop, no row reference resolves. Bound: up to 2 fixed accounts, a stride up to 3, up
/// to 3 rows; every reference byte.
#[kani::proof]
#[kani::unwind(12)]
fn row_references_resolve_to_the_validated_account() {
    let fixed: usize = kani::any_where(|n: &usize| *n <= 2);
    let stride: usize = kani::any_where(|n: &usize| (1..=3).contains(n));
    let iterations: usize = kani::any_where(|n: &usize| (1..=3).contains(n));
    let header = ProgramHeader::new(fixed as u8, stride as u8, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let constraints: [AccountConstraint; 5] = core::array::from_fn(|_| any::constraint());
    let program = ProgramView {
        header: &header,
        accounts: &constraints[..fixed + stride],
        inputs: &[],
        instructions: &[],
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &[],
        pubkeys: &[],
        blob: &[],
    };
    let mut memory = loop_accounts::<11>();
    let accounts: [AccountView; 11] = {
        let base = memory.as_mut_ptr();
        core::array::from_fn(|i| unsafe { (*base.add(i)).view() })
    };
    let runtime = &accounts[..fixed + stride * iterations];
    let reference: u8 = kani::any();
    let pass: usize = kani::any_where(|n: &usize| *n < iterations);
    let row_base = fixed + pass * stride;
    let row = reference & ITERATION_ACCOUNT_BIT != 0;

    let resolved = resolve_account(&program, runtime, reference, Some((pass, row_base)));
    let named = program.account_constraint(reference, true);
    assert_eq!(resolved.is_ok(), named.is_some());
    if let (Ok(account), Some(named)) = (resolved, named) {
        let index = (0..runtime.len()).find(|i| core::ptr::eq(account, &runtime[*i])).expect("a runtime account");
        // The constraint `validate_runtime_accounts` checked this account against.
        let checked = if index < fixed { index } else { fixed + (index - fixed) % stride };
        assert!(core::ptr::eq(named, &program.accounts[checked]));
        if row {
            assert_eq!(index, row_base + (reference & !ITERATION_ACCOUNT_BIT) as usize);
        }
        kani::cover!(row && pass == 2, "a row account on the third row");
    }
    if row {
        assert!(resolve_account(&program, runtime, reference, Some((pass, NO_ROWS))).is_err());
        assert!(resolve_account(&program, runtime, reference, None).is_err());
    }
}
