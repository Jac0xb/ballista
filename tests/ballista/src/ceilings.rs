//! Compute-unit ceilings: a ratchet that only ever goes down.
//!
//! `fixtures/cu-ceilings.json` records the best compute-unit figure each case has ever reached.
//! This test fails when a case costs more than its ceiling, so a change that makes the program
//! slower cannot land quietly. Running it with `UPDATE_BENCHMARKS=1` lowers a ceiling that an
//! improvement has beaten and adds any case that has none, but never raises one: a deliberate
//! regression has to be accepted by editing the file, which shows up in review.
#![cfg(test)]

use std::collections::BTreeMap;

use crate::cases::{cases, mollusk};

const CEILINGS: &str = include_str!("../../../fixtures/cu-ceilings.json");
/// Headroom under a ceiling before the test suggests lowering it. Codegen shifts a few units
/// either way between builds, so the ratchet ignores noise-sized wins.
const SLACK: u64 = 64;

#[test]
fn compute_units_stay_under_their_ceiling() {
    let ceilings: BTreeMap<String, u64> = serde_json::from_str(CEILINGS).expect("ceilings");
    let mollusk = mollusk();
    let mut measured: BTreeMap<String, u64> = BTreeMap::new();
    let mut over: Vec<String> = Vec::new();
    let mut under: Vec<String> = Vec::new();

    for (name, case) in cases() {
        let result = mollusk.process_instruction(&case.instruction, &case.accounts);
        assert!(
            result.program_result.is_ok(),
            "{name} does not run: {result:#?}"
        );
        let units = result.compute_units_consumed;
        measured.insert(name.to_string(), units);
        match ceilings.get(name) {
            Some(&ceiling) if units > ceiling => over.push(format!(
                "  {name}: {units} CU, ceiling {ceiling}, over by {}",
                units - ceiling
            )),
            Some(&ceiling) if ceiling - units >= SLACK => under.push(format!(
                "  {name}: {units} CU, ceiling {ceiling}, {} to spare",
                ceiling - units
            )),
            Some(_) => {}
            None => under.push(format!("  {name}: {units} CU, no ceiling recorded")),
        }
    }

    if std::env::var("UPDATE_BENCHMARKS").as_deref() == Ok("1") {
        // Ratchet: keep the lower of the two, so an accidental regression cannot raise a ceiling.
        let lowered: BTreeMap<String, u64> = measured
            .iter()
            .map(|(name, units)| {
                let ceiling = ceilings.get(name).copied().unwrap_or(u64::MAX).min(*units);
                (name.clone(), ceiling)
            })
            .collect();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/cu-ceilings.json");
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&lowered).unwrap()),
        )
        .expect("write ceilings");
        return;
    }

    if !under.is_empty() {
        eprintln!(
            "These cases beat their ceiling. Lock the win in with `pnpm cu:ceilings`:\n{}",
            under.join("\n")
        );
    }
    assert!(
        over.is_empty(),
        "compute units regressed against fixtures/cu-ceilings.json:\n{}\n\nIf the increase is \
         intended, raise the ceiling in that file in the same commit so the cost is reviewed.",
        over.join("\n")
    );
}
