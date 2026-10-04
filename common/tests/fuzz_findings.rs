//! Regression tests for what the verifier fuzzers in `fuzz/` found. Each test states the behavior
//! the docs promise, and the exact error the verifier now refuses the shape with.
//!
//! The findings are about canonical encodings, not safety: every template here would run exactly
//! as its canonical twin does. But a stored template is never verified again, so a non-canonical
//! field accepted at finalization would stay accepted for good, and could never take a meaning
//! later.

use ballista_common::template::{
    record, LoopScope, ProgramBuilder, ProgramView, Segment, TemplateError, VerificationStats,
    ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, MAX_REGISTERS, NO_INDEX,
    OP_CONST_U64,
};

fn verify(builder: &ProgramBuilder) -> Result<VerificationStats, TemplateError> {
    let bytes = builder.build().expect("builds");
    ProgramView::parse(&bytes)?.verify()
}

/// A system transfer, the same as the `system-transfer.hex` fixture: one CPI whose data is a
/// literal discriminator then a `u64` amount. Returns the builder and the CPI's descriptor. Its
/// two data segments are at indexes 0 and 1 unless `segments_before` pushes others first.
fn transfer(segments_before: usize) -> (ProgramBuilder, u8) {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
    let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let amount = builder.const_u64(5);
    if segments_before > 0 {
        // A log line ahead of the CPI takes the first segment indexes.
        let tag = builder.blob(b"TAG1");
        let mut parts = vec![Segment::Literal(tag)];
        parts.extend((1..segments_before).map(|_| Segment::Register(DATA_REG_U64, amount)));
        builder.emit_data(&parts);
    }
    let discriminator = builder.blob(&[2, 0, 0, 0]);
    let cpi = builder.cpi(
        system,
        &[
            (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(cpi, None);
    assert!(verify(&builder).is_ok(), "the canonical program verifies");
    (builder, cpi)
}

/// `verify_cpi` checked invocation data segments with a loop of its own instead of
/// `verify_segment`, and that loop checked neither field a segment's kind leaves unused. The wire
/// format fixes both ("Source register, or `0xff` for a literal"; "Literal: offset into the blob.
/// Zero otherwise"), as `verify_segment` does for PDA seeds, `EMIT` and `SET_RETURN_DATA`. Found
/// by the `differential` and `structured` fuzz targets, rules `cpi.segment-literal-register` and
/// `cpi.segment-register-fields`.
#[test]
fn invocation_data_literals_name_no_register() {
    let (mut builder, _) = transfer(0);
    builder.segments_mut()[0].register = 7;
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(0)));
}

#[test]
fn invocation_data_register_segments_have_no_offset_or_length() {
    let (mut builder, _) = transfer(0);
    builder.segments_mut()[1].offset_le = 3u16.to_le_bytes();
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(1)));
    let (mut builder, _) = transfer(0);
    builder.segments_mut()[1].len_le = 9u16.to_le_bytes();
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(1)));
}

/// The same segments, used by an `EMIT` instead, are refused with the same error: every use of a
/// segment checks it alike.
#[test]
fn output_segments_refuse_both_alike() {
    let mut builder = ProgramBuilder::new();
    let amount = builder.const_u64(5);
    let tag = builder.blob(b"TAG1");
    builder.emit_data(&[
        Segment::Literal(tag),
        Segment::Register(DATA_REG_U64, amount),
    ]);
    assert!(verify(&builder).is_ok());
    let mut literal = builder.clone();
    literal.segments_mut()[0].register = 7;
    assert_eq!(verify(&literal), Err(TemplateError::InvalidDataSegment(0)));
    let mut register = builder.clone();
    register.segments_mut()[1].offset_le = 3u16.to_le_bytes();
    assert_eq!(verify(&register), Err(TemplateError::InvalidDataSegment(1)));
}

/// `verify_cpi` reported a bad invocation data segment with `InvalidDataSegment(i)`, where `i`
/// counted from the descriptor's first segment, unlike every other indexed error. Here the CPI's
/// segments are 2 and 3, and the error names 3, the segment's index in the table.
#[test]
fn invocation_data_segment_errors_name_the_segment() {
    let (mut builder, _) = transfer(2);
    builder.segments_mut()[3].reserved = [1, 0];
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(3)));
}

/// "The verifier rejects non-zero reserved bytes in records" (`docs/reference/wire-format.md`,
/// Design rules). A data segment nothing named was never checked, so its reserved bytes, kind
/// and fields could hold anything. Now every segment must be part of an invoked CPI's data, an
/// output or a PDA's seeds: one that isn't is refused, well-formed or not. Found by the
/// `differential` fuzz target, rule `unreferenced.segment`.
#[test]
fn unreferenced_segments_are_refused() {
    let (mut builder, _) = transfer(0);
    let stray = builder.segments_mut()[0];
    builder.segments_mut().push(stray);
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(2)));
    builder.segments_mut()[2].reserved = [0xaa, 0xbb];
    builder.segments_mut()[2].kind = 0xee;
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(2)));
}

/// A CPI descriptor nothing invoked was checked for its shape only (`verify_cpi_shape`): its
/// account records were never resolved or held to the privilege ceiling, though the wire format
/// says a record "May not include a flag that the account's constraint lacks". Such a descriptor
/// could never run, so it was never a privilege escalation, but a reader of the template could take
/// it for one. Now every descriptor must be invoked: one that isn't is refused, well-formed or not.
/// Found by the `differential` fuzz target, rule `unreferenced.cpi`.
#[test]
fn uninvoked_descriptors_are_refused() {
    let (mut builder, cpi) = transfer(0);
    let system = builder.cpis_mut()[cpi as usize].program_account;
    // Never invoked, and well-formed: no account, no data.
    builder.cpi(system, &[], &[]);
    assert_eq!(verify(&builder), Err(TemplateError::InvalidCpi(1)));
    // Never invoked, with a read-only account passed as signer and writable, and an account that
    // does not exist.
    let (mut builder, cpi) = transfer(0);
    let reader = builder.account(0, None, None, 0);
    let system = builder.cpis_mut()[cpi as usize].program_account;
    builder.cpi(
        system,
        &[
            (reader, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (200, ACCOUNT_SIGNER),
        ],
        &[],
    );
    assert_eq!(verify(&builder), Err(TemplateError::InvalidCpi(1)));
}

/// `verify_single_instruction`, public for the Certora specs, skips the header checks `verify`
/// makes first, so a view that declared more than `MAX_REGISTERS` registers indexed past the
/// 64-entry typing table and panicked where `verify` returns `TooManyRegisters`. It now returns
/// the same error. Finalization always calls `verify`, so no upload reached this; the specs parse
/// a fixed 4-register program. Found while reviewing the harness's entry points.
#[test]
fn verifying_one_instruction_never_panics() {
    let mut builder = ProgramBuilder::new();
    builder.const_u64(1);
    let mut bytes = builder.build().expect("builds");
    bytes[9] = 200; // the header's register count
    let program = ProgramView::parse(&bytes).expect("parses");
    assert_eq!(program.verify(), Err(TemplateError::TooManyRegisters));
    let mut typing = [None; MAX_REGISTERS];
    let constant = record(OP_CONST_U64, 100, NO_INDEX, NO_INDEX, NO_INDEX, 0, 7);
    let verdict = std::panic::catch_unwind(move || {
        program.verify_single_instruction(&constant, 0, LoopScope::Root, None, &mut typing)
    });
    assert_eq!(
        verdict.expect("an error, not a panic"),
        Err(TemplateError::TooManyRegisters)
    );
}
