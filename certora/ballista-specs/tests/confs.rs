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
