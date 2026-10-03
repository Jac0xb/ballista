//! The main seeded executor fuzz loop. Each seed deterministically generates a template that
//! verifies, uploads and finalizes it, generates a run (accounts, run data, surrounding
//! instructions, with duplicates and deliberate mutations), runs it, and checks the invariants.
//!
//! Every finding is hard and fails the test with its seed: an invariant violation, a structural
//! error from a verified template, an account-rule error in Ballista's own frame, or any
//! disagreement with a concrete prediction of the reference model. The loop also fails when the
//! oracles compared too little: each count in [`FLOORS`] must reach its floor, so a generator or
//! model change that silently stops the comparisons cannot pass.
//!
//! - `FV_CASES` sets the case count (default 1,500), `FV_START` the first seed.
//! - `FV_SEED=<n>` replays one seed and prints what it compared.
//! - `FV_LIMITS=1` generates templates at the format's limits (128 instructions, 64 registers,
//!   64 worst-case CPIs, 64 accounts in one CPI, CPIs into row-account programs); see
//!   [`ballista_fuzz_gen::template::Config::limits`].
//! - `BALLISTA_MAINNET_FEATURES=1` runs on mainnet's feature set (`cases::base_mollusk`).
//! - `FV_DUMP=1` prints each compared template and its accounts.

use std::collections::BTreeMap;

use ballista_fuzz_gen::scenario::generate_scenario;
use ballista_fuzz_gen::source::{Gen, SplitMix64};
use ballista_fuzz_gen::template::{generate_template_with, Config};
use mollusk_svm::result::types::TransactionProgramResult;
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;

use super::harness::{Harness, BALLISTA_ID};
use super::invariants::{self, Compared};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

fn limits_mode() -> bool {
    std::env::var("FV_LIMITS").as_deref() == Ok("1")
}

/// One seed's outcome: its findings (each prefixed with the seed), and what it compared.
struct SeedResult {
    hard: Vec<String>,
    stats: Stats,
}

/// What one or many runs did, for the report and the floors.
#[derive(Default, Clone)]
struct Stats {
    cases: usize,
    fallbacks: usize,
    succeeded: usize,
    failed: usize,
    ballista_codes: usize,
    runs_with_cpis: usize,
    multi_open_successes: usize,
    registry_entry_rejections: usize,
    compared_runs: usize,
    compared_cpis: usize,
    compared_row_program_cpis: usize,
    compared_wide_cpis: usize,
    compared_data: usize,
    compared_flags: usize,
    compared_emits: usize,
    compared_return_data: usize,
    compared_loop_passes: usize,
    predicted_failures: usize,
    exact_failures: usize,
    classified_failures: usize,
    /// Why the model passed over successful runs.
    skipped: BTreeMap<String, usize>,
    /// Ballista codes raised while instructions ran, by kind and the opcode at the code's pc.
    in_run_failures: BTreeMap<String, usize>,
    /// Every failure, by the error and the program whose frame raised it.
    failures: BTreeMap<String, usize>,
    /// The largest template, register file and CPI fan-out generated, for limits mode.
    max_instructions: usize,
    max_registers: usize,
    max_cpi_accounts: usize,
    max_worst_case_cpis: usize,
    row_program_cpis: usize,
}

impl Stats {
    fn add(&mut self, other: Stats) {
        self.cases += other.cases;
        self.fallbacks += other.fallbacks;
        self.succeeded += other.succeeded;
        self.failed += other.failed;
        self.ballista_codes += other.ballista_codes;
        self.runs_with_cpis += other.runs_with_cpis;
        self.multi_open_successes += other.multi_open_successes;
        self.registry_entry_rejections += other.registry_entry_rejections;
        self.compared_runs += other.compared_runs;
        self.compared_cpis += other.compared_cpis;
        self.compared_row_program_cpis += other.compared_row_program_cpis;
        self.compared_wide_cpis += other.compared_wide_cpis;
        self.compared_data += other.compared_data;
        self.compared_flags += other.compared_flags;
        self.compared_emits += other.compared_emits;
        self.compared_return_data += other.compared_return_data;
        self.compared_loop_passes += other.compared_loop_passes;
        self.predicted_failures += other.predicted_failures;
        self.exact_failures += other.exact_failures;
        self.classified_failures += other.classified_failures;
        for (key, count) in other.skipped {
            *self.skipped.entry(key).or_default() += count;
        }
        for (key, count) in other.in_run_failures {
            *self.in_run_failures.entry(key).or_default() += count;
        }
        for (key, count) in other.failures {
            *self.failures.entry(key).or_default() += count;
        }
        self.max_instructions = self.max_instructions.max(other.max_instructions);
        self.max_registers = self.max_registers.max(other.max_registers);
        self.max_cpi_accounts = self.max_cpi_accounts.max(other.max_cpi_accounts);
        self.max_worst_case_cpis = self.max_worst_case_cpis.max(other.max_worst_case_cpis);
        self.row_program_cpis += other.row_program_cpis;
    }

