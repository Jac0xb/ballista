//! Byte-level paths of the executor: encoding a register into CPI data, an output or a PDA seed;
//! the fixed-size seed buffer; decoding run inputs; and lending account bytes to a register.

use ballista::error::BallistaError;
use ballista::processor::execute::{
    encode_register_segment, parse_inputs, parse_run_inputs, ByteSink, FixedSink, RunError,
    RuntimeValue,
};
use ballista::processor::introspect::read_account_bytes;
use ballista_common::template::*;

use crate::accounts::{AccountMemory, Fields};
use crate::util::{any, any_runtime_value, one_of, same_value};

fn err(kind: BallistaError) -> RunError {
    RunError::Vm(kind)
}

/// Little-endian bytes of `value`, `width` of them, by shifts.
fn le(value: u128, width: usize) -> [u8; 16] {
    let mut out = [0u8; 16];
    for j in 0..width {
        out[j] = (value >> (8 * j)) as u8;
    }
    out
}

/// Encoding a register-backed data segment (CPI data, `EMIT`/`SET_RETURN_DATA` output, PDA seeds)
/// either appends exactly the bytes of the value at the segment's width or appends nothing and
/// fails. The narrowing kinds (`u8`, `u16`, `u32`, `u64` from a `u64` or `u128` register) fail with
/// `ArithmeticOverflow` exactly when the value does not fit, never truncating; every fixed width
/// is the builder's `segment_width`, which the verifier's bound on a descriptor's data length also
/// uses; a `bytes` register appends exactly its bytes. A missing or unset register is
/// `InvalidRegister`, a wrong type `TypeMismatch`, an unknown kind `InvalidTemplateProgram`. Bound:
/// none over the kind, the register index and its value, except `bytes` values up to 4 bytes.
#[kani::proof]
#[kani::unwind(33)]
fn register_segments_encode_their_exact_width_or_fail() {
    let bytes: [u8; 4] = kani::any();
    let registers = [any_runtime_value(&bytes), any_runtime_value(&bytes)];
    let segment = any::segment();
    let mut sink: Vec<u8> = Vec::new();
    let result = encode_register_segment(&registers, &segment, &mut sink);
    let register = registers.get(segment.register as usize);
    let present = !matches!(register, None | Some(RuntimeValue::Unset));

    // The expected bytes and width, or the expected error.
    let narrow = |width: usize| -> Result<([u8; 16], usize), BallistaError> {
        let value = match register {
            Some(RuntimeValue::U64(value)) => *value as u128,
            Some(RuntimeValue::U128(value)) => u128::from_le_bytes(*value),
            _ => return Err(BallistaError::TypeMismatch),
        };
        if width < 16 && value >> (8 * width) != 0 {
            return Err(BallistaError::ArithmeticOverflow);
        }
        Ok((le(value, width), width))
    };
    let expected: Option<Result<([u8; 16], usize), BallistaError>> = match segment.kind {
        DATA_REG_U8 | DATA_REG_U16 | DATA_REG_U32 | DATA_REG_U64 | DATA_REG_I64 | DATA_REG_U128
        | DATA_REG_BOOL | DATA_REG_PUBKEY | DATA_REG_BYTES
            if !present =>
        {
            Some(Err(BallistaError::InvalidRegister))
        }
        DATA_REG_U8 => Some(narrow(1)),
        DATA_REG_U16 => Some(narrow(2)),
        DATA_REG_U32 => Some(narrow(4)),
        DATA_REG_U64 => Some(narrow(8)),
        DATA_REG_I64 => Some(match register {
            Some(RuntimeValue::I64(v)) => Ok((le(*v as u64 as u128, 8), 8)),
            _ => Err(BallistaError::TypeMismatch),
        }),
        DATA_REG_U128 => Some(match register {
            Some(RuntimeValue::U128(v)) => Ok((*v, 16)),
            _ => Err(BallistaError::TypeMismatch),
        }),
        DATA_REG_BOOL => Some(match register {
            Some(RuntimeValue::Bool(v)) => Ok((le(*v as u128, 1), 1)),
            _ => Err(BallistaError::TypeMismatch),
        }),
        // Wider than the 16-byte buffer: checked separately below.
        DATA_REG_PUBKEY | DATA_REG_BYTES => None,
        _ => Some(Err(BallistaError::InvalidTemplateProgram)),
    };
    match expected {
        Some(Ok((encoded, width))) => {
            assert_eq!(result, Ok(()));
            assert_eq!(sink.len(), width);
            assert_eq!(width, segment_width(segment.kind) as usize);
            for j in 0..width {
                assert_eq!(sink[j], encoded[j]);
            }
            kani::cover!(segment.kind == DATA_REG_U8 && encoded[0] == 255, "255 fits a u8 segment");
            kani::cover!(
                segment.kind == DATA_REG_U64 && matches!(register, Some(RuntimeValue::U128(_))),
                "a u128 register narrowed to u64"
            );
        }
        Some(Err(kind)) => {
            assert_eq!(result, Err(err(kind)));
            assert!(sink.is_empty());
            kani::cover!(kind == BallistaError::ArithmeticOverflow && segment.kind == DATA_REG_U8, "a value too wide for a u8 segment");
        }
        None => match (segment.kind, register) {
            (DATA_REG_PUBKEY, Some(RuntimeValue::Pubkey(key))) => {
                assert_eq!(result, Ok(()));
                assert_eq!(sink.len(), 32);
                assert_eq!(segment_width(DATA_REG_PUBKEY), 32);
                for j in 0..32 {
                    assert_eq!(sink[j], key[j]);
                }
            }
            (DATA_REG_BYTES, Some(RuntimeValue::Bytes(value))) => {
                assert_eq!(result, Ok(()));
                assert_eq!(sink.len(), value.len());
                for j in 0..value.len() {
                    assert_eq!(sink[j], value[j]);
                }
                kani::cover!(value.len() == 4, "a 4-byte bytes segment");
            }
            _ => {
                assert_eq!(result, Err(err(BallistaError::TypeMismatch)));
                assert!(sink.is_empty());
            }
        },
    }
}

