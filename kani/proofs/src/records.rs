//! Instruction-record decoding: the immediates the executor unpacks, the registry immediates, and
//! the typed reads `read_value` performs for account, registry, return-data and instruction-data
//! reads, against the widths and result types the verifier assigns (`read_width`, `read_type`).

use ballista::error::BallistaError;
use ballista::processor::execute::{read_array, read_value, RunError, RuntimeValue};
use ballista_common::template::*;

fn err(kind: BallistaError) -> RunError {
    RunError::Vm(kind)
}

/// `immediate` is the little-endian `u64` of the record's eight immediate bytes, and `blob_range`
/// splits it into `(low 32 bits, high 32 bits)`, the inverse of the builder's `range_immediate` for
/// every `u16` pair. Bound: none (every record, every pair).
#[kani::proof]
#[kani::unwind(9)]
fn immediates_decode_little_endian_and_ranges_round_trip() {
    let record = crate::util::any::instruction();
    let mut expected = 0u64;
    for j in 0..8 {
        expected |= (record.immediate_le[j] as u64) << (8 * j);
    }
    assert_eq!(record.immediate(), expected);
    let (start, len) = record.blob_range();
    assert_eq!(start as u64 | (len as u64) << 32, expected);
    assert!(start <= u32::MAX as usize && len <= u32::MAX as usize);
    kani::cover!(start > u16::MAX as usize, "a range start past 16 bits");

    let (start, len): (u16, u16) = (kani::any(), kani::any());
    let packed = ballista_common::template::record(0, 0, 0, 0, 0, 0, range_immediate(start, len));
    assert_eq!(packed.blob_range(), (start as usize, len as usize));
}

/// `RegistryOpen` and `RegistryField` round-trip, and decoding refuses every immediate with a bit
/// set past its fields, so two different immediates never decode to the same open or field. The
/// executor's own unpacking of a field immediate (`immediate as u16` for the offset, byte 2 for the
/// selector) agrees with `RegistryField::decode` on every immediate the decoder accepts. Bound:
/// none (every immediate, every field value).
#[kani::proof]
fn registry_immediates_round_trip_and_refuse_spare_bits() {
    let immediate: u64 = kani::any();
    match RegistryOpen::decode(immediate) {
        Some(open) => {
            assert_eq!(immediate >> 32, 0);
            assert_eq!(open.encode(), immediate);
            kani::cover!(open.size > 255, "an open with a two-byte size");
        }
        None => {
            assert!(immediate >> 32 != 0);
            kani::cover!(immediate >> 32 == 1, "one spare bit refuses an open");
        }
    }
    let open = RegistryOpen { index: kani::any(), size: kani::any(), system_program: kani::any() };
    assert_eq!(RegistryOpen::decode(open.encode()), Some(open));

    match RegistryField::decode(immediate) {
        Some(field) => {
            assert_eq!(immediate >> 24, 0);
            assert_eq!(field.encode(), immediate);
            // `registry_instruction` reads the field as `immediate as u16` and
            // `(immediate >> 16) as u8`.
            assert_eq!(immediate as u16, field.offset);
            assert_eq!((immediate >> 16) as u8, field.selector);
            kani::cover!(field.selector == OP_READ_U128, "a u128 field");
        }
        None => assert!(immediate >> 24 != 0),
    }
    let field = RegistryField { offset: kani::any(), selector: kani::any() };
    assert_eq!(RegistryField::decode(field.encode()), Some(field));
}

/// The fixed-width accessors of the other records are the little-endian values of their bytes,
/// and a CPI's account group is `None` exactly for `NO_INDEX`. Bound: none (every record).
#[kani::proof]
fn record_accessors_are_little_endian() {
    let descriptor = crate::util::any::cpi_descriptor();
    let le16 = |bytes: [u8; 2]| bytes[0] as usize | (bytes[1] as usize) << 8;
    assert_eq!(descriptor.account_start(), le16(descriptor.account_start_le));
    assert_eq!(descriptor.segment_start(), le16(descriptor.segment_start_le));
    assert_eq!(descriptor.max_data_len(), le16(descriptor.max_data_len_le));
    assert_eq!(descriptor.account_group().is_none(), descriptor.account_group == NO_INDEX);
    if let Some(group) = descriptor.account_group() {
        assert_eq!(group, descriptor.account_group as usize);
        kani::cover!(group == 254, "the largest group index");
    }
    let segment = crate::util::any::segment();
    assert_eq!(segment.offset(), le16(segment.offset_le));
    assert_eq!(segment.len(), le16(segment.len_le));
    let input = crate::util::any::input();
    assert_eq!(input.max_len(), le16(input.max_len_le));
    let constraint = crate::util::any::constraint();
    let bytes = constraint.min_data_len_le;
    assert_eq!(constraint.min_data_len(), le16([bytes[0], bytes[1]]) | le16([bytes[2], bytes[3]]) << 16);
    kani::cover!(constraint.min_data_len() > u16::MAX as usize, "a minimum length past 16 bits");
}

/// Data bytes the typed-read proofs explore.
const DATA: usize = 40;

