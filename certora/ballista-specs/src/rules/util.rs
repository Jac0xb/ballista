//! Shared helpers for building symbolic inputs to the executor and verifier.

use ballista::processor::execute::RuntimeValue;
use ballista_common::template::*;
use cvlr::asserts::cvlr_assume;
use cvlr::nondet::nondet;
use cvlr_pinocchio::{nondet_address, nondet_bytes};

/// Registers each spec program declares. Small enough for the prover, large enough for every
/// operand shape (two sources, a condition, and a destination).
pub const REGISTERS: usize = 4;

/// Longest `bytes` value a spec register holds.
pub const BYTES_MAX: usize = 8;

/// Picks one element of a small constant table nondeterministically.
pub fn pick<T: Copy>(options: &[T]) -> T {
    let index: usize = nondet();
    cvlr_assume!(index < options.len());
    options[index]
}

/// A fully nondeterministic instruction record. Every field is symbolic, including reserved bytes
/// and flags, so the verifier's rejection paths are exercised too.
pub fn nondet_instruction() -> InstructionRecord {
    InstructionRecord {
        opcode: nondet(),
        dst: nondet(),
        a: nondet(),
        b: nondet(),
        c: nondet(),
        flags: nondet(),
        immediate_le: nondet::<u64>().to_le_bytes(),
        reserved: [nondet(), nondet()],
    }
}

/// A nondeterministic register typing: uninitialized, one of the five scalar types, or `bytes`
/// with a maximum length up to [`BYTES_MAX`].
pub fn nondet_register_info() -> Option<RegisterInfo> {
    match pick(&[0u8, VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY, VALUE_BYTES]) {
        0 => None,
        VALUE_BYTES => {
            let max_len: usize = nondet();
            cvlr_assume!(max_len <= BYTES_MAX);
            Some(RegisterInfo::bytes(max_len))
        }
        scalar => Some(RegisterInfo::scalar(scalar)),
    }
}

/// A runtime register value consistent with a typing: the invariant the executor relies on.
pub fn runtime_value_for(info: Option<RegisterInfo>) -> RuntimeValue<'static> {
    match info {
        None => RuntimeValue::Unset,
        Some(info) => match info.value_type {
            VALUE_BOOL => RuntimeValue::Bool(nondet()),
            VALUE_U64 => RuntimeValue::U64(nondet()),
            VALUE_I64 => RuntimeValue::I64(nondet()),
            VALUE_U128 => RuntimeValue::U128(nondet::<u128>().to_le_bytes()),
            VALUE_PUBKEY => RuntimeValue::Pubkey(nondet_address().to_bytes()),
            _ => {
                let bytes = nondet_bytes::<BYTES_MAX>();
                let len: usize = nondet();
                cvlr_assume!(len <= info.bytes_max_len);
                RuntimeValue::Bytes(&bytes[..len])
            }
        },
    }
}

/// The value type a runtime register currently holds, if any.
pub fn runtime_type(value: RuntimeValue<'_>) -> Option<u8> {
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

/// Whether an accepted instruction writes its destination register.
pub fn writes_destination(opcode: u8) -> bool {
    !matches!(opcode, OP_REQUIRE | OP_INVOKE | OP_FOREACH)
}

/// A program with `REGISTERS` registers, two constant pubkeys, a 32-byte blob, and optionally one
/// unconstrained fixed account. It has no inputs, CPIs, or data segments, so the verifier only
/// accepts the pure instruction subset (plus account header reads when `with_account` is set).
pub fn spec_program(with_account: bool) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    if with_account {
        builder.account(0, None, None, 0);
    }
    for _ in 0..REGISTERS {
        builder.register();
    }
    builder.pubkey([1; 32]);
    builder.pubkey([2; 32]);
    builder.blob(&[3; 32]);
    builder.build().expect("spec program builds")
}

/// A program with exactly the given fixed account constraint and one trivial instruction.
pub fn constrained_program(
    flags: u8,
    address: Option<[u8; 32]>,
    owner: Option<[u8; 32]>,
    min_data_len: u32,
) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    builder.account(flags, address, owner, min_data_len);
    builder.const_bool(true);
    builder.build().expect("constrained program builds")
}