/// A PDA seed buffer (`FixedSink`) never writes past its end: a push that fits copies exactly its
/// bytes after the ones already there and advances the length; any other push, including one whose
/// end overflows `usize`, fails with `InvalidPdaDerivation` and changes nothing. Bound: an 8-byte
/// buffer and pushes up to 8 bytes; the starting length is any `usize`.
#[kani::proof]
#[kani::unwind(9)]
fn seed_buffers_never_write_past_their_end() {
    let original: [u8; 8] = kani::any();
    let mut buffer = original;
    let start: usize = kani::any();
    let chunk: [u8; 8] = kani::any();
    let len: usize = kani::any_where(|len: &usize| *len <= 8);
    let mut sink = FixedSink::new(&mut buffer);
    sink.len = start;
    let result = sink.push_bytes(&chunk[..len]);
    let end = sink.len;
    let fits = start <= 8 && 8 - start >= len;
    if fits {
        assert_eq!(result, Ok(()));
        assert_eq!(end, start + len);
        kani::cover!(end == 8 && len > 0, "a push that fills the buffer");
    } else {
        assert_eq!(result, Err(err(BallistaError::InvalidPdaDerivation)));
        assert_eq!(end, start);
        kani::cover!(start == usize::MAX && len > 0, "a push whose end overflows usize");
    }
    for i in 0..8 {
        let written = fits && i >= start && i - start < len;
        assert_eq!(buffer[i], if written { chunk[i - start] } else { original[i] });
    }
}

/// One run input decoded by the documented encoding (docs/reference/language.md, inputs): `u64`
/// and `i64` little-endian, 8 bytes; `u128` 16; `pubkey` 32; `bool` one byte, 0 or 1; `bytes` a
/// little-endian `u16` length of at most the declared maximum, then that many bytes. `None` when
/// `data` does not start with one.
fn decode_one<'a>(descriptor: &InputDescriptor, data: &'a [u8]) -> Option<(RuntimeValue<'a>, &'a [u8])> {
    let take = |n: usize| (data.len() >= n).then(|| data.split_at(n));
    match descriptor.value_type {
        VALUE_U64 => take(8).map(|(v, rest)| (RuntimeValue::U64(u64::from_le_bytes(v.try_into().unwrap())), rest)),
        VALUE_I64 => take(8).map(|(v, rest)| (RuntimeValue::I64(i64::from_le_bytes(v.try_into().unwrap())), rest)),
        VALUE_U128 => take(16).map(|(v, rest)| (RuntimeValue::U128(v.try_into().unwrap()), rest)),
        VALUE_PUBKEY => take(32).map(|(v, rest)| (RuntimeValue::Pubkey(v.try_into().unwrap()), rest)),
        VALUE_BOOL => match take(1) {
            Some(([0], rest)) => Some((RuntimeValue::Bool(false), rest)),
            Some(([1], rest)) => Some((RuntimeValue::Bool(true), rest)),
            _ => None,
        },
        VALUE_BYTES => {
            let (prefix, rest) = take(2)?;
            let len = u16::from_le_bytes(prefix.try_into().unwrap()) as usize;
            if len > descriptor.max_len() || rest.len() < len {
                return None;
            }
            let (value, rest) = rest.split_at(len);
            Some((RuntimeValue::Bytes(value), rest))
        }
        _ => None,
    }
}

