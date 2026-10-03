//! The verifier-executor soundness step, one instruction at a time: if every register holds a
//! value of the type the verifier recorded for it, and the verifier accepts an instruction against
//! that typing, then executing it either succeeds, leaving the destination holding the type the
//! verifier recorded and every other register as it was, or fails only for a reason that depends
//! on the values (an overflow, a division by zero, a failed requirement). Induction over the
//! instructions gives the guarantee docs/guide/trust-model.md ("Finalization checks") makes: a
//! verified template never fails at run time for a structural reason.
//!
//! This is the Certora typing rule (`certora/ballista-specs/src/rules/typing.rs`), which is
//! blocked on that prover's heap model, restated for Kani with each opcode concrete: the solver
//! then follows one arm of the verifier and one of the executor, not all of them.
//!
//! Scope: the opcodes that touch only registers, inputs, the blob, pubkeys and the outputs. The
//! account, PDA, CPI, registry and introspection opcodes need declared accounts and are left to
//! the frame and account proofs in `executor.rs`; `RETURN_DATA` needs a preceding `INVOKE`, which
//! `verify_single_instruction` cannot see, so the verifier rejects it here.

use ballista::error::BallistaError;
use ballista::processor::execute::{execute_instruction, RunError, RuntimeValue, Scratch, NO_ROWS};
use ballista_common::template::*;

use crate::util::{any, one_of};

/// Registers in these proofs; the verifier's table has `MAX_REGISTERS` entries, the rest unset.
const REGISTERS: usize = 4;

/// The longest `bytes` value a register or an input holds here.
const BYTES_MAX: usize = 4;

/// Any register typing: unset, one of the five scalar types, or `bytes` of a maximum up to
/// `BYTES_MAX`.
fn any_info() -> Option<RegisterInfo> {
    match one_of(&[0u8, VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY, VALUE_BYTES]) {
        0 => None,
        VALUE_BYTES => Some(RegisterInfo::bytes(kani::any_where(|n: &usize| *n <= BYTES_MAX))),
        scalar => Some(RegisterInfo::scalar(scalar)),
    }
}

/// Any value of the type `info` records: the invariant a verified program keeps.
fn value_for(info: Option<RegisterInfo>, pool: &[u8; BYTES_MAX]) -> RuntimeValue<'_> {
    let Some(info) = info else {
        return RuntimeValue::Unset;
    };
    match info.value_type {
        VALUE_BOOL => RuntimeValue::Bool(kani::any()),
        VALUE_U64 => RuntimeValue::U64(kani::any()),
        VALUE_I64 => RuntimeValue::I64(kani::any()),
        VALUE_U128 => RuntimeValue::U128(kani::any()),
        VALUE_PUBKEY => RuntimeValue::Pubkey(kani::any()),
        _ => {
            let len: usize = kani::any_where(|len: &usize| *len <= info.bytes_max_len.min(BYTES_MAX));
            RuntimeValue::Bytes(&pool[..len])
        }
    }
}

/// The type a register holds, if any.
fn runtime_type(value: &RuntimeValue<'_>) -> Option<u8> {
    match value {
        RuntimeValue::Unset => None,
        RuntimeValue::Bool(_) => Some(VALUE_BOOL),
        RuntimeValue::U64(_) => Some(VALUE_U64),
        RuntimeValue::I64(_) => Some(VALUE_I64),
        RuntimeValue::U128(_) => Some(VALUE_U128),
        RuntimeValue::Pubkey(_) => Some(VALUE_PUBKEY),
        RuntimeValue::Bytes(_) => Some(VALUE_BYTES),
    }
}

/// A valid input descriptor, as `verify` requires of every one: a type byte of 1 to 6, a `bytes`
/// maximum of 1 to `BYTES_MAX`, and no maximum for the other types.
fn valid_input() -> InputDescriptor {
    let value_type = one_of(&[VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY, VALUE_BYTES]);
    let max_len: u16 = if value_type == VALUE_BYTES { kani::any_where(|n: &u16| (1..=BYTES_MAX as u16).contains(n)) } else { 0 };
    InputDescriptor { value_type, reserved: 0, max_len_le: max_len.to_le_bytes() }
}

