//! Every rule the specification defines sits in exactly one prover configuration, and every rule a
//! configuration names is defined. A rule left out of every configuration never runs; a name that
//! matches no rule makes the prover job fail before it analyzes anything.
//!
//! Rules are found in the source: each `pub fn rule_*`, and each rule name a `u128_rules!`
//! invocation generates. Run: `cargo test -p ballista-specs --features rt --test confs`

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Every `rule_*` identifier in `text` that starts at a word boundary.
fn rule_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(found) = text[index..].find("rule_") {
        let start = index + found;
        let boundary = start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if boundary {
            names.push(text[start..end].to_string());
        }
        index = end.max(start + 1);
    }
    names
}

/// The rules the source defines.
fn defined_rules() -> Vec<String> {
    let mut rules = Vec::new();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rules");
    for entry in fs::read_dir(directory).expect("rules directory") {
        let text = fs::read_to_string(entry.expect("entry").path()).expect("source file");
        for line in text.lines() {
            if let Some(rest) = line.trim_start().strip_prefix("pub fn ") {
                if rest.starts_with("rule_") {
                    rules.extend(rule_names(rest).into_iter().take(1));
                }
            }
        }
        // Rules a macro generates: every name inside a `u128_rules!(...)` invocation.
        let mut rest = text.as_str();
        while let Some(start) = rest.find("\nu128_rules!(") {
            let invocation = &rest[start..];
            let end = invocation.find(");").expect("invocation ends");
            rules.extend(rule_names(&invocation[..end]));
            rest = &invocation[end..];
        }
    }
    rules.sort();
    rules
}

#[test]
fn every_rule_runs_in_exactly_one_configuration() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut placed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in fs::read_dir(directory).expect("crate directory") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|extension| extension == "conf") {
            let text = fs::read_to_string(&path).expect("configuration");
            let rules = &text[text.find("\"rule\"").expect("a rule list")..];
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            for rule in rule_names(rules) {
                placed.entry(rule).or_default().push(name.clone());
            }
        }
    }
    let defined = defined_rules();
    assert!(defined.len() > 100, "found only {} rules", defined.len());
    for rule in &defined {
        match placed.get(rule).map(Vec::as_slice) {
            Some([_]) => {}
            Some(confs) => panic!("{rule} is in {} configurations: {confs:?}", confs.len()),
            None => panic!("{rule} is in no configuration"),
        }
    }
    for rule in placed.keys() {
        assert!(defined.contains(rule), "a configuration names {rule}, which no source defines");
    }
}

/// Rules the prover is known to fail on its own model, not on the program (certora/README.md,
/// "Prover results at cb2fb2d"). Each must sit only in a conf the workflow runs `--report-only`,
/// so the gating jobs never wait on a verdict that cannot come.
const PROVER_ARTIFACTS: [&str; 2] = [
    // A copied word is never rebuilt from two four-byte stores: violated by design.
    "rule_stack_word_copy_keeps_both_halves",
    // Limb adds have no 64-bit wraparound in the model, so the overflow branch is unreachable.
    "rule_u128_add_reaches_overflow",
];

/// The confs `.github/workflows/certora.yml` checks with `--report-only`, read from its matrix.
fn report_only_confs() -> Vec<String> {
    let workflow = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/certora.yml");
    let text = fs::read_to_string(workflow).expect("certora workflow");
    let mut confs = Vec::new();
    let mut conf: Option<&str> = None;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("- conf: ") {
            conf = Some(name);
        } else if let (Some(name), Some(check)) = (conf, line.strip_prefix("check: ")) {
            if check == "--report-only" {
                confs.push(name.to_string());
            }
            conf = None;
        }
    }
    confs
}

#[test]
fn known_prover_artifacts_only_report() {
    let report_only = report_only_confs();
    assert!(report_only.contains(&"run-blocked.conf".to_string()), "report-only confs: {report_only:?}");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in fs::read_dir(directory).expect("crate directory") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|extension| extension != "conf") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(&path).expect("configuration");
        let rules = rule_names(&text[text.find("\"rule\"").expect("a rule list")..]);
        for artifact in PROVER_ARTIFACTS {
            if rules.iter().any(|rule| rule == artifact) {
                assert!(report_only.contains(&name), "{artifact} is in {name}, which gates");
            }
        }
    }
}