/// An input descriptor of one of the six types, or an invalid one (type 0), with a `bytes`
/// maximum up to 4.
fn any_input() -> InputDescriptor {
    let value_type = one_of(&[0, VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY, VALUE_BYTES]);
    let max_len: u16 = kani::any_where(|n: &u16| *n <= 4);
    InputDescriptor { value_type, reserved: 0, max_len_le: max_len.to_le_bytes() }
}

/// Bytes of run data in the input proofs.
const INPUT_DATA: usize = 12;

/// `parse_inputs` decodes exactly what the documented encoding says (`decode_one`, written
/// independently): value `i` with descriptor `i`, in order, each `bytes` value borrowing the run
/// data in place; it fails at the first value that is not there with `InvalidRunInputs` and that
/// value's index, and rejects trailing bytes with the count as the index. Bound: up to 2
/// descriptors (each type, or an invalid one; `bytes` maximum up to 4) and up to 12 bytes of data,
/// so a `u128` or `pubkey` input is only ever seen truncated.
#[kani::proof]
#[kani::unwind(5)]
fn input_decoding_follows_the_documented_encoding() {
    let descriptors = [any_input(), any_input()];
    let count: usize = kani::any_where(|n: &usize| *n <= 2);
    let descriptors = &descriptors[..count];
    let bytes: [u8; INPUT_DATA] = kani::any();
    let len: usize = kani::any_where(|n: &usize| *n <= INPUT_DATA);
    let data = &bytes[..len];

    let result = parse_inputs(descriptors, data);

    let mut rest = data;
    let mut expected = [RuntimeValue::Unset; 2];
    let mut failure = None;
    for (index, descriptor) in descriptors.iter().enumerate() {
        match decode_one(descriptor, rest) {
            Some((value, remaining)) => {
                expected[index] = value;
                rest = remaining;
            }
            None => {
                failure = Some(index);
                break;
            }
        }
    }
    match failure {
        Some(index) => {
            assert_eq!(result, Err(RunError::VmAt(BallistaError::InvalidRunInputs, index as u16)));
            kani::cover!(index == 1, "the second input is malformed");
        }
        None if !rest.is_empty() => {
            assert_eq!(result, Err(RunError::VmAt(BallistaError::InvalidRunInputs, count as u16)));
            kani::cover!(count == 2, "trailing bytes after two inputs");
        }
        None => {
            let values = result.expect("decoding failed on valid data");
            assert_eq!(values.len(), count);
            for i in 0..count {
                assert!(same_value(&values[i], &expected[i]));
            }
            kani::cover!(
                count == 2 && matches!(values[1], RuntimeValue::Bytes(value) if value.len() == 2),
                "a value then two bytes"
            );
        }
    }
}

