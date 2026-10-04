//! Confirmation tests for findings from the safety-property review
//! (`docs/superpowers/specs/2026-10-03-safety-properties.md`). Each test names the property it
//! checks, and pins a fact that a check elsewhere states wrongly or that the documentation leaves
//! out, or, for a closed finding, the fix.

use ballista_common::template::{
    InstructionRecord, ProgramBuilder, ProgramView, TemplateError, ACCOUNT_SIGNER,
    ACCOUNT_WRITABLE, OP_FOREACH, OP_REQUIRE, PROGRAM_HEADER_LEN, VALUE_U64,
};

/// P82, finding F1. `ProgramView::parse` reads `input_count + row_input_count` input descriptors.
/// The Certora rule `rule_parsed_sections_exactly_consume_the_payload` (in `run.conf`) asserts
/// `program.inputs.len() == header.input_count()`, so this 28-byte payload, well inside the rule's
/// 96-byte bound, is a counterexample to it: the rule cannot prove as written. The fix is to assert
/// against `header.total_input_count()`.
#[test]
fn a_row_input_descriptor_is_a_counterexample_to_the_parser_rule() {
    let mut builder = ProgramBuilder::new();
    builder.row_input(VALUE_U64, 0);
    let bytes = builder.build().expect("builds");
    assert_eq!(bytes.len(), PROGRAM_HEADER_LEN + 4);

    let program = ProgramView::parse(&bytes).expect("parse checks structure, not batch rules");
    assert_eq!(program.header.input_count(), 0);
    assert_eq!(program.header.row_input_count(), 1);
    // The parser is right: the inputs table holds the fixed and the row descriptors.
    assert_eq!(program.inputs.len(), program.header.total_input_count());
    // The rule's assertion is false here.
    assert_ne!(program.inputs.len(), program.header.input_count());
}

/// P17, finding F4, closed. The verifier checked every field the wire format calls reserved, but
/// not the operands an opcode leaves unused: a FOREACH's `dst`, `b` and `c`, and a REQUIRE's `dst`,
/// `b`, `c` and immediate, took any value. Every unused field must now be `0xff`, or zero for the
/// immediate, so two encodings of one template can no longer both verify. A FOREACH reports its
/// malformations as `InvalidBatch`, other opcodes as `InvalidInstruction` at their pc.
#[test]
fn unused_operands_of_foreach_and_require_are_refused() {
    let build = |dirty: &dyn Fn(&mut InstructionRecord, &mut InstructionRecord)| {
        let mut builder = ProgramBuilder::new();
        builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| {
            body.require(condition);
        });
        let instructions = builder.instructions_mut();
        let (foreach, require) = instructions.split_at_mut(2);
        assert_eq!((foreach[1].opcode, require[0].opcode), (OP_FOREACH, OP_REQUIRE));
        dirty(&mut foreach[1], &mut require[0]);
        builder.build().expect("builds")
    };
    let verify = |bytes: &[u8]| ProgramView::parse(bytes).and_then(|program| program.verify());
    assert!(verify(&build(&|_, _| {})).is_ok());
    let foreach: [(&str, &dyn Fn(&mut InstructionRecord)); 3] = [
        ("dst", &|record| record.dst = 7),
        ("b", &|record| record.b = 3),
        ("c", &|record| record.c = 9),
    ];
    for (field, set) in foreach {
        let bytes = build(&|record, _| set(record));
        assert_eq!(verify(&bytes), Err(TemplateError::InvalidBatch), "FOREACH {field}");
    }
    let require: [(&str, &dyn Fn(&mut InstructionRecord)); 4] = [
        ("dst", &|record| record.dst = 5),
        ("b", &|record| record.b = 6),
        ("c", &|record| record.c = 7),
        ("immediate", &|record| *record = record_with_immediate(record, 0xdead_beef)),
    ];
    for (field, set) in require {
        let bytes = build(&|_, record| set(record));
        assert_eq!(verify(&bytes), Err(TemplateError::InvalidInstruction(2)), "REQUIRE {field}");
    }
}

/// `record` with its immediate replaced.
fn record_with_immediate(record: &InstructionRecord, immediate: u64) -> InstructionRecord {
    InstructionRecord { immediate_le: immediate.to_le_bytes(), ..*record }
}
