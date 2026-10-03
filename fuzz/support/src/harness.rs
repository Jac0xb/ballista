//! What each fuzz target asserts, shared so the targets stay one line each and the support crate's
//! tests can replay seeds through exactly the same checks.

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

use ballista_common::{
    instruction::BallistaInstruction,
    template::{
        decode_ballista_error, encode_error, ProgramView, TemplateAccount, TemplateAccountHeader,
        TemplateError, VerificationStats, MAX_TEMPLATE_PAYLOAD_LEN, VERIFIER_ERROR_BASE,
        VERIFIER_ERROR_NAMES,
    },
};

use crate::{ceiling, checker, model::Program, negative};

/// Rules whose violation is a known, reported finding not fixed yet (see `fuzz/README.md`). The
/// differential target reports them once each and keeps going, so nightly fuzzing still stops on
/// anything new. Set `BALLISTA_FUZZ_STRICT=1` to make them fatal again, for example to check a
/// fix. Empty: the verifier now refuses the four this list held (`cpi.segment-literal-register`,
/// `cpi.segment-register-fields`, `unreferenced.segment` and `unreferenced.cpi`), and
/// `common/tests/fuzz_findings.rs` pins each refusal.
pub const KNOWN_FINDINGS: &[&str] = &[];

/// `BALLISTA_FUZZ_STRICT`: unset or `0` keeps every known finding non-fatal; `1` makes them all
/// fatal; a comma-separated list of rule names makes just those fatal, so a short run crashes on
/// one finding and `cargo fuzz tmin` can minimize it.
fn strict(rule: &str) -> bool {
    static STRICT: OnceLock<Option<String>> = OnceLock::new();
    match STRICT.get_or_init(|| std::env::var("BALLISTA_FUZZ_STRICT").ok()).as_deref() {
        None | Some("0") | Some("") => false,
        Some("1") => true,
        Some(rules) => rules.split(',').any(|named| named.trim() == rule),
    }
}

/// Whether a violation of `rule` stops the fuzzer.
pub fn is_fatal(rule: &str) -> bool {
    strict(rule) || !KNOWN_FINDINGS.contains(&rule)
}

/// Prints a known finding the first time this process sees its rule.
pub fn note_known(violation: &checker::Violation) {
    static SEEN: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if seen.get_or_insert_with(HashSet::new).insert(violation.rule) {
        eprintln!("known finding, not fatal: {violation}");
    }
}

/// The sections of two views are the same slices of the same payload.
pub fn same_sections(left: &ProgramView<'_>, right: &ProgramView<'_>) -> bool {
    core::ptr::eq(left.header, right.header)
        && core::ptr::eq(left.accounts, right.accounts)
        && core::ptr::eq(left.inputs, right.inputs)
        && core::ptr::eq(left.instructions, right.instructions)
        && core::ptr::eq(left.cpis, right.cpis)
        && core::ptr::eq(left.cpi_accounts, right.cpi_accounts)
        && core::ptr::eq(left.data_segments, right.data_segments)
        && core::ptr::eq(left.pubkeys, right.pubkeys)
        && core::ptr::eq(left.blob, right.blob)
}

