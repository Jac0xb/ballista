//! Which errors an instruction the verifier accepted may still raise when it runs.
//!
//! The typing rules, and the host enumeration in `tests/typing_enumeration.rs`, ask one question
//! of every failed execution: could this failure happen to a template that finalization checked?
//! This module is the single answer they share.
//!
//! An error is *value-dependent* when it depends on what a run meets: register values, the
//! accounts and data a transaction passes, or what an invoked program returns. Nothing at
//! finalization can rule those out. An error is *structural* when the verifier's checks exclude it:
//! a structural error from an accepted instruction means the verifier and the executor disagree,
//! which is the property's counterexample.
//!
//! The split assumes what a real run guarantees before its first instruction: the run validated
//! its accounts against the template's declarations (`validate_runtime_accounts`) and decoded its
//! inputs, and every register holds a value of the type the verifier recorded for it.
//!
//! | Code | Error | Class | Why |
//! | --- | --- | --- | --- |
//! | 6000, 6001, 6003-6008, 6010, 6020 | instruction data, template account and lifecycle, inputs, account checks | not raised | Raised before or outside execution, never by an instruction |
//! | 6002 | `InvalidTemplateProgram` | structural | The executor's re-check of a shape the verifier settles: an operand or range out of bounds, an opcode it does not run |
//! | 6009 | `InvalidRuntimeAccount` | value-dependent for a typed account read with a dynamic offset; structural otherwise | The offset is a register value. A fixed offset is bounded by the declared minimum length, which validation enforces; account references and the sysvar pin are settled by the verifier and validation |
//! | 6011 | `InvalidRegister` | structural | An unset or out-of-range register: the verifier tracks initialization and the register count |
//! | 6012 | `TypeMismatch` | value-dependent for a `bool` decoded from bytes; structural otherwise | A byte other than 0 or 1 is not a `bool`. That is a typed account read (`READ_BOOL`), and a return-data, instruction-data or registry-field read whose read opcode is `READ_BOOL` |
//! | 6013, 6014 | `ArithmeticOverflow`, `DivisionByZero` | value-dependent | Operand values |
//! | 6015 | `RequirementFailed` | value-dependent | The condition's value |
//! | 6016 | `CpiDataTooLarge` | structural | The verifier sums each segment's largest encoding, `bytes` included, into the descriptor's maximum |
//! | 6017 | `InvalidPdaDerivation` | value-dependent | Seed values, an off-curve bump, a search that finds none |
//! | 6018, 6019 | `MissingReturnData`, `ReturnDataMismatch` | value-dependent | What the invoked program, or a program it called, set |
//! | 6021 | `CpiAccountLimitExceeded` | value-dependent | A forwarded account group's length is the caller's |
//! | 6022 | `LoopCountExceeded` | value-dependent | The count register's value |
//! | 6023 | `InstructionOutOfRange` | value-dependent | Introspection indexes and byte ranges are register values |
//! | 6024 | `WritableAccountBytesRead` | value-dependent | Whether the transaction passed the account writable |
//! | 6025, 6026 | `InvalidRegistryEntry`, `RegistryReentry` | value-dependent | The entry accounts, and the accounts a CPI forwards, are the transaction's |
//!
//! Errors that are not Ballista's (`RunError::Program`) come from the runtime or from another
//! program, so they are value-dependent for the opcodes that can return one: the clock read, a CPI
//! and a registry open (the callee's error, the rent sysvar), and a typed account read (a failed
//! borrow, when the transaction passed an open registry entry in a second slot). An error with an
//! account index attached (`RunError::VmAt`) is raised while running only by `INVOKE`, for a
//! forwarded group that is too large.
//!
//! `tests/typing_oracle_gap.rs` pins the one case the first version of this split left out, and
//! the host tests below pin the `bool` cases against the executor.

use ballista::error::BallistaError;
use ballista_common::template::*;

/// Whether an accepted `instruction` may fail with `kind` because of the values it meets.
pub fn value_dependent(kind: BallistaError, instruction: &InstructionRecord) -> bool {
    match kind {
        BallistaError::ArithmeticOverflow
        | BallistaError::DivisionByZero
        | BallistaError::RequirementFailed
        | BallistaError::InvalidPdaDerivation
        | BallistaError::MissingReturnData
        | BallistaError::ReturnDataMismatch
        | BallistaError::CpiAccountLimitExceeded
        | BallistaError::LoopCountExceeded
        | BallistaError::InstructionOutOfRange
        | BallistaError::WritableAccountBytesRead
        | BallistaError::InvalidRegistryEntry
        | BallistaError::RegistryReentry => true,
        BallistaError::TypeMismatch => decodes_a_bool(instruction),
        BallistaError::InvalidRuntimeAccount => {
            reads_account_data(instruction.opcode)
                && instruction.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0
        }
        _ => false,
    }
}

