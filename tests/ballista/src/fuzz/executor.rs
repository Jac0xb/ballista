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

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

/// One seed's outcome: its hard and soft findings (each prefixed with the seed), and whether the
/// generator fell back to the minimal template.
struct SeedResult {
    hard: Vec<String>,
    soft: Vec<String>,
    fell_back: bool,
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

    let prefix = |messages: Vec<String>| messages.into_iter().map(|m| format!("seed {seed}: {m}")).collect();
    SeedResult { hard: prefix(report.hard), soft: prefix(report.soft), fell_back }
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
    for offset in 0..cases {
        let seed = start.wrapping_add(offset);
        let result = run_seed(&harness, seed);
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
    for sample in &soft_samples {
        eprintln!("soft: {sample}");
    }
    assert!(
        hard.is_empty(),
        "hard invariant violations (replay with FV_SEED=<n>):\n{}",
        hard.join("\n")
    );
}