    fn record(&mut self, compared: Compared) {
        self.compared_runs += usize::from(compared.run);
        self.compared_cpis += compared.cpis;
        self.compared_row_program_cpis += compared.row_program_cpis;
        self.compared_wide_cpis += compared.wide_cpis;
        self.compared_data += compared.data;
        self.compared_flags += compared.flags;
        self.compared_emits += compared.emits;
        self.compared_return_data += usize::from(compared.return_data);
        self.compared_loop_passes += compared.loop_passes;
        self.predicted_failures += usize::from(compared.predicted_failure);
        self.exact_failures += usize::from(compared.exact_failure);
        self.classified_failures += usize::from(compared.classified_failure);
    }
}

/// A floor on one of the loop's counts, per 1,000 cases, for normal and for limits mode. Each is
/// about half of what 20,000 seeds from seed 0 measured when it was set (2026-10-03, both feature
/// sets alike), so a change that halves a comparison fails the test while the noise between seed
/// ranges does not. Changing a floor, like changing a compute-unit ceiling, is deliberate and
/// reviewed.
struct Floor {
    what: &'static str,
    normal: usize,
    limits: usize,
    count: fn(&Stats) -> usize,
}

const FLOORS: [Floor; 12] = [
    // Measured per 1,000 at 20,000 seeds, normal / limits: 175 / 99.
    Floor { what: "successful runs the model compared", normal: 87, limits: 50, count: |s| s.compared_runs },
    // 262 / 850.
    Floor { what: "CPIs compared by program and accounts", normal: 130, limits: 425, count: |s| s.compared_cpis },
    // 0 / 30: only limits mode names a program in a batch row.
    Floor { what: "CPIs into a row-account program compared", normal: 0, limits: 15, count: |s| s.compared_row_program_cpis },
    // 0 / 118: only limits mode passes 16 or more accounts.
    Floor { what: "CPIs with 16 or more accounts compared", normal: 0, limits: 58, count: |s| s.compared_wide_cpis },
    // 252 / 810.
    Floor { what: "CPIs compared byte for byte", normal: 125, limits: 405, count: |s| s.compared_data },
    // 133 / 754.
    Floor { what: "probe calls whose received flags were compared", normal: 65, limits: 375, count: |s| s.compared_flags },
    // 311 / 590.
    Floor { what: "EMIT lines compared byte for byte", normal: 155, limits: 295, count: |s| s.compared_emits },
    // 15.3 / 9.3.
    Floor { what: "return data compared byte for byte", normal: 7, limits: 4, count: |s| s.compared_return_data },
    // 230 / 1,006.
    Floor { what: "loop passes on compared runs", normal: 115, limits: 500, count: |s| s.compared_loop_passes },
    // 439 / 461.
    Floor { what: "failures the model predicted", normal: 220, limits: 230, count: |s| s.predicted_failures },
    // 140 / 80.
    Floor { what: "predicted failures the run matched exactly", normal: 70, limits: 40, count: |s| s.exact_failures },
    // 441 / 424.
    Floor { what: "failed runs whose Ballista code was classified", normal: 220, limits: 210, count: |s| s.classified_failures },
];

/// Floors apply from this many cases; fewer are a smoke run whose counts are too noisy to judge.
const FLOOR_MIN_CASES: usize = 1_000;

