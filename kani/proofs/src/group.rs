//! `GROUP_ANY` and `GROUP_COUNT`'s filter, `GroupScan`: its immediate's encoding, the segment
//! range it names, and the verifier's checks of it.
//!
//! Bounds, per harness:
//! - `group_scan_round_trips`, `group_scan_fields_round_trip`, `group_segment_range_never_overflows`:
//!   none. Every `u64` immediate and every field value.
//! - `group_verify_filter_never_panics`: one `GROUP_ANY` or `GROUP_COUNT` record, every field
//!   symbolic; a program of `SEGMENTS` data segments (every byte symbolic), two pubkeys, 0 to
//!   `MAX_ACCOUNT_GROUPS` declared groups and the first `REGISTERS` registers typed any way (the
//!   rest unset). The immediate is symbolic too, so a segment start anywhere in `u16` and any
//!   counts reach the range check; at most 8 segments get past it, which `SEGMENTS` covers.
//!
//! Run: `cargo kani -p ballista-kani --harness group_` from `kani/`. Measured on an M-series Mac
//! under load: the first three under a second each; `group_verify_filter_never_panics` 31 minutes
//! (18 of them symbolic execution, 6.2 million steps).

use ballista_common::template::*;

use crate::util::{any, one_of};

/// Data segments in the program: 4 matches and 4 excepts, plus two so a start past zero fits.
const SEGMENTS: usize = 10;

/// Typed registers; the verifier's table has `MAX_REGISTERS`, the rest unset.
const REGISTERS: usize = 6;

fn any_scan() -> GroupScan {
    GroupScan {
        segment_start: kani::any(),
        matches: kani::any(),
        excepts: kani::any(),
        min_data_len: kani::any(),
    }
}

/// Every bit of the immediate is a field: decoding then encoding gives the immediate back.
#[kani::proof]
fn group_scan_round_trips() {
    let immediate: u64 = kani::any();
    let scan = GroupScan::decode(immediate);
    assert_eq!(scan.encode(), immediate);
    kani::cover!(scan.matches == 4 && scan.excepts == 4 && scan.min_data_len == u32::MAX);
    kani::cover!(immediate == u64::MAX);
}

/// Encoding then decoding gives the fields back: no field overlaps another.
#[kani::proof]
fn group_scan_fields_round_trip() {
    let scan = any_scan();
    assert_eq!(GroupScan::decode(scan.encode()), scan);
    kani::cover!(scan.segment_start == u16::MAX && scan.min_data_len == u32::MAX);
}

/// The range never overflows, even with a 32-bit `usize`: it ends at most `u16::MAX + 510`, and
/// it holds exactly the matches and the excepts.
#[kani::proof]
fn group_segment_range_never_overflows() {
    let scan = GroupScan::decode(kani::any());
    let (start, end) = scan.segment_range();
    assert_eq!(start, usize::from(scan.segment_start));
    assert!(end >= start);
    assert_eq!(
        end - start,
        usize::from(scan.matches) + usize::from(scan.excepts)
    );
    assert!(end <= usize::from(u16::MAX) + 2 * usize::from(u8::MAX));
    assert!(end as u64 <= u32::MAX as u64);
    kani::cover!(end == usize::from(u16::MAX) + 510);
}

fn any_info() -> Option<RegisterInfo> {
    match one_of(&[
        0u8,
        VALUE_BOOL,
        VALUE_U64,
        VALUE_I64,
        VALUE_U128,
        VALUE_PUBKEY,
        VALUE_BYTES,
    ]) {
        0 => None,
        VALUE_BYTES => Some(RegisterInfo::bytes(kani::any_where(|n: &usize| *n <= 32))),
        scalar => Some(RegisterInfo::scalar(scalar)),
    }
}

/// Within the bound above, checking any `GROUP_ANY` or `GROUP_COUNT` record never panics,
/// overflows or reads out of bounds, whatever its operands, immediate and segments hold. When it
/// accepts the record, the filter holds 1 to 4 matches and at most 4 excepts, its segments are in
/// the program, and its floor covers every match's bytes.
#[kani::proof]
#[kani::unwind(11)]
fn group_verify_filter_never_panics() {
    let groups: u8 = kani::any_where(|groups: &u8| usize::from(*groups) <= MAX_ACCOUNT_GROUPS);
    let header = ProgramHeader::new(
        0,
        1,
        0,
        0,
        0,
        REGISTERS as u8,
        1,
        0,
        0,
        SEGMENTS as u16,
        2,
        0,
        0,
        0,
        groups,
    );
    let segments: [DataSegment; SEGMENTS] = core::array::from_fn(|_| any::segment());
    let pubkeys = [
        PubkeyRecord { bytes: kani::any() },
        PubkeyRecord { bytes: kani::any() },
    ];
    let mut instruction = any::instruction();
    instruction.opcode = one_of(&[OP_GROUP_ANY, OP_GROUP_COUNT]);
    let program = ProgramView {
        header: &header,
        accounts: &[],
        inputs: &[],
        instructions: core::slice::from_ref(&instruction),
        cpis: &[],
        cpi_accounts: &[],
        data_segments: &segments,
        pubkeys: &pubkeys,
        blob: &[],
    };
    let mut typing: [Option<RegisterInfo>; MAX_REGISTERS] = [None; MAX_REGISTERS];
    for register in typing.iter_mut().take(REGISTERS) {
        *register = any_info();
    }
    let scope = one_of(&[LoopScope::Root, LoopScope::Rows, LoopScope::Count]);
    let outcome = program.verify_single_instruction(&instruction, 0, scope, None, &mut typing);

    let scan = GroupScan::decode(instruction.immediate());
    if outcome.is_ok() {
        let (start, end) = scan.segment_range();
        assert!((1..=MAX_GROUP_MATCHES).contains(&usize::from(scan.matches)));
        assert!(usize::from(scan.excepts) <= MAX_GROUP_EXCEPTS);
        assert!(end <= SEGMENTS);
        for segment in &segments[start..start + usize::from(scan.matches)] {
            let width = match segment.kind {
                DATA_REG_BOOL => 1,
                DATA_REG_U64 | DATA_REG_I64 => 8,
                DATA_REG_U128 => 16,
                _ => 32,
            };
            assert!(segment.offset() + width <= scan.min_data_len as usize);
        }
    }
    kani::cover!(outcome.is_ok() && scan.matches == 4 && scan.excepts == 4);
    kani::cover!(outcome.is_ok() && scan.segment_start > 0);
    kani::cover!(outcome == Err(TemplateError::InvalidAccountGroup(0)));
    kani::cover!(outcome == Err(TemplateError::TypeMismatch));
    kani::cover!(matches!(
        outcome,
        Err(TemplateError::RegisterNotInitialized(_))
    ));
}
