//! The main seeded executor fuzz loop. Each seed deterministically generates a template that
//! verifies, uploads and finalizes it, generates a run (accounts, run data, surrounding
//! instructions, with duplicates and deliberate mutations), runs it, and checks the invariants.
//!
//! Replay a failing seed with `FV_SEED=<n>`. Set the case count with `FV_CASES` and the first seed
//! with `FV_START`. The default count keeps `cargo test` quick; the campaign runs tens of
//! thousands (see the report).

use ballista_fuzz_gen::scenario::generate_scenario;
use ballista_fuzz_gen::source::{Gen, SplitMix64};
use ballista_fuzz_gen::template::generate_template;
use solana_pubkey::Pubkey;

use super::harness::{Harness, BALLISTA_ID};
use super::invariants;

/// Critic (second pass): the oracle this loop lacked. It accepted any Ballista code on a failed run,
/// so a verified template failing for a structural reason (6002, 6011, 6016, 6012 other than a
/// `bool` decode, 6009 at a fixed offset; the split in `certora/ballista-specs/src/rules/oracle.rs`)
/// passed. With `>=` for `>` in `invoke_cpi`'s data-length check, 5,895 of 20,000 seeds failed with
/// 6016 and the loop still passed. Unmutated, 20,000 seeds give 0 (13,219 runs failed with a
/// Ballista code). The host enumeration covers this property for 55 opcodes; this covers the other
/// 21 on generated templates.
static CRITIC_KINDS: std::sync::Mutex<std::collections::BTreeMap<String, usize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());
static CRITIC_STRUCTURAL: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn critic_classify(seed: u64, plan: &ballista_fuzz_gen::template::TemplatePlan, scenario: &ballista_fuzz_gen::scenario::Scenario, outcome: &super::harness::RunOutcome) {
    use ballista_common::template::*;
    let mollusk_svm::result::types::TransactionProgramResult::Failure(index, solana_program_error::ProgramError::Custom(raw)) = &outcome.result.program_result else { return };
    let (kind, context) = (raw & 0xffff, (raw >> 16) as usize);
    if !(6000..=6026).contains(&kind) {
        return;
    }
    let program = ProgramView::parse(&plan.bytes).ok();
    let instruction = program.as_ref().and_then(|p| p.instructions.get(context)).copied();
    let opcode = instruction.map(|i| i.opcode);
    let reads = |op: u8| matches!(op, OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_I32 | OP_READ_U128 | OP_READ_PUBKEY | OP_READ_BOOL);
    let bool_decode = |i: &InstructionRecord| match i.opcode {
        OP_READ_BOOL => true,
        OP_RETURN_DATA => i.a == OP_READ_BOOL,
        OP_READ_INSTRUCTION_DATA => i.immediate() == u64::from(OP_READ_BOOL),
        OP_READ_REGISTRY => (i.immediate() >> 16) as u8 == OP_READ_BOOL,
        _ => false,
    };
    let structural = match (kind, instruction) {
        (6002 | 6011 | 6016, _) => true,
        (6009, Some(i)) => !(reads(i.opcode) && i.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0) && reads(i.opcode),
        (6012, Some(i)) => !bool_decode(&i),
        _ => false,
    };
    let calls_ballista = plan.calls.contains(&ballista_fuzz_gen::template::Program::Ballista);
    let key = format!("kind={kind} op={opcode:?}{}", if structural { " STRUCTURAL" } else { "" });
    *CRITIC_KINDS.lock().unwrap().entry(key.clone()).or_default() += 1;
    if structural {
        CRITIC_STRUCTURAL.lock().unwrap().push(format!(
            "seed {seed}: {key} pc={context} failing_ix={index} before={} wrap={:?} mutation={:?} calls_ballista={calls_ballista} calls={:?}",
            scenario.before.len(), scenario.wrap, scenario.mutation, plan.calls
        ));
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

/// One seed's outcome: its hard and soft findings (each prefixed with the seed), and whether the
/// generator fell back to the minimal template.
struct SeedResult {
    hard: Vec<String>,
    soft: Vec<String>,
    fell_back: bool,
    stats: Stats,
}

/// What one or many runs did, for the report: how often the oracles actually had something to check.
#[derive(Default, Clone, Copy)]
struct Stats {
    succeeded: usize,
    failed: usize,
    ballista_codes: usize,
    model_compared: usize,
    cpis_compared: usize,
    runs_with_cpis: usize,
    multi_open_successes: usize,
    registry_entry_rejections: usize,
}

impl Stats {
    fn add(&mut self, other: Stats) {
        self.succeeded += other.succeeded;
        self.failed += other.failed;
        self.ballista_codes += other.ballista_codes;
        self.model_compared += other.model_compared;
        self.cpis_compared += other.cpis_compared;
        self.runs_with_cpis += other.runs_with_cpis;
        self.multi_open_successes += other.multi_open_successes;
        self.registry_entry_rejections += other.registry_entry_rejections;
    }
}

/// Runs one seed end to end.
fn run_seed(harness: &Harness, seed: u64) -> SeedResult {
    harness.reset();
    let world = super::harness::world();
    let mut source = SplitMix64::new(seed);
    let mut gen = Gen::new(&mut source);
    let plan = generate_template(&mut gen, &world);
    let fell_back = plan.fell_back;

    // A fixed creator and id: every seed runs in its own stateless Mollusk invocation, so there is
    // no collision between seeds.
    let creator = Pubkey::new_from_array([0x11; 32]);
    let template_id = 1u16;
    let (template, finalized) = match harness.upload(&creator, template_id, &plan.bytes) {
        Ok(uploaded) => uploaded,
        Err(error) => {
            return SeedResult {
                hard: vec![format!("seed {seed}: a generated template failed to upload: {error}")],
                soft: Vec::new(),
                fell_back,
                stats: Stats::default(),
            };
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
    critic_classify(seed, &plan, &scenario, &outcome);

    let succeeded = outcome.result.program_result.is_ok();
    let code = match &outcome.result.program_result {
        mollusk_svm::result::types::TransactionProgramResult::Failure(_, solana_program_error::ProgramError::Custom(code)) => Some(code & 0xffff),
        _ => None,
    };
    let stats = Stats {
        succeeded: usize::from(succeeded),
        failed: usize::from(!succeeded),
        ballista_codes: usize::from(code.is_some_and(|kind| (6000..=6132).contains(&kind))),
        model_compared: usize::from(report.model_compared),
        cpis_compared: report.cpis_compared,
        runs_with_cpis: usize::from(!super::harness::captured_cpis(&outcome).is_empty()),
        multi_open_successes: usize::from(succeeded && plan.opens.len() >= 2),
        registry_entry_rejections: usize::from(code == Some(6025)),
    };
    let prefix = |messages: Vec<String>| messages.into_iter().map(|m| format!("seed {seed}: {m}")).collect();
    SeedResult { hard: prefix(report.hard), soft: prefix(report.soft), fell_back, stats }
}

#[test]
fn fuzz_executor_differential() {
    assert_eq!(super::harness::BALLISTA_ID, BALLISTA_ID);
    let harness = Harness::new();
    if !harness.has_probe {
        eprintln!(
            "note: probe program not built; CPI coverage is reduced. Build it with\n  \
             cargo build-sbf --manifest-path fuzz-executor/probe/Cargo.toml"
        );
    }

    let cases = env_u64("FV_CASES", 1500);
    let start = env_u64("FV_START", 0);
    if let Ok(seed) = std::env::var("FV_SEED") {
        let seed: u64 = seed.parse().expect("FV_SEED must be a number");
        let result = run_seed(&harness, seed);
        for message in &result.soft {
            eprintln!("soft: {message}");
        }
        assert!(result.hard.is_empty(), "hard findings:\n{}", result.hard.join("\n"));
        return;
    }

    let mut hard = Vec::new();
    let mut soft_count = 0usize;
    let mut soft_samples = Vec::new();
    let mut fallbacks = 0usize;
    let mut stats = Stats::default();
    for offset in 0..cases {
        let seed = start.wrapping_add(offset);
        let result = run_seed(&harness, seed);
        stats.add(result.stats);
        hard.extend(result.hard);
        soft_count += result.soft.len();
        fallbacks += usize::from(result.fell_back);
        for message in result.soft {
            if soft_samples.len() < 20 {
                soft_samples.push(message);
            }
        }
        if hard.len() > 40 {
            break;
        }
    }

    eprintln!("ran {cases} cases from seed {start}; {soft_count} soft model divergences; {fallbacks} generator fallbacks");
    eprintln!(
        "outcomes: {} succeeded, {} failed ({} with a Ballista code, {} InvalidRegistryEntry); \
         {} runs made CPIs; model compared {} runs and {} CPIs; {} runs with 2+ opens succeeded",
        stats.succeeded,
        stats.failed,
        stats.ballista_codes,
        stats.registry_entry_rejections,
        stats.runs_with_cpis,
        stats.model_compared,
        stats.cpis_compared,
        stats.multi_open_successes,
    );
    for sample in &soft_samples {
        eprintln!("soft: {sample}");
    }
    // Kinds raised while instructions run, by the opcode at the code's pc. Account and input
    // validation kinds (6000-6008, 6010, 6020) carry an index, not a pc, and are left out.
    for (key, count) in CRITIC_KINDS.lock().unwrap().iter() {
        let kind: u32 = key[5..9].parse().unwrap_or(0);
        if !matches!(kind, 6000..=6008 | 6010 | 6020) {
            eprintln!("in-run failure {key}: {count}");
        }
    }
    let structural = CRITIC_STRUCTURAL.lock().unwrap();
    eprintln!("structural failures of verified templates: {}", structural.len());
    for line in structural.iter().take(20) {
        eprintln!("structural: {line}");
    }
    assert!(
        structural.is_empty(),
        "verified templates failed with structural errors (replay with FV_SEED=<n>):\n{}",
        structural.join("\n")
    );
    assert!(
        hard.is_empty(),
        "hard invariant violations (replay with FV_SEED=<n>):\n{}",
        hard.join("\n")
    );
}
