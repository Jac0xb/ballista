//! The guide and example pages show every template in TypeScript and in Rust. These tests hold the
//! two to the same bytes, and the Rust runs to the same accounts and run data as the TypeScript
//! runs.
//!
//! Most of the Rust is in `examples/docs_templates.rs` and `examples/docs_runs.rs`. A few pages keep
//! theirs with the rest of the page's Rust: the registries page in `docs_language.rs`, the ATA
//! assertion in `docs_security.rs`, and the input encoding in `docs_limits.rs`.
//!
//! Expected values come from the TypeScript side: `fixtures/benchmarks.json` (written by
//! `clients/js/src/benchmarks.test.ts`) for the examples the benchmarks measure, and
//! `tests/fixtures/docs-examples.json` (written by `clients/js/examples/docs/docs-examples.test.ts`)
//! for the examples only the guide pages use. Both record, per example, the compiled template, the
//! encoded run data, the signer and writable flags of every runtime account, and the row count.

#[path = "../examples/docs_language.rs"]
mod language;
#[path = "../examples/docs_limits.rs"]
mod limits;
#[path = "../examples/docs_runs.rs"]
mod runs;
#[path = "../examples/docs_security.rs"]
mod security;
#[path = "../examples/docs_templates.rs"]
mod templates;

use ballista_sdk::{ballista_common::instruction::IX_RUN, ballista_common::template::*};

const BENCHMARKS: &str = include_str!("../../../fixtures/benchmarks.json");
const DOCS_ONLY: &str = include_str!("fixtures/docs-examples.json");

/// What the TypeScript side recorded for one example.
struct Expected {
    template: Vec<u8>,
    run_data: Vec<u8>,
    /// `(signer, writable)` for each runtime account, template account excluded.
    flags: Vec<(bool, bool)>,
    rows: usize,
}

/// Finds `"name": {` at the top level of either fixture and reads the fields this test needs. The
/// fixtures are machine-written JSON, so plain string search is enough.
fn expected(name: &str) -> Expected {
    let key = format!("\n  \"{name}\": {{");
    let (source, start) = [BENCHMARKS, DOCS_ONLY]
        .iter()
        .find_map(|source| source.find(&key).map(|start| (*source, start + key.len())))
        .unwrap_or_else(|| panic!("{name} is in neither fixture"));
    // The entry ends where the next top-level key (two-space indent) begins.
    let body = &source[start..];
    let end = body.find("\n  \"").unwrap_or(body.len());
    let body = &body[..end];

    let string_field = |field: &str| -> &str {
        let marker = format!("\"{field}\": \"");
        let from = body
            .find(&marker)
            .unwrap_or_else(|| panic!("{name}: no {field}"))
            + marker.len();
        &body[from..from + body[from..].find('"').unwrap()]
    };
    let number_field = |field: &str| -> usize {
        let marker = format!("\"{field}\": ");
        let from = body
            .find(&marker)
            .unwrap_or_else(|| panic!("{name}: no {field}"))
            + marker.len();
        let digits: String = body[from..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().unwrap()
    };
    let flags_from = body
        .find("\"runtimeAccountFlags\": [")
        .unwrap_or_else(|| panic!("{name}: no flags"));
    let flags_body = &body[flags_from..];
    let flags_body = &flags_body[..flags_body.find(']').unwrap()];
    let flags = flags_body
        .split('{')
        .skip(1)
        .map(|entry| {
            (
                entry.contains("\"signer\": true"),
                entry.contains("\"writable\": true"),
            )
        })
        .collect();

    Expected {
        template: decode_hex(string_field("templateHex")),
        run_data: decode_hex(string_field("runData")),
        flags,
        rows: number_field("rowCount"),
    }
}

fn decode_hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}

