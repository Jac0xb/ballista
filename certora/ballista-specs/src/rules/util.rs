//! Shared helpers for building symbolic inputs to the executor and verifier.
//!
//! Two limits of the prover's pointer analysis shape everything here. Stack memory is tracked slot
//! by slot, so a stack array indexed by a nondeterministic value is an analysis error rather than a
//! symbolic read. And a program built at analysis time with `ProgramBuilder` grows vectors from a
//! dangling pointer, which the heap model does not follow. Rules therefore run byte-constant
//! programs copied to the heap, and choose among alternatives with [`pick!`], a chain of
//! nondeterministic branches instead of a table lookup. The constants below are checked against
//! `ProgramBuilder` by the tests at the end of this file, which print the replacement bytes when
//! the wire format changes.

use ballista::processor::execute::RuntimeValue;
use ballista_common::template::*;
use cvlr::asserts::cvlr_assume;
use cvlr::nondet::nondet;
use cvlr_pinocchio::{heap_bytes, nondet_address, nondet_bytes};

/// Registers each spec program declares. Small enough for the prover, large enough for every
/// operand shape (two sources, a condition, and a destination).
pub const REGISTERS: usize = 4;

/// Longest `bytes` value a spec register holds.
pub const BYTES_MAX: usize = 8;

/// Address the pinned-account program requires.
pub const PINNED_ADDRESS: [u8; 32] = [1; 32];

/// Owner the pinned-account program requires.
pub const PINNED_OWNER: [u8; 32] = [2; 32];

/// One of the listed values, chosen nondeterministically. Every option is reachable: the first
/// when no branch is taken, any other when its branch is the last one taken.
macro_rules! pick {
    ($first:expr $(, $rest:expr)* $(,)?) => {{
        let mut chosen = $first;
        $(
            if cvlr::nondet::nondet::<bool>() {
                chosen = $rest;
            }
        )*
        chosen
    }};
}
pub(crate) use pick;

/// Offset of the first account constraint record: it follows the program header directly.
const FIRST_ACCOUNT: usize = PROGRAM_HEADER_LEN;

/// `REGISTERS` registers, two constant pubkeys, and a 32-byte blob. No accounts, inputs, CPIs, or
/// data segments, so the verifier accepts only the pure instruction subset against it.
pub const SPEC_PROGRAM_PURE: [u8; 120] = [
    66, 86, 77, 50, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0,
    2, 0, 32, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3,
];

/// [`SPEC_PROGRAM_PURE`] plus one unconstrained fixed account, which admits the account header
/// reads.
pub const SPEC_PROGRAM_WITH_ACCOUNT: [u8; 128] = [
    66, 86, 77, 50, 3, 1, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0,
    2, 0, 32, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
];

/// One fixed account with no flags, no pinned address or owner, and no minimum length, followed by
/// one trivial instruction. [`constrained_program`] patches the flags and minimum length in place.
pub const CONSTRAINED_PLAIN: [u8; 48] = [
    66, 86, 77, 50, 3, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0,
    2, 0, 1, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// One fixed account pinned to [`PINNED_ADDRESS`] and [`PINNED_OWNER`], and one trivial
/// instruction.
pub const CONSTRAINED_PINNED: [u8; 112] = [
    66, 86, 77, 50, 3, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0,
    2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
    2, 0, 1, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
];

/// The canonical SOL transfer template: System Program, signer, recipient, one `u64` input, and
/// one CPI. Identical to `fixtures/system-transfer.hex`.
pub const TRANSFER_PAYLOAD: [u8; 152] = [
    66, 86, 77, 50, 3, 3, 0, 0, 1, 1, 2, 1, 2, 0, 2, 0,
    1, 0, 4, 0, 0, 0, 0, 0, 4, 0, 255, 0, 0, 0, 0, 0,
    3, 255, 255, 0, 0, 0, 0, 0, 2, 255, 255, 0, 0, 0, 0, 0,
    2, 0, 0, 0, 1, 0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 41, 255, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 12, 0, 0, 0,
    1, 3, 2, 2, 0, 255, 0, 0, 4, 0, 0, 0, 4, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 2, 0, 0, 0,
];

/// The pure spec program, or the one with an account, on the heap.
pub fn spec_program(with_account: bool) -> &'static [u8] {
    if with_account {
        heap_bytes(&SPEC_PROGRAM_WITH_ACCOUNT)
    } else {
        heap_bytes(&SPEC_PROGRAM_PURE)
    }
}

/// A program whose single fixed account carries exactly `flags` and `min_data_len`, with no
/// pinned address or owner.
pub fn constrained_program(flags: u8, min_data_len: u32) -> &'static [u8] {
    let bytes = heap_bytes(&CONSTRAINED_PLAIN);
    bytes[FIRST_ACCOUNT] = flags;
    bytes[FIRST_ACCOUNT + 4..FIRST_ACCOUNT + 8].copy_from_slice(&min_data_len.to_le_bytes());
    bytes
}

/// A program whose single fixed account is pinned to [`PINNED_ADDRESS`] and [`PINNED_OWNER`].
pub fn pinned_program() -> &'static [u8] {
    heap_bytes(&CONSTRAINED_PINNED)
}

/// `REGISTERS` unset runtime registers.
pub fn unset_registers() -> Vec<RuntimeValue<'static>> {
    let mut registers = Vec::with_capacity(REGISTERS);
    for _ in 0..REGISTERS {
        registers.push(RuntimeValue::Unset);
    }
    registers
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
    match pick!(0u8, VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY, VALUE_BYTES) {
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

/// The constants above are derived from `ProgramBuilder`; these tests keep them honest and print
/// the replacement when they drift.
#[cfg(test)]
mod tests {
    use super::*;

    fn same(name: &str, constant: &[u8], built: &[u8]) {
        assert!(
            constant == built,
            "{name} is stale; replace it with\npub const {name}: [u8; {}] = {:?};",
            built.len(),
            built
        );
    }

    fn built_spec_program(with_account: bool) -> Vec<u8> {
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

    fn built_constrained_program(
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

    #[test]
    fn spec_programs_match_the_builder() {
        same("SPEC_PROGRAM_PURE", spec_program(false), &built_spec_program(false));
        same("SPEC_PROGRAM_WITH_ACCOUNT", spec_program(true), &built_spec_program(true));
    }

    #[test]
    fn constrained_programs_match_the_builder() {
        for flags in [
            0,
            ACCOUNT_SIGNER,
            ACCOUNT_WRITABLE,
            ACCOUNT_EXECUTABLE,
            ACCOUNT_WRITABLE | ACCOUNT_EXECUTABLE,
        ] {
            for min_data_len in [0, 7, 128, u32::MAX] {
                same(
                    "CONSTRAINED_PLAIN",
                    constrained_program(flags, min_data_len),
                    &built_constrained_program(flags, None, None, min_data_len),
                );
            }
        }
        same(
            "CONSTRAINED_PINNED",
            pinned_program(),
            &built_constrained_program(0, Some(PINNED_ADDRESS), Some(PINNED_OWNER), 0),
        );
    }

    #[test]
    fn transfer_payload_matches_the_shared_fixture() {
        let hex = include_str!("../../../../fixtures/system-transfer.hex").trim();
        let fixture: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        same("TRANSFER_PAYLOAD", &TRANSFER_PAYLOAD, &fixture);
    }

    #[test]
    fn pick_yields_the_first_option_under_the_host_runtime() {
        // The host runtime answers every nondeterministic bool with `false`.
        assert_eq!(pick!(7u8, 8, 9), 7);
    }
}