/// Runs one seed end to end.
fn run_seed(harness: &Harness, config: &Config, seed: u64) -> SeedResult {
    harness.reset();
    let world = super::harness::world();
    let mut source = SplitMix64::new(seed);
    let mut gen = Gen::new(&mut source);
    let plan = generate_template_with(&mut gen, &world, config);
    let mut stats = Stats { cases: 1, fallbacks: usize::from(plan.fell_back), ..Stats::default() };
    if let Ok(program) = ballista_common::template::ProgramView::parse(&plan.bytes) {
        stats.max_instructions = program.instructions.len();
        stats.max_registers = program.header.register_count();
        stats.max_cpi_accounts = program.cpis.iter().map(|cpi| cpi.account_len as usize).max().unwrap_or(0);
        stats.max_worst_case_cpis = program.verify().map(|summary| summary.max_expanded_cpis as usize).unwrap_or(0);
        stats.row_program_cpis = program
            .cpis
            .iter()
            .filter(|cpi| cpi.program_account & ballista_common::template::ITERATION_ACCOUNT_BIT != 0)
            .count();
    }

    // A fixed creator and id: every seed runs in its own stateless Mollusk invocation, so there is
    // no collision between seeds.
    let creator = Pubkey::new_from_array([0x11; 32]);
    let template_id = 1u16;
    let (template, finalized) = match harness.upload(&creator, template_id, &plan.bytes) {
        Ok(uploaded) => uploaded,
        Err(error) => {
            return SeedResult { hard: vec![format!("seed {seed}: a generated template failed to upload: {error}")], stats };
        }
    };

    let rent = |len: usize| harness.rent_minimum(len);
    let pda = |seeds: &[&[u8]], program: &[u8; 32]| {
        let (address, bump) = Pubkey::find_program_address(seeds, &Pubkey::new_from_array(*program));
        (address.to_bytes(), bump)
    };
    let scenario = generate_scenario(&mut gen, &world, &plan, template.to_bytes(), &pda, &rent);

    let inputs = invariants::decode_inputs(&plan, &scenario);
    let outcome = harness.run(&scenario, &template, &finalized);
    let report = invariants::check(harness, &world, &plan, &scenario, &template, &finalized, &inputs, &outcome);

    let succeeded = outcome.result.program_result.is_ok();
    let code = match &outcome.result.program_result {
        TransactionProgramResult::Failure(_, ProgramError::Custom(code)) => Some(*code),
        _ => None,
    };
    let kind = code.map(|code| code & 0xffff);
    if !succeeded {
        let raiser = super::harness::log_trace(&outcome)
            .first_failure
            .map(|(program, _, _)| program_name(&world, &program))
            .unwrap_or("?");
        let error = match &outcome.result.program_result {
            TransactionProgramResult::Failure(_, ProgramError::Custom(code)) if (6000..=6200).contains(&(code & 0xffff)) => {
                format!("{}", code & 0xffff)
            }
            TransactionProgramResult::Failure(_, ProgramError::Custom(code))
                if code & !0xff == ballista_fuzz_gen::template::probe::FAIL_BASE =>
            {
                "FAIL op".to_string()
            }
            TransactionProgramResult::Failure(_, error) => format!("{error:?}"),
            TransactionProgramResult::UnknownError(_, error) => format!("{error:?}"),
            TransactionProgramResult::Success => unreachable!(),
        };
        stats.failures.insert(format!("{raiser}: {error}"), 1);
    }
    stats.succeeded = usize::from(succeeded);
    stats.failed = usize::from(!succeeded);
    stats.ballista_codes = usize::from(kind.is_some_and(|kind| (6000..=6132).contains(&kind)));
    stats.runs_with_cpis = usize::from(!super::harness::captured_cpis(&outcome).is_empty());
    stats.multi_open_successes = usize::from(succeeded && plan.opens.len() >= 2);
    stats.registry_entry_rejections = usize::from(kind == Some(6025));
    stats.record(report.compared);
    if let Some(reason) = report.skipped {
        stats.skipped.insert(reason, 1);
    }
    if let (Some(code), Some(kind)) = (code, kind) {
        // Account and input validation kinds carry an index, not a pc, and are left out.
        if (6000..=6026).contains(&kind) && !matches!(kind, 6000..=6008 | 6010 | 6020) {
            let opcode = ballista_common::template::ProgramView::parse(&plan.bytes)
                .ok()
                .and_then(|program| program.instructions.get((code >> 16) as usize).map(|record| record.opcode));
            stats.in_run_failures.insert(format!("kind={kind} op={opcode:?}"), 1);
        }
    }
    let hard = report.hard.into_iter().map(|message| format!("seed {seed}: {message}")).collect();
    SeedResult { hard, stats }
}

/// A short name for a program address, for the failure histogram.
fn program_name(world: &ballista_fuzz_gen::template::World, address: &Pubkey) -> &'static str {
    use ballista_fuzz_gen::template::{Program, ALL_PROGRAMS};
    let program = ALL_PROGRAMS.into_iter().find(|&program| Pubkey::new_from_array(world.program(program)) == *address);
    match program {
        Some(Program::Ballista) => "ballista",
        Some(Program::Probe | Program::ProbeCopy) => "probe",
        Some(Program::System) => "system",
        Some(Program::Token) => "token",
        None => "other",
    }
}