/// `parse` target. The bytes are tried as a payload, as a whole template account, and as
/// instruction data, the three things the program parses from untrusted input. Nothing may panic,
/// and:
///
/// - the run path's `parse_finalized` splits a payload exactly as `parse` does wherever `parse`
///   accepts it, and refuses only what `parse` refuses for its structure (it skips the size cap,
///   the header flags and the reserved byte, which verification settled);
/// - the independent model in `model.rs` splits the payload into the same sections;
/// - a finalized template account's fast read finds the program its full parse finds.
pub fn parse(data: &[u8]) {
    let parsed = ProgramView::parse(data);
    let fast = ProgramView::parse_finalized(data);
    match (&parsed, &fast) {
        (Ok(view), Some(fast)) => {
            assert!(same_sections(view, fast), "parse_finalized splits the payload as parse does")
        }
        (Ok(_), None) => panic!("parse_finalized refuses a payload parse accepts"),
        (Err(error), Some(_)) => assert!(
            matches!(error, TemplateError::PayloadTooLarge(_) | TemplateError::InvalidReservedBytes),
            "parse_finalized accepts a payload parse refuses for its structure: {error:?}"
        ),
        (Err(_), None) => {}
    }

    let model = Program::decode(data);
    match (&parsed, &model) {
        (Ok(view), Ok(model)) => {
            assert_eq!(view.accounts.len(), model.accounts.len());
            assert_eq!(view.inputs.len(), model.inputs.len());
            assert_eq!(view.instructions.len(), model.instrs.len());
            assert_eq!(view.cpis.len(), model.cpis.len());
            assert_eq!(view.cpi_accounts.len(), model.cpi_accounts.len());
            assert_eq!(view.data_segments.len(), model.segments.len());
            assert_eq!(view.pubkeys.len(), model.pubkeys.len());
            assert_eq!(view.blob, model.blob.as_slice());
            assert_eq!(model.encode(), data, "the model re-encodes a payload byte for byte");
        }
        (Ok(_), Err(error)) => panic!("the model cannot split a payload parse accepts: {error:?}"),
        (Err(error), Ok(model)) => {
            // The model checks only the structure, so parse must have refused something else.
            let header = &model.header;
            let refusable = data.len() > MAX_TEMPLATE_PAYLOAD_LEN
                || header.magic != crate::model::MAGIC
                || header.version != crate::model::VERSION
                || header.flags & !checker::FLAG_EMIT_EVENT != 0
                || header.reserved != 0;
            assert!(refusable, "parse refuses a well-shaped payload with {error:?}");
        }
        (Err(_), Err(_)) => {}
    }

    // The same bytes as a template account's data.
    if let Ok(account) = TemplateAccount::parse(data) {
        if let Ok(program) = account.finalized_program() {
            let fast = TemplateAccount::finalized_program_unchecked(data)
                .expect("the run's fast path reads every finalized account the full parse reads");
            assert!(same_sections(&program, &fast));
        }
    }
    let _ = TemplateAccount::finalized_program_unchecked(data);

    // The same bytes as instruction data.
    let _ = BallistaInstruction::parse(data);
}

/// `verify` target: `data` is a payload, verified the two ways the program verifies one.
///
/// - `CreateTemplate` parses the instruction's payload and verifies it.
/// - `BeginTemplate`, `WriteTemplateChunk` and `FinalizeTemplate` store it in a template account,
///   then `FinalizeTemplate` reads it back through `TemplateAccount::parse` and verifies that.
///   The hash check that precedes it is left out: it reads the same bytes and never reaches
///   `verify`'s inputs.
///
/// Both must reach the same verdict without panicking. A payload that verifies must then run:
/// the finalized account's fast read finds the same program. Its stats echo the header and stay
/// within the limits, and an error maps to a verifier code the SDKs can name.
pub fn verify(data: &[u8]) -> Option<VerificationStats> {
    let direct = ProgramView::parse(data).and_then(|program| program.verify());
    assert_eq!(
        ProgramView::parse(data).and_then(|program| program.verify()),
        direct,
        "verification is deterministic"
    );

    if (1..=MAX_TEMPLATE_PAYLOAD_LEN).contains(&data.len()) {
        let mut header = TemplateAccountHeader::new_uploading([7; 32], 1, 254, data.len(), [9; 32])
            .expect("a payload within the limit gets an account header");
        header.set_written_len(data.len()).expect("every byte written");
        let mut account = header.as_bytes().to_vec();
        account.extend_from_slice(data);
        let staged = {
            let stored = TemplateAccount::parse(&account).expect("an uploaded account parses");
            assert!(!stored.header().is_finalized());
            assert_eq!(stored.header().written_len(), stored.header().payload_len());
            assert!(TemplateAccount::finalized_program_unchecked(&account).is_none());
            ProgramView::parse(stored.payload()).and_then(|program| program.verify())
        };
        assert_eq!(staged, direct, "create and finalize reach the same verdict");

        if direct.is_ok() {
            header.finalize().expect("finalize");
            let header_len = header.as_bytes().len();
            account[..header_len].copy_from_slice(header.as_bytes());
            let fast = TemplateAccount::finalized_program_unchecked(&account)
                .expect("a run reads every template finalization accepted");
            let full = ProgramView::parse(data).expect("verified, so parsed");
            assert_eq!(fast.header, full.header);
            assert_eq!(fast.instructions, full.instructions);
            assert_eq!(fast.blob, full.blob);
        }
    }

    match direct {
        Ok(stats) => {
            let view = ProgramView::parse(data).expect("verified, so parsed");
            let header = view.header;
            assert_eq!(stats.fixed_accounts as usize, header.fixed_account_count());
            assert_eq!(stats.batch_stride as usize, header.batch_stride());
            assert_eq!(stats.batch_max_iterations as usize, header.batch_max_iterations());
            assert_eq!(stats.inputs as usize, header.input_count());
            assert_eq!(stats.registers as usize, header.register_count());
            assert_eq!(stats.instructions as usize, header.instruction_count());
            assert_eq!(stats.cpis as usize, header.cpi_count());
            assert!(stats.max_expanded_cpis as usize <= checker::limit::CPIS);
            assert!(stats.max_cpi_data_len as usize <= checker::limit::CPI_DATA);
            Some(stats)
        }
        Err(error) => {
            let (kind, context) = error.code();
            assert!(
                (VERIFIER_ERROR_BASE..VERIFIER_ERROR_BASE + VERIFIER_ERROR_NAMES.len() as u32).contains(&kind),
                "{error:?} maps to {kind}"
            );
            let decoded = decode_ballista_error(encode_error(kind, context)).expect("a verifier code decodes");
            assert_eq!(decoded.context, context);
            let debug = format!("{error:?}");
            let bare = debug.split('(').next().unwrap_or_default();
            assert_eq!(bare, decoded.name, "{error:?} decodes as {}", decoded.name);
            None
        }
    }
}

