//! Regression tests for what the verifier fuzzers in `fuzz/` found. Each test states the behavior
//! the docs promise, fails today, and is ignored until the owner decides on a fix. Run them with
//! `cargo test -p ballista-common --test fuzz_findings -- --ignored`.
//!
//! The findings are about canonical encodings, not safety: every template here runs exactly as
//! its canonical twin would. But a stored template is never verified again, so a non-canonical
//! field accepted at finalization stays accepted for good, and can never take a meaning later.

use ballista_common::template::{
    ProgramBuilder, ProgramView, Segment, TemplateError, VerificationStats, ACCOUNT_EXECUTABLE,
    ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
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

/// `verify_cpi` checks invocation data segments with a loop of its own instead of
/// `verify_segment`, and that loop checks neither field a segment's kind leaves unused. The wire
/// format fixes both ("Source register, or `0xff` for a literal"; "Literal: offset into the blob.
/// Zero otherwise"), and `verify_segment` enforces them for PDA seeds, `EMIT` and
/// `SET_RETURN_DATA`. Found by the `differential` and `structured` fuzz targets, rules
/// `cpi.segment-literal-register` and `cpi.segment-register-fields`.
#[test]
#[ignore = "finding: verify_cpi accepts a literal data segment that names a register"]
fn invocation_data_literals_name_no_register() {
    let (mut builder, _) = transfer(0);
    builder.segments_mut()[0].register = 7;
    assert!(
        verify(&builder).is_err(),
        "a literal segment's source register must be 0xff, as it must be in a seed or an output"
    );
}

#[test]
#[ignore = "finding: verify_cpi accepts a register data segment with a blob offset and length"]
fn invocation_data_register_segments_have_no_offset_or_length() {
    let (mut builder, _) = transfer(0);
    builder.segments_mut()[1].offset_le = 3u16.to_le_bytes();
    builder.segments_mut()[1].len_le = 9u16.to_le_bytes();
    assert!(
        verify(&builder).is_err(),
        "a register segment's offset and length must be zero, as they must be in a seed or an output"
    );
}

/// The same segments, used by an `EMIT` instead, are refused: the two paths disagree.
#[test]
fn output_segments_already_refuse_both() {
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

/// `verify_cpi` reports a bad invocation data segment with `InvalidDataSegment(i)`, where `i`
/// counts from the descriptor's first segment. `verify_segment`, for seeds and outputs, and every
/// other indexed error count from the start of the table, so the context names the wrong segment
/// whenever a descriptor's segments do not start at 0. Here the CPI's segments are 2 and 3, and the
/// error names 1.
#[test]
#[ignore = "finding: verify_cpi reports InvalidDataSegment with an index relative to the descriptor"]
fn invocation_data_segment_errors_name_the_segment() {
    let (mut builder, _) = transfer(2);
    builder.segments_mut()[3].reserved = [1, 0];
    assert_eq!(verify(&builder), Err(TemplateError::InvalidDataSegment(3)));
}

/// "The verifier rejects non-zero reserved bytes in records" (`docs/reference/wire-format.md`,
/// Design rules). A data segment nothing names is never checked, so its reserved bytes, kind and
/// fields can hold anything. Found by the `differential` fuzz target, rule
/// `unreferenced.segment`.
#[test]
#[ignore = "finding: a data segment no CPI, seed or output names is never checked"]
fn unreferenced_segments_follow_the_record_rules() {
    let (mut builder, _) = transfer(0);
    let stray = builder.segments_mut()[0];
    builder.segments_mut().push(stray);
    assert!(
        verify(&builder).is_ok(),
        "an unused, well-formed segment is fine"
    );
    builder.segments_mut()[2].reserved = [0xaa, 0xbb];
    builder.segments_mut()[2].kind = 0xee;
    assert!(
        verify(&builder).is_err(),
        "reserved bytes are zero and the kind is 0 to 9 in every record"
    );
}

/// A CPI descriptor nothing invokes is checked for its shape only (`verify_cpi_shape`): its
/// account records are never resolved or held to the privilege ceiling, though the wire format
/// says a record "May not include a flag that the account's constraint lacks". Such a descriptor
/// can never run, so this is not a privilege escalation; it is a record a reader of the template
/// could take for one. Found by the `differential` fuzz target, rule `unreferenced.cpi-account`.
#[test]
#[ignore = "finding: an uninvoked CPI descriptor's account records are never checked"]
fn uninvoked_descriptors_follow_the_record_rules() {
    let (mut builder, cpi) = transfer(0);
    let reader = builder.account(0, None, None, 0);
    let system = builder.cpis_mut()[cpi as usize].program_account;
    // Never invoked: the read-only account passed as signer and writable, and an account that
    // does not exist.
    builder.cpi(
        system,
        &[
            (reader, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (200, ACCOUNT_SIGNER),
        ],
        &[],
    );
    assert!(
        verify(&builder).is_err(),
        "every CPI account record keeps to its slot's declaration"
    );
}
