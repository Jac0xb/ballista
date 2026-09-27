//! The Rust-authored protocol templates are byte-identical to what the TypeScript compiler
//! produced, and the Rust runs pass the accounts and inputs those templates declare.

#[path = "../examples/protocol_templates.rs"]
mod templates;
#[path = "../examples/protocol_templates_run.rs"]
mod runs;

use ballista_sdk::ballista_common::template::*;

const FIXTURE: &str = include_str!("../../../fixtures/protocol-examples.json");

/// `{ "name": "hex", ... }`, without pulling in a JSON parser.
fn fixture() -> Vec<(String, Vec<u8>)> {
    FIXTURE
        .split('"')
        .collect::<Vec<_>>()
        .chunks(4)
        .filter(|chunk| chunk.len() == 4)
        .map(|chunk| (chunk[1].to_string(), decode_hex(chunk[3])))
        .collect()
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
        return format!("headers differ:\n  expected {:?}\n  actual   {:?}", expected.header, actual.header);
    }
    compare!(accounts, inputs, instructions, cpis, cpi_accounts, data_segments, pubkeys);
    if expected.blob != actual.blob {
        return format!("blobs differ:\n  expected {:?}\n  actual   {:?}", expected.blob, actual.blob);
    }
    "no table differs".into()
}

#[test]
fn rust_templates_match_the_typescript_fixture_byte_for_byte() {
    let fixture = fixture();
    assert_eq!(fixture.len(), templates::TEMPLATES.len(), "one Rust template per fixture entry");
    let mut failures = Vec::new();
    for (name, build) in templates::TEMPLATES {
        let expected = &fixture
            .iter()
            .find(|(fixture_name, _)| fixture_name == name)
            .unwrap_or_else(|| panic!("{name} is not in the fixture"))
            .1;
        let actual = build();
        if &actual != expected {
            failures.push(format!("{name}: {}", first_difference(expected, &actual)));
            continue;
        }
        ProgramView::parse(&actual).unwrap().verify().unwrap();
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// Each Rust run passes the template account, then one meta per declared account with the
/// declared signer and writable flags, then the group members; and encodes one value per
/// declared input, after one length byte per group.
#[test]
fn rust_runs_match_the_accounts_and_inputs_each_template_declares() {
    assert_eq!(runs::RUNS.len(), templates::TEMPLATES.len(), "one run per template");
    for (name, run) in runs::RUNS {
        let build = templates::TEMPLATES
            .iter()
            .find(|(template, _)| *template == name)
            .unwrap_or_else(|| panic!("{name} has no Rust template"))
            .1;
        let payload = build();
        let program = ProgramView::parse(&payload).unwrap();
        let header = program.header;
        let instruction = run();
        let groups = header.account_group_count();
        let group_lengths: Vec<usize> =
            instruction.data[1..1 + groups].iter().map(|&len| len as usize).collect();
        let fixed = header.fixed_account_count();
        let stride = header.batch_stride();
        let group_total: usize = group_lengths.iter().sum();
        let row_accounts = instruction.accounts.len() - 1 - fixed - group_total;
        assert_eq!(row_accounts % stride.max(1), 0, "{name}: rows are whole");
        if stride == 0 {
            assert_eq!(row_accounts, 0, "{name}: account count");
        }
        let rows = row_accounts / stride.max(1);

        assert_eq!(instruction.accounts[0].pubkey, runs::TEMPLATE, "{name}: template first");
        let declared = program.accounts[..fixed]
            .iter()
            .chain((0..rows).flat_map(|_| program.accounts[fixed..fixed + stride].iter()));
        for (position, (meta, constraint)) in
            instruction.accounts[1..].iter().zip(declared).enumerate()
        {
            assert_eq!(
                meta.is_signer,
                constraint.flags & ACCOUNT_SIGNER != 0,
                "{name}: signer flag of account {position}"
            );
            assert_eq!(
                meta.is_writable,
                constraint.flags & ACCOUNT_WRITABLE != 0,
                "{name}: writable flag of account {position}"
            );
        }

        // Walk the input values the template declares and check they fill the data exactly.
        let mut cursor = 1 + groups;
        for input in program.inputs.iter().take(header.input_count()) {
            cursor += match input.value_type {
                VALUE_BOOL => 1,
                VALUE_U64 | VALUE_I64 => 8,
                VALUE_U128 => 16,
                VALUE_PUBKEY => 32,
                VALUE_BYTES => {
                    let len = u16::from_le_bytes([instruction.data[cursor], instruction.data[cursor + 1]]);
                    assert!(len <= u16::from_le_bytes(input.max_len_le), "{name}: bytes input too long");
                    2 + len as usize
                }
                other => panic!("{name}: unexpected input type {other}"),
            };
        }
        assert_eq!(cursor, instruction.data.len(), "{name}: input bytes");
    }
}
