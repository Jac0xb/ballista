//! The parser rejects foreign payloads before anything else and never returns a view whose
//! sections disagree with its header or overrun the payload.
//!
//! Whole-program `verify` has no rule here. It keeps its register table in a stack array and
//! indexes it by register numbers read from the payload, which the prover's pointer analysis does
//! not follow; moving the table would be a change to the program for the prover's sake. The
//! verifier is covered one instruction at a time in `rules::typing` instead.

use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::nondet_slice;

/// Longest payload the parser rules explore. Section tables are tiny at this size, which keeps
/// the prover fast while still covering every header field.
const PAYLOAD: usize = 96;

#[rule]
pub fn rule_parse_checks_magic_then_version_first() {
    let bytes: &[u8] = nondet_slice::<PAYLOAD>();
    cvlr_assume!(bytes.len() >= PROGRAM_HEADER_LEN);
    let magic_ok = bytes[..4] == TEMPLATE_PROGRAM_MAGIC;
    let version_ok = bytes[4] == TEMPLATE_PROGRAM_VERSION;
    match ProgramView::parse(bytes) {
        Ok(_) => cvlr_assert!(magic_ok && version_ok),
        Err(TemplateError::InvalidMagic) => cvlr_assert!(!magic_ok),
        Err(TemplateError::UnsupportedVersion(version)) => {
            cvlr_assert!(magic_ok && !version_ok && version == bytes[4])
        }
        Err(_) => cvlr_assert!(magic_ok && version_ok),
    }
}

#[rule]
pub fn rule_short_payloads_are_truncated_not_misparsed() {
    let bytes: &[u8] = nondet_slice::<PAYLOAD>();
    cvlr_assume!(bytes.len() < PROGRAM_HEADER_LEN);
    cvlr_assert!(matches!(ProgramView::parse(bytes), Err(TemplateError::Truncated)));
}

/// On success every section is exactly as long as the header says and the sections tile the
/// payload: header, accounts (the fixed accounts and one batch row), inputs (the fixed inputs and
/// one row's inputs), instructions, CPIs, CPI accounts, data segments, pubkeys, then the blob.
/// The flag and reserved bytes the parser checks are clear.
#[rule]
pub fn rule_parsed_sections_exactly_consume_the_payload() {
    let bytes: &[u8] = nondet_slice::<PAYLOAD>();
    if let Ok(program) = ProgramView::parse(bytes) {
        let header = program.header;
        cvlr_assert!(header.flags() & !PROGRAM_FLAGS_MASK == 0);
        cvlr_assert!(header.reserved() == &[0; 1]);
        cvlr_assert!(program.accounts.len() == header.fixed_account_count() + header.batch_stride());
        // The inputs table holds the fixed inputs, then the row inputs a batch reads per row.
        cvlr_assert!(program.inputs.len() == header.total_input_count());
        cvlr_assert!(program.instructions.len() == header.instruction_count());
        cvlr_assert!(program.cpis.len() == header.cpi_count());
        cvlr_assert!(program.cpi_accounts.len() == header.cpi_account_count());
        cvlr_assert!(program.data_segments.len() == header.data_segment_count());
        cvlr_assert!(program.pubkeys.len() == header.pubkey_count());
        cvlr_assert!(program.blob.len() == header.blob_len());
        let total = PROGRAM_HEADER_LEN
            + program.accounts.len() * 8
            + program.inputs.len() * 4
            + program.instructions.len() * 16
            + program.cpis.len() * 12
            + program.cpi_accounts.len() * 2
            + program.data_segments.len() * 8
            + program.pubkeys.len() * 32
            + program.blob.len();
        cvlr_assert!(total == bytes.len());
    } else {
        cvlr_satisfy!(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `rule_parsed_sections_exactly_consume_the_payload` asserts of a parsed payload.
    fn sections_tile_the_payload(bytes: &[u8]) {
        let program = ProgramView::parse(bytes).expect("parses");
        let header = program.header;
        assert_eq!(header.flags() & !PROGRAM_FLAGS_MASK, 0);
        assert_eq!(header.reserved(), &[0; 1]);
        assert_eq!(program.accounts.len(), header.fixed_account_count() + header.batch_stride());
        assert_eq!(program.inputs.len(), header.total_input_count());
        assert_eq!(program.instructions.len(), header.instruction_count());
        assert_eq!(program.cpis.len(), header.cpi_count());
        assert_eq!(program.cpi_accounts.len(), header.cpi_account_count());
        assert_eq!(program.data_segments.len(), header.data_segment_count());
        assert_eq!(program.pubkeys.len(), header.pubkey_count());
        assert_eq!(program.blob.len(), header.blob_len());
        let total = PROGRAM_HEADER_LEN
            + program.accounts.len() * 8
            + program.inputs.len() * 4
            + program.instructions.len() * 16
            + program.cpis.len() * 12
            + program.cpi_accounts.len() * 2
            + program.data_segments.len() * 8
            + program.pubkeys.len() * 32
            + program.blob.len();
        assert_eq!(total, bytes.len());
    }

    /// The counterexample to the rule's old statement, `inputs.len() == input_count()`: one row
    /// input descriptor and no fixed inputs, in 28 bytes.
    #[test]
    fn row_inputs_count_toward_the_inputs_table() {
        let mut builder = ProgramBuilder::new();
        builder.row_input(VALUE_U64, 0);
        let bytes = builder.build().expect("builds");
        assert!(bytes.len() <= PAYLOAD);
        let program = ProgramView::parse(&bytes).expect("parses");
        assert_ne!(program.inputs.len(), program.header.input_count());
        sections_tile_the_payload(&bytes);
    }

    #[test]
    fn every_section_kind_tiles_the_payload() {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        builder.input(VALUE_U64, 0);
        builder.row_input(VALUE_BOOL, 0);
        builder.pubkey([4; 32]);
        builder.blob(&[5; 3]);
        let value = builder.const_u64(1);
        let _ = (account, value);
        let bytes = builder.build().expect("builds");
        sections_tile_the_payload(&bytes);
    }
}
