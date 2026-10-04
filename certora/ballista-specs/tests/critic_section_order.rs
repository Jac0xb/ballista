//! Critic experiment (second pass): `rule_parsed_sections_exactly_consume_the_payload` asserts each
//! section's length and that the lengths sum to the payload, not where each section starts. A parser
//! that took the CPI table before the instructions would satisfy every assertion of the rule while
//! reading every instruction from the wrong bytes. Kani's `wire::assert_sections_tile` checks each
//! section's start pointer and would refute it. Run with the parser mutated to see the gap.
use ballista_common::template::*;

/// What the Certora rule asserts of a parsed payload (copied from `rules::parser`).
fn rule_assertions_hold(bytes: &[u8], program: &ProgramView<'_>) -> bool {
    let header = program.header;
    let total = PROGRAM_HEADER_LEN
        + program.accounts.len() * 8
        + program.inputs.len() * 4
        + program.instructions.len() * 16
        + program.cpis.len() * 12
        + program.cpi_accounts.len() * 2
        + program.data_segments.len() * 8
        + program.pubkeys.len() * 32
        + program.blob.len();
    program.accounts.len() == header.fixed_account_count() + header.batch_stride()
        && program.inputs.len() == header.total_input_count()
        && program.instructions.len() == header.instruction_count()
        && program.cpis.len() == header.cpi_count()
        && program.cpi_accounts.len() == header.cpi_account_count()
        && program.data_segments.len() == header.data_segment_count()
        && program.pubkeys.len() == header.pubkey_count()
        && program.blob.len() == header.blob_len()
        && total == bytes.len()
}

#[test]
fn the_rule_cannot_tell_where_sections_start() {
    let mut builder = ProgramBuilder::new();
    let callee = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
    let cpi = builder.cpi(callee, &[], &[]);
    builder.invoke(cpi, None);
    let bytes = builder.build().expect("builds");
    let program = ProgramView::parse(&bytes).expect("parses");
    assert!(program.cpis.len() == 1 && program.instructions.len() == 1);
    let rule = rule_assertions_hold(&bytes, &program);
    let instructions_at = program.instructions.as_ptr() as usize - bytes.as_ptr() as usize;
    let expected_at = PROGRAM_HEADER_LEN + program.accounts.len() * 8 + program.inputs.len() * 4;
    println!("rule assertions hold: {rule}; instructions start at {instructions_at}, expected {expected_at}; first opcode parsed {} (INVOKE is {OP_INVOKE})", program.instructions[0].opcode);
    assert!(rule, "the rule's assertions hold");
    assert_eq!(instructions_at, expected_at, "instructions start right after the inputs");
}