/// `parse_run_inputs` lays a run's inputs out as the fixed inputs followed by one copy of the row
/// inputs per batch row: it decodes exactly as `parse_inputs` does over that flattened descriptor
/// list. So value `F + n·R + o`, the index `LOAD_INPUT` computes for row input `o` in pass `n`, is
/// row `n`'s `o`th input, and every such index is in range. Bound: up to 2 fixed inputs and 1 row
/// input (types as in `input_decoding_follows_the_documented_encoding`), up to 2 rows, up to 12
/// bytes of data.
#[kani::proof]
#[kani::unwind(5)]
fn run_inputs_are_the_fixed_inputs_then_one_row_per_iteration() {
    let fixed: u8 = kani::any_where(|n: &u8| *n <= 2);
    let row: u8 = kani::any_where(|n: &u8| *n <= 1);
    let iterations: usize = kani::any_where(|n: &usize| *n <= 2);
    let header = ProgramHeader::new(0, 1, 2, 0, fixed, 0, 0, 0, 0, 0, 0, 0, 0, row, 0);
    let table = [any_input(), any_input(), any_input()];
    let (fixed, row) = (fixed as usize, row as usize);
    let program = ProgramView {
        header: &header,
        accounts: &[],
        inputs: &table[..fixed + row],
        instructions: &[],
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &[],
        pubkeys: &[],
        blob: &[],
    };
    let flat: [InputDescriptor; 4] = core::array::from_fn(|i| if i < fixed { table[i] } else { table[fixed] });
    let bytes: [u8; INPUT_DATA] = kani::any();
    let len: usize = kani::any_where(|n: &usize| *n <= INPUT_DATA);
    let data = &bytes[..len];

    let run = parse_run_inputs(&program, data, iterations);
    let flattened = parse_inputs(&flat[..fixed + iterations * row], data);
    assert_eq!(run.is_ok(), flattened.is_ok());
    if let (Ok(values), Ok(expected)) = (&run, &flattened) {
        assert_eq!(values.len(), expected.len());
        for i in 0..values.len() {
            assert!(same_value(&values[i], &expected[i]));
        }
        for pass in 0..iterations {
            for offset in 0..row {
                assert!(header.input_count() + pass * header.row_input_count() + offset < values.len());
            }
        }
        kani::cover!(iterations == 2 && row == 1 && fixed == 1, "a fixed input and two rows");
    }
    if let (Err(a), Err(b)) = (&run, &flattened) {
        assert_eq!(a, b);
    }
}

/// `READ_ACCOUNT_BYTES` lends exactly `immediate` bytes of the account's data from the offset in
/// register `b`, in place, and only from an account this instruction cannot write: the register
/// ends up holding the slice `data[offset..offset + len]` itself. Otherwise it fails and changes no
/// register: `InvalidRegister`/`TypeMismatch` for a bad offset register, `WritableAccountBytesRead`
/// for a writable account, `InstructionOutOfRange` for a range past the data (an end past `usize`
/// included), `InvalidRegister` for a destination past the file. Bound: data up to 16 bytes; every
/// offset value, length, operand and flag; registers of any variant (`bytes` up to 2 bytes).
#[kani::proof]
#[kani::unwind(5)]
fn account_byte_reads_lend_exactly_the_range() {
    let mut fields = Fields::plain(kani::any(), kani::any());
    fields.writable = kani::any();
    let data: [u8; 16] = kani::any();
    let data_len: usize = kani::any_where(|n: &usize| *n <= 16);
    let mut memory = AccountMemory::new(fields, data, data_len);
    let account = memory.view();
    let bytes: [u8; 2] = kani::any();
    let mut registers = [any_runtime_value(&bytes), any_runtime_value(&bytes), any_runtime_value(&bytes)];
    let before = registers;
    let length: u64 = kani::any();
    let instruction = record(OP_READ_ACCOUNT_BYTES, kani::any(), 0, kani::any(), NO_INDEX, 0, length);

    let result = read_account_bytes(&account, &mut registers, &instruction);

    let expected: Result<u64, BallistaError> = match before.get(instruction.b as usize) {
        None | Some(RuntimeValue::Unset) => Err(BallistaError::InvalidRegister),
        Some(RuntimeValue::U64(offset)) => {
            let in_range = *offset <= data_len as u64 && data_len as u64 - offset >= length;
            if fields.writable {
                Err(BallistaError::WritableAccountBytesRead)
            } else if !in_range {
                Err(BallistaError::InstructionOutOfRange)
            } else if instruction.dst as usize >= registers.len() {
                Err(BallistaError::InvalidRegister)
            } else {
                Ok(*offset)
            }
        }
        Some(_) => Err(BallistaError::TypeMismatch),
    };
    match expected {
        Ok(offset) => {
            assert_eq!(result, Ok(()));
            let lent = match registers[instruction.dst as usize] {
                RuntimeValue::Bytes(lent) => Some(lent),
                _ => None,
            };
            let lent = lent.expect("a bytes value");
            assert_eq!(lent.len() as u64, length);
            assert!(core::ptr::eq(lent.as_ptr(), memory.data[offset as usize..].as_ptr()));
            kani::cover!(length > 0 && offset > 0, "a read from inside the data");
        }
        Err(kind) => {
            assert_eq!(result, Err(err(kind)));
            kani::cover!(kind == BallistaError::WritableAccountBytesRead, "a writable account is refused");
            kani::cover!(kind == BallistaError::InstructionOutOfRange, "a range past the data");
        }
    }
    for register in 0..registers.len() {
        if result.is_err() || register != instruction.dst as usize {
            assert!(same_value(&registers[register], &before[register]));
        }
    }
}
