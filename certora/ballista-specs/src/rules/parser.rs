//! The parser rejects foreign payloads before anything else and never returns a view whose
//! sections disagree with its header or overrun the payload.

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

#[rule]
pub fn rule_parsed_sections_exactly_consume_the_payload() {
    let bytes: &[u8] = nondet_slice::<PAYLOAD>();
    if let Ok(program) = ProgramView::parse(bytes) {
        let header = program.header;
        cvlr_assert!(header.flags() & !PROGRAM_FLAGS_MASK == 0);
        cvlr_assert!(program.accounts.len() == header.fixed_account_count() + header.batch_stride());
        cvlr_assert!(program.inputs.len() == header.input_count());
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

#[rule]
pub fn rule_verified_programs_respect_every_static_limit() {
    let bytes: &[u8] = nondet_slice::<PAYLOAD>();
    let Ok(program) = ProgramView::parse(bytes) else {
        cvlr_satisfy!(true);
        return;
    };
    if let Ok(stats) = program.verify() {
        cvlr_assert!(stats.registers as usize <= MAX_REGISTERS);
        cvlr_assert!(stats.instructions as usize <= MAX_VM_INSTRUCTIONS);
        cvlr_assert!(stats.instructions >= 1);
        cvlr_assert!(stats.max_expanded_cpis as usize <= MAX_EXPANDED_CPIS);
        cvlr_assert!(stats.max_cpi_data_len as usize <= MAX_CPI_DATA_LEN);
        cvlr_assert!(stats.batch_stride as usize <= MAX_BATCH_STRIDE);
        cvlr_assert!(
            program.header.batch_min_iterations() <= program.header.batch_max_iterations()
        );
        cvlr_assert!(
            stats.fixed_accounts as usize
                + stats.batch_stride as usize * stats.batch_max_iterations as usize
                <= MAX_RUNTIME_ACCOUNTS
        );
    }
}