/// `read_value` agrees with the verifier's view of every read opcode. For an opcode whose
/// `read_width` is zero it fails with `InvalidTemplateProgram`. For a read opcode it succeeds
/// exactly when `offset + width` fits the data (and, for `bool`, the byte is 0 or 1), returns a
/// value of `read_type(opcode)`, and that value is the little-endian decoding of exactly those
/// bytes: zero-extended for `u8`/`u16`/`u32`, sign-extended for `i32`. Out of range is
/// `InvalidRuntimeAccount`, a bad bool byte `TypeMismatch`. Bound: data up to 40 bytes; every
/// opcode byte and every `usize` offset.
#[kani::proof]
#[kani::unwind(33)]
fn typed_reads_match_the_verifiers_widths_and_types() {
    let bytes: [u8; DATA] = kani::any();
    let len: usize = kani::any_where(|len: &usize| *len <= DATA);
    let data = &bytes[..len];
    let opcode: u8 = kani::any();
    let offset: usize = kani::any();
    let result = read_value(opcode, data, offset);

    let width = read_width(opcode);
    let fits = offset <= len && len - offset >= width;
    if width == 0 {
        assert_eq!(result, Err(err(BallistaError::InvalidTemplateProgram)));
    } else if !fits {
        assert_eq!(result, Err(err(BallistaError::InvalidRuntimeAccount)));
        kani::cover!(offset == len - width.min(len) + 1, "one byte past the end");
    } else if opcode == OP_READ_BOOL && data[offset] > 1 {
        assert_eq!(result, Err(err(BallistaError::TypeMismatch)));
    } else {
        // The bytes read, as an unsigned little-endian number, assembled byte by byte.
        let mut raw: u128 = 0;
        for j in 0..width.min(16) {
            raw |= (data[offset + j] as u128) << (8 * j);
        }
        let value = result.expect("a read that fits succeeds");
        let value_type = match value {
            RuntimeValue::Bool(flag) => {
                assert_eq!(flag, data[offset] == 1);
                VALUE_BOOL
            }
            RuntimeValue::U64(v) => {
                assert_eq!(v as u128, raw);
                VALUE_U64
            }
            RuntimeValue::I64(v) => {
                // Sign-extend from the read's width: the top bit of the last byte read decides.
                let bits = 8 * width as u32;
                let negative = raw >> (bits - 1) & 1 == 1;
                let expected = if negative { raw as i128 - (1i128 << bits) } else { raw as i128 };
                assert_eq!(v as i128, expected);
                kani::cover!(width == 4 && v < 0, "a negative i32 read");
                VALUE_I64
            }
            RuntimeValue::U128(v) => {
                assert_eq!(u128::from_le_bytes(v), raw);
                VALUE_U128
            }
            RuntimeValue::Pubkey(key) => {
                for j in 0..32 {
                    assert_eq!(key[j], data[offset + j]);
                }
                kani::cover!(offset + 32 == len, "a pubkey ending at the end of the data");
                VALUE_PUBKEY
            }
            // A read never produces an unset or bytes value; this compares unequal below.
            _ => 0,
        };
        assert_eq!(value_type, read_type(opcode));
    }
}

/// `READ_I32` in particular: every 4-byte pattern reads as the `i64` with the same value as the
/// pattern's `i32`, so a negative value stays negative and nothing above bit 31 is lost or
/// invented. Bound: none (all four bytes).
#[kani::proof]
fn read_i32_sign_extends() {
    let bytes: [u8; 4] = kani::any();
    let unsigned = bytes[0] as i64 | (bytes[1] as i64) << 8 | (bytes[2] as i64) << 16 | (bytes[3] as i64) << 24;
    let expected = if bytes[3] & 0x80 != 0 { unsigned - (1 << 32) } else { unsigned };
    assert_eq!(read_value(OP_READ_I32, &bytes, 0), Ok(RuntimeValue::I64(expected)));
    assert!((i32::MIN as i64..=i32::MAX as i64).contains(&expected));
    kani::cover!(expected == i32::MIN as i64, "i32::MIN");
}

/// `read_array` returns exactly `data[offset..offset + N]` when that range is inside `data`, and
/// `InvalidRuntimeAccount` for every other offset, including those where `offset + N` overflows.
/// Bound: data up to 40 bytes, `N` = 8; every `usize` offset.
#[kani::proof]
fn read_array_is_bounds_checked() {
    let bytes: [u8; DATA] = kani::any();
    let len: usize = kani::any_where(|len: &usize| *len <= DATA);
    let data = &bytes[..len];
    let offset: usize = kani::any();
    let fits = offset <= len && len - offset >= 8;
    let result = read_array::<8>(data, offset);
    assert_eq!(result.is_ok(), fits);
    match result {
        Ok(array) => {
            assert!(core::ptr::eq(array.as_ptr(), data.as_ptr().wrapping_add(offset)));
            kani::cover!(offset + 8 == len, "the last eight bytes");
        }
        Err(error) => {
            assert_eq!(error, err(BallistaError::InvalidRuntimeAccount));
            kani::cover!(offset > usize::MAX - 8, "an offset whose end overflows");
        }
    }
}