/// Whether an accepted `instruction` may fail with an error that is not Ballista's own.
pub fn may_return_program_errors(instruction: &InstructionRecord) -> bool {
    matches!(
        instruction.opcode,
        OP_CLOCK_SLOT | OP_CLOCK_TIMESTAMP | OP_INVOKE | OP_OPEN_REGISTRY
    ) || reads_account_data(instruction.opcode)
}

/// Whether an accepted `instruction` may fail with `kind` and an account index attached.
pub fn may_fail_at_an_index(kind: BallistaError, instruction: &InstructionRecord) -> bool {
    instruction.opcode == OP_INVOKE && kind == BallistaError::CpiAccountLimitExceeded
}

/// Whether `instruction` decodes a `bool` from bytes, the one decode that fails on a byte's value:
/// the executor's `decode_value` refuses anything but 0 and 1 with `TypeMismatch`.
pub fn decodes_a_bool(instruction: &InstructionRecord) -> bool {
    match instruction.opcode {
        OP_READ_BOOL => true,
        // The read opcode is operand `a`.
        OP_RETURN_DATA => instruction.a == OP_READ_BOOL,
        // The read opcode is the immediate.
        OP_READ_INSTRUCTION_DATA => instruction.immediate() == u64::from(OP_READ_BOOL),
        // The read opcode is byte 2 of the immediate, as `registry_instruction` takes it.
        OP_READ_REGISTRY => (instruction.immediate() >> 16) as u8 == OP_READ_BOOL,
        _ => false,
    }
}

/// The typed reads of account data.
pub fn reads_account_data(opcode: u8) -> bool {
    matches!(
        opcode,
        OP_READ_U8
            | OP_READ_U16
            | OP_READ_U32
            | OP_READ_U64
            | OP_READ_I64
            | OP_READ_I32
            | OP_READ_U128
            | OP_READ_PUBKEY
            | OP_READ_BOOL
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ballista::processor::execute::{read_value, RunError};

    /// Every runtime error code is classified: the table above lists each of the 27.
    #[test]
    fn the_split_covers_every_runtime_error() {
        assert_eq!(RUNTIME_ERROR_NAMES.len(), 27);
    }

    /// A `bool` decode of a byte above 1 fails with `TypeMismatch` in the executor, and the split
    /// counts that error as value-dependent for exactly the opcodes that decode one.
    #[test]
    fn bool_decodes_count_type_mismatch_as_value_dependent() {
        assert_eq!(
            read_value(OP_READ_BOOL, &[2], 0),
            Err(RunError::Vm(BallistaError::TypeMismatch))
        );
        let registry_bool = RegistryField { offset: 0, selector: OP_READ_BOOL }.encode();
        let registry_u64 = RegistryField { offset: 0, selector: OP_READ_U64 }.encode();
        for decodes in [
            record(OP_READ_BOOL, 0, 0, NO_INDEX, NO_INDEX, 0, 0),
            record(OP_RETURN_DATA, 0, OP_READ_BOOL, NO_INDEX, NO_INDEX, 0, 0),
            record(OP_READ_INSTRUCTION_DATA, 0, 0, 1, 2, 0, u64::from(OP_READ_BOOL)),
            record(OP_READ_REGISTRY, 0, 0, NO_INDEX, NO_INDEX, 0, registry_bool),
        ] {
            assert!(value_dependent(BallistaError::TypeMismatch, &decodes), "{decodes:?}");
        }
        for other in [
            record(OP_ADD, 0, 1, 2, NO_INDEX, 0, 0),
            record(OP_READ_U64, 0, 0, NO_INDEX, NO_INDEX, 0, 0),
            record(OP_RETURN_DATA, 0, OP_READ_U64, NO_INDEX, NO_INDEX, 0, 0),
            record(OP_READ_REGISTRY, 0, 0, NO_INDEX, NO_INDEX, 0, registry_u64),
        ] {
            assert!(!value_dependent(BallistaError::TypeMismatch, &other), "{other:?}");
        }
    }

    /// A typed account read past the data is a value error only when a register chose the offset.
    #[test]
    fn reads_past_the_data_are_value_errors_only_at_dynamic_offsets() {
        assert_eq!(
            read_value(OP_READ_U64, &[0; 4], 0),
            Err(RunError::Vm(BallistaError::InvalidRuntimeAccount))
        );
        let dynamic = record(OP_READ_U64, 0, 0, 1, NO_INDEX, INSTRUCTION_FLAG_DYNAMIC_OFFSET, 0);
        let fixed = record(OP_READ_U64, 0, 0, NO_INDEX, NO_INDEX, 0, 8);
        assert!(value_dependent(BallistaError::InvalidRuntimeAccount, &dynamic));
        assert!(!value_dependent(BallistaError::InvalidRuntimeAccount, &fixed));
        assert!(!value_dependent(BallistaError::InvalidRegister, &dynamic));
        assert!(!value_dependent(BallistaError::InvalidTemplateProgram, &dynamic));
    }
}
