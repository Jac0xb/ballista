//! Confirmation tests for findings from the safety-property review
//! (`docs/superpowers/specs/2026-10-03-safety-properties.md`). Each test names the property it
//! checks. They pass today: each one pins a fact that a check elsewhere states wrongly, or that the
//! documentation leaves out.

use ballista_common::template::{
    record, ProgramBuilder, ProgramView, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, OP_FOREACH,
    OP_REQUIRE, PROGRAM_HEADER_LEN, VALUE_U64,
};

/// P-PARSE-2. `ProgramView::parse` reads `input_count + row_input_count` input descriptors. The
/// Certora rule `rule_parsed_sections_exactly_consume_the_payload` (in `run.conf`) asserts
/// `program.inputs.len() == header.input_count()`, so this 28-byte payload, well inside the rule's
/// 96-byte bound, is a counterexample to it: the rule cannot prove as written. The fix is to
/// assert against `header.total_input_count()`.
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

/// P-FIN-1 (canonical encoding). The verifier checks every field the wire format calls reserved,
/// but not the operands an opcode leaves unused: a FOREACH's `dst`, `b` and `c`, and a REQUIRE's
/// `dst`, `b`, `c` and immediate, accept any value. Two payloads that differ only there verify
/// alike and run alike, so these bytes can never take a meaning later without a version bump.
#[test]
fn unused_operands_of_foreach_and_require_accept_any_value() {
    let build = |dirty: bool| {
        let mut builder = ProgramBuilder::new();
        builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| {
            body.require(condition);
        });
        if dirty {
            let instructions = builder.instructions_mut();
            let foreach = instructions
                .iter_mut()
                .find(|record| record.opcode == OP_FOREACH)
                .expect("a FOREACH");
            foreach.dst = 7;
            foreach.b = 3;
            foreach.c = 9;
            let require = instructions
                .iter_mut()
                .find(|record| record.opcode == OP_REQUIRE)
                .expect("a REQUIRE");
            *require = record(OP_REQUIRE, 5, condition, 6, 7, 0, 0xdead_beef);
        }
        builder.build().expect("builds")
    };
    let clean = build(false);
    let dirty = build(true);
    assert_ne!(clean, dirty);
    let verify = |bytes: &[u8]| ProgramView::parse(bytes).and_then(|program| program.verify());
    assert!(verify(&clean).is_ok());
    assert_eq!(verify(&dirty), verify(&clean), "the unused operands are not checked");
}