/// Names the first table that differs, so a mismatch points at the builder call to fix.
fn first_difference(expected: &[u8], actual: &[u8]) -> String {
    let (expected, actual) = match (ProgramView::parse(expected), ProgramView::parse(actual)) {
        (Ok(expected), Ok(actual)) => (expected, actual),
        (_, Err(error)) => return format!("Rust payload does not parse: {error:?}"),
        (Err(error), _) => return format!("fixture does not parse: {error:?}"),
    };
    macro_rules! compare {
        ($($table:ident),*) => {$(
            if expected.$table != actual.$table {
                let index = expected
                    .$table
                    .iter()
                    .zip(actual.$table.iter())
                    .position(|(left, right)| left != right);
                return format!(
                    "{} differ at {:?} (lengths {} vs {}):\n  expected {:?}\n  actual   {:?}",
                    stringify!($table),
                    index,
                    expected.$table.len(),
                    actual.$table.len(),
                    index.map(|i| expected.$table[i]),
                    index.map(|i| actual.$table[i]),
                );
            }
        )*};
    }
    if expected.header != actual.header {
        return format!(
            "headers differ:\n  expected {:?}\n  actual   {:?}",
            expected.header, actual.header
        );
    }
    compare!(
        accounts,
        inputs,
        instructions,
        cpis,
        cpi_accounts,
        data_segments,
        pubkeys
    );
    if expected.blob != actual.blob {
        return format!(
            "blobs differ:\n  expected {:?}\n  actual   {:?}",
            expected.blob, actual.blob
        );
    }
    "no table differs".into()
}

/// Every Rust template a page shows beside a TypeScript one.
fn all_templates() -> impl Iterator<Item = &'static templates::Example> {
    templates::ALL
        .iter()
        .chain(language::TEMPLATES)
        .chain(security::TEMPLATES)
}

/// Every Rust run a page shows beside a TypeScript one.
fn all_runs() -> impl Iterator<Item = &'static runs::ExampleRun> {
    runs::ALL.iter().chain(language::RUNS).chain(security::RUNS)
}

#[test]
fn every_rust_template_matches_the_typescript_compiler_byte_for_byte() {
    let mut failures = Vec::new();
    for (name, build) in all_templates() {
        let expected = expected(name).template;
        let actual = build();
        if actual != expected {
            failures.push(format!("{name}: {}", first_difference(&expected, &actual)));
            continue;
        }
        ProgramView::parse(&actual).unwrap().verify().unwrap();
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn every_rust_run_matches_the_typescript_run() {
    assert_eq!(
        runs::ALL.len(),
        templates::ALL.len(),
        "one run per template"
    );
    let mut failures = Vec::new();
    for (name, run) in all_runs() {
        assert!(
            all_templates().any(|(template, _)| template == name),
            "{name} has no Rust template"
        );
        let expected = expected(name);
        let instruction = run(expected.rows);

        let mut data = vec![IX_RUN];
        data.extend_from_slice(&expected.run_data);
        if instruction.data != data {
            failures.push(format!(
                "{name}: run data\n  expected {:02x?}\n  actual   {:02x?}",
                data, instruction.data
            ));
        }
        if instruction.program_id != ballista_sdk::ID {
            failures.push(format!("{name}: program id"));
        }
        let flags: Vec<(bool, bool)> = instruction.accounts[1..]
            .iter()
            .map(|meta| (meta.is_signer, meta.is_writable))
            .collect();
        if flags != expected.flags {
            failures.push(format!(
                "{name}: account flags\n  expected {:?}\n  actual   {:?}",
                expected.flags, flags
            ));
        }
        if instruction.accounts[0].is_signer || instruction.accounts[0].is_writable {
            failures.push(format!("{name}: the template account is read-only"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// The expressions page shows only how a run encodes its inputs, with the same 40-byte route as the
/// TypeScript encoding.
#[test]
fn the_rust_input_encoding_matches_the_typescript_encoding() {
    let expected = expected("named-inputs").run_data;
    let actual = limits::encode_run_inputs(&[1; 40]);
    assert!(
        actual == expected,
        "run data\n  expected {expected:02x?}\n  actual   {actual:02x?}"
    );
}