#[test]
fn fuzz_executor_differential() {
    assert_eq!(super::harness::BALLISTA_ID, BALLISTA_ID);
    let harness = Harness::new();
    assert!(
        harness.has_probe,
        "the probe program is not built, so wrapped runs and probe CPIs would fail for a reason \
         unrelated to Ballista and the model would compare almost nothing. Build it with\n  \
         cargo build-sbf --manifest-path fuzz-executor/probe/Cargo.toml"
    );
    let limits = limits_mode();
    let config = if limits { Config::limits() } else { Config::default() };

    if let Ok(seed) = std::env::var("FV_SEED") {
        let seed: u64 = seed.parse().expect("FV_SEED must be a number");
        let result = run_seed(&harness, &config, seed);
        let s = &result.stats;
        eprintln!(
            "seed {seed}: succeeded={} compared run={} cpis={} data={} flags={} return_data={} loop_passes={} \
             predicted_failure={} classified_failure={} skipped={:?} in_run={:?}",
            s.succeeded, s.compared_runs, s.compared_cpis, s.compared_data, s.compared_flags, s.compared_return_data,
            s.compared_loop_passes, s.predicted_failures, s.classified_failures, s.skipped, s.in_run_failures
        );
        assert!(result.hard.is_empty(), "findings:\n{}", result.hard.join("\n"));
        return;
    }

    let cases = env_u64("FV_CASES", 1500);
    let start = env_u64("FV_START", 0);
    let mut hard = Vec::new();
    let mut stats = Stats::default();
    for offset in 0..cases {
        let seed = start.wrapping_add(offset);
        let result = run_seed(&harness, &config, seed);
        stats.add(result.stats);
        hard.extend(result.hard);
        if hard.len() > 40 {
            break;
        }
    }

    let mode = if limits { "limits" } else { "normal" };
    let features = if std::env::var("BALLISTA_MAINNET_FEATURES").as_deref() == Ok("1") { "mainnet" } else { "all" };
    eprintln!(
        "ran {} cases from seed {start} ({mode} templates, {features} features); {} generator fallbacks",
        stats.cases, stats.fallbacks
    );
    eprintln!(
        "outcomes: {} succeeded, {} failed ({} with a Ballista code, {} InvalidRegistryEntry); {} runs made CPIs; \
         {} runs with 2+ opens succeeded",
        stats.succeeded, stats.failed, stats.ballista_codes, stats.registry_entry_rejections, stats.runs_with_cpis,
        stats.multi_open_successes,
    );
    eprintln!(
        "compared: {} runs, {} CPIs ({} into row-account programs, {} with 16+ accounts), {} CPI data, {} probe flag sets, \
         {} EMIT lines, {} return data, {} loop passes; {} predicted failures ({} matched exactly); {} failures classified",
        stats.compared_runs, stats.compared_cpis, stats.compared_row_program_cpis, stats.compared_wide_cpis,
        stats.compared_data, stats.compared_flags, stats.compared_emits, stats.compared_return_data,
        stats.compared_loop_passes, stats.predicted_failures, stats.exact_failures, stats.classified_failures,
    );
    eprintln!(
        "largest template: {} instructions, {} registers, {} accounts in one CPI, {} worst-case CPIs; {} CPIs into row-account programs",
        stats.max_instructions, stats.max_registers, stats.max_cpi_accounts, stats.max_worst_case_cpis, stats.row_program_cpis
    );
    for (reason, count) in &stats.skipped {
        eprintln!("model skipped a successful run ({reason}): {count}");
    }
    for (key, count) in &stats.failures {
        eprintln!("failure {key}: {count}");
    }
    for (key, count) in &stats.in_run_failures {
        eprintln!("in-run failure {key}: {count}");
    }
    assert!(hard.is_empty(), "findings (replay with FV_SEED=<n>, and FV_LIMITS=1 if set):\n{}", hard.join("\n"));

    if stats.cases < FLOOR_MIN_CASES {
        eprintln!("note: {} cases is under {FLOOR_MIN_CASES}; the comparison floors were not checked", stats.cases);
        return;
    }
    let mut short = Vec::new();
    for floor in &FLOORS {
        let per_thousand = if limits { floor.limits } else { floor.normal };
        if per_thousand == 0 {
            continue;
        }
        let needed = per_thousand * stats.cases / 1000;
        let got = (floor.count)(&stats);
        eprintln!("floor: {} {got} (needs {needed})", floor.what);
        if got < needed {
            short.push(format!("{}: {got}, under the floor of {needed} ({per_thousand} per 1,000 cases)", floor.what));
        }
    }
    assert!(short.is_empty(), "the oracles compared too little:\n{}", short.join("\n"));
}