/// Accepted programs the ceiling pass has checked in this process, with their invoke sites and
/// account records. Logged at every power of two, so a run's log says how much it covered.
fn count_ceiling(found: &ceiling::Ceiling) {
    static PROGRAMS: AtomicU64 = AtomicU64::new(0);
    static SITES: AtomicU64 = AtomicU64::new(0);
    static RECORDS: AtomicU64 = AtomicU64::new(0);
    static AT_BOUND: AtomicU64 = AtomicU64::new(0);
    let programs = PROGRAMS.fetch_add(1, Ordering::Relaxed) + 1;
    let sites = SITES.fetch_add(found.invoke_sites as u64, Ordering::Relaxed) + found.invoke_sites as u64;
    let records = RECORDS.fetch_add(found.records as u64, Ordering::Relaxed) + found.records as u64;
    let at_bound = AT_BOUND.fetch_add(u64::from(found.worst_case_cpis == 64), Ordering::Relaxed)
        + u64::from(found.worst_case_cpis == 64);
    if programs.is_power_of_two() && programs >= 1 << 10 {
        eprintln!(
            "ceiling pass: {programs} accepted programs, {sites} invoke sites, {records} account records, \
             {at_bound} at exactly 64 worst-case CPIs"
        );
    }
}

/// Breaks one rule in `data`, a payload `verify` accepts, and requires `verify` to reject the
/// result with that rule's error. `choice` picks which of the breaks that apply.
pub fn negative(data: &[u8], choice: usize) {
    let Ok(program) = Program::decode(data) else { return };
    let breaks = negative::breaks(&program);
    if let Some(broken) = breaks.get(choice % breaks.len().max(1)) {
        negative::require_rejected(broken);
    }
}

/// `differential` target, and the last step of `structured`: whatever `verify` accepts must pass
/// the CPI ceiling pass in `ceiling.rs`, always fatal, then the reference checker in `checker.rs`,
/// and both must count the worst case as the verifier does.
///
/// Returns the violation of a known finding, if one was seen, so callers can report it.
pub fn differential(data: &[u8]) -> Option<checker::Violation> {
    let stats = verify(data)?;
    let model = Program::decode(data).expect("a verified payload fits the model");
    // Only finalization enforces these, so nothing may mask them: no known finding applies.
    match ceiling::check(&model) {
        Ok(found) => {
            assert_eq!(
                found.worst_case_cpis, stats.max_expanded_cpis as usize,
                "the verifier and the ceiling pass count different worst-case CPIs"
            );
            count_ceiling(&found);
        }
        Err(violation) => panic!("verify accepted a program whose CPIs break the ceiling: {violation}"),
    }
    match checker::check(&model, data.len()) {
        Ok(report) => {
            assert_eq!(
                report.worst_case_cpis, stats.max_expanded_cpis as usize,
                "the verifier and the checker count different worst-case CPIs"
            );
            assert_eq!(
                report.max_cpi_data_len, stats.max_cpi_data_len as usize,
                "the verifier and the checker find different worst-case CPI data"
            );
            let mut known = None;
            for note in report.notes {
                if is_fatal(note.rule) {
                    panic!("verify accepted a program that breaks an encoding rule: {note}");
                }
                known.get_or_insert(note);
            }
            known
        }
        Err(violation) => {
            if is_fatal(violation.rule) {
                panic!("verify accepted a program the reference checker rejects: {violation}");
            }
            Some(violation)
        }
    }
}