/// The input value a descriptor admits: what `parse_run_inputs` produces for it (proved in
/// `encoding.rs`).
fn input_for<'a>(descriptor: &InputDescriptor, pool: &'a [u8; BYTES_MAX]) -> RuntimeValue<'a> {
    let info = if descriptor.value_type == VALUE_BYTES {
        RegisterInfo::bytes(descriptor.max_len())
    } else {
        RegisterInfo::scalar(descriptor.value_type)
    };
    value_for(Some(info), pool)
}

/// Runs the soundness step for `opcode`, every other field of the record symbolic, in a program
/// with one fixed input and one row input (two batch rows of one row account each), a 16-byte
/// blob, one pubkey and two data segments, all symbolic; 4 registers with any typing and values
/// of those types; any scope (root, a FOREACH pass over either row, a REPEAT pass). Returns whether
/// the verifier accepted the instruction and it ran to success.
fn typing_preserved(opcode: u8) -> bool {
    let header = ProgramHeader::new(0, 1, 2, 0, 1, REGISTERS as u8, 1, 0, 0, 2, 1, 0, 16, 1, 0);
    let constraints = [any::constraint()];
    let inputs_table = [valid_input(), valid_input()];
    let segments = [any::segment(), any::segment()];
    let pubkeys = [PubkeyRecord { bytes: kani::any() }];
    let blob: [u8; 16] = kani::any();
    let mut instruction = any::instruction();
    instruction.opcode = opcode;
    let program = ProgramView {
        header: &header,
        accounts: &constraints,
        inputs: &inputs_table,
        instructions: core::slice::from_ref(&instruction),
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &segments,
        pubkeys: &pubkeys,
        blob: &blob,
    };

    let mut typing: [Option<RegisterInfo>; MAX_REGISTERS] = [None; MAX_REGISTERS];
    for register in typing.iter_mut().take(REGISTERS) {
        *register = any_info();
    }
    let before = typing;
    let scope = one_of(&[LoopScope::Root, LoopScope::Rows, LoopScope::Count]);
    let accepted = program.verify_single_instruction(&instruction, 0, scope, None, &mut typing).is_ok();
    if !accepted {
        return false;
    }

    let pool: [u8; BYTES_MAX] = kani::any();
    let mut registers: [RuntimeValue<'_>; REGISTERS] = core::array::from_fn(|r| value_for(before[r], &pool));
    let snapshot = registers;
    // The fixed input, then the row input of each of the two rows.
    let inputs = [
        input_for(&inputs_table[0], &pool),
        input_for(&inputs_table[1], &pool),
        input_for(&inputs_table[1], &pool),
    ];
    let pass: usize = kani::any_where(|pass: &usize| *pass < 2);
    let loop_context = match scope {
        LoopScope::Root => None,
        LoopScope::Rows => Some((pass, pass)),
        LoopScope::Count => Some((pass, NO_ROWS)),
    };
    let mut scratch = Scratch::new(&program);
    let outcome = execute_instruction(&program, &inputs, &[], &mut registers, &mut scratch, &instruction, loop_context);

    match &outcome {
        Ok(()) => {
            let dst = instruction.dst as usize;
            for register in 0..REGISTERS {
                let expected = typing[register];
                assert_eq!(runtime_type(&registers[register]), expected.map(|info| info.value_type));
                if let (RuntimeValue::Bytes(value), Some(info)) = (registers[register], expected) {
                    assert!(value.len() <= info.bytes_max_len);
                }
                if register != dst {
                    assert_eq!(typing[register], before[register]);
                    assert!(crate::util::same_value(&registers[register], &snapshot[register]));
                }
            }
        }
        Err(RunError::Vm(kind)) => assert!(matches!(
            kind,
            BallistaError::ArithmeticOverflow | BallistaError::DivisionByZero | BallistaError::RequirementFailed
        )),
        Err(RunError::VmAt(_, _)) => panic!("a verified instruction failed with an indexed error"),
        // Only the clock reads go to the runtime here, and off-chain it has no clock.
        Err(RunError::Program(_)) => assert!(matches!(opcode, OP_CLOCK_SLOT | OP_CLOCK_TIMESTAMP)),
    }
    outcome.is_ok()
}

/// One soundness step per opcode, each with a cover showing the verifier accepts it and it runs.
macro_rules! steps {
    ($($opcode:expr => $runs:literal),* $(,)?) => {{
        $(
            let ok = typing_preserved($opcode);
            kani::cover!(ok, $runs);
        )*
    }};
}

/// The soundness step for loads, constants, the loop index, moves, `SELECT`, `REQUIRE` and the
/// outputs. Bound: as `typing_preserved`.
#[kani::proof]
#[kani::unwind(5)]
fn verified_loads_constants_and_outputs_keep_their_types() {
    steps!(
        OP_LOAD_INPUT => "a verified LOAD_INPUT runs",
        OP_CONST_BOOL => "a verified CONST_BOOL runs",
        OP_CONST_U64 => "a verified CONST_U64 runs",
        OP_CONST_I64 => "a verified CONST_I64 runs",
        OP_CONST_U128 => "a verified CONST_U128 runs",
        OP_CONST_PUBKEY => "a verified CONST_PUBKEY runs",
        OP_CONST_BYTES => "a verified CONST_BYTES runs",
        OP_LOOP_INDEX => "a verified LOOP_INDEX runs",
        OP_MOVE => "a verified MOVE runs",
        OP_SELECT => "a verified SELECT runs",
        OP_REQUIRE => "a verified REQUIRE runs",
        OP_EMIT => "a verified EMIT runs",
        OP_SET_RETURN_DATA => "a verified SET_RETURN_DATA runs",
    );
}

/// The soundness step for arithmetic, comparisons, boolean logic and casts. Bound: as
/// `typing_preserved`.
#[kani::proof]
#[kani::unwind(33)]
fn verified_arithmetic_and_logic_keep_their_types() {
    steps!(
        OP_ADD => "a verified ADD runs",
        OP_SUB => "a verified SUB runs",
        OP_MUL => "a verified MUL runs",
        OP_DIV => "a verified DIV runs",
        OP_MIN => "a verified MIN runs",
        OP_MAX => "a verified MAX runs",
        OP_EQ => "a verified EQ runs",
        OP_NE => "a verified NE runs",
        OP_LT => "a verified LT runs",
        OP_LTE => "a verified LTE runs",
        OP_GT => "a verified GT runs",
        OP_GTE => "a verified GTE runs",
        OP_AND => "a verified AND runs",
        OP_OR => "a verified OR runs",
        OP_NOT => "a verified NOT runs",
        OP_CAST_U64 => "a verified CAST_U64 runs",
        OP_CAST_I64 => "a verified CAST_I64 runs",
        OP_CAST_U128 => "a verified CAST_U128 runs",
    );
}

/// The soundness step for the integer opcodes from `POW10` on and `BYTES_LEN`, and for the clock
/// (which off-chain only fails, with the runtime's error, never a VM one). `MUL_DIV` is left to
/// `muldiv.rs`: at full width its absence of panics is out of reach. Bound: as `typing_preserved`.
#[kani::proof]
#[kani::unwind(5)]
fn verified_integer_opcodes_keep_their_types() {
    steps!(
        OP_POW10 => "a verified POW10 runs",
        OP_REM => "a verified REM runs",
        OP_SHL => "a verified SHL runs",
        OP_SHR => "a verified SHR runs",
        OP_BIT_AND => "a verified BIT_AND runs",
        OP_BIT_OR => "a verified BIT_OR runs",
        OP_BIT_XOR => "a verified BIT_XOR runs",
        OP_BYTES_LEN => "a verified BYTES_LEN runs",
        OP_CLOCK_SLOT => "a verified CLOCK_SLOT runs (expected unsatisfiable off-chain)",
        OP_CLOCK_TIMESTAMP => "a verified CLOCK_TIMESTAMP runs (expected unsatisfiable off-chain)",
    );
}
