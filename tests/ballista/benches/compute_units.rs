//! Compute-unit benchmarks, in the shape the wider Solana ecosystem uses.
//!
//! This is `mollusk-svm-bencher`, the bencher that ships with Mollusk: each case is one
//! instruction against a fixed account set, and the run writes `benches/compute_units.md` with
//! the current numbers and the delta against the last committed run. Unlike
//! `fixtures/cu-profile.json`, which isolates the marginal cost of one feature at a time, this
//! table tracks whole instructions, so a change that moves any of these numbers shows up as a
//! diff in the benchmark file.
//!
//! The cases themselves live in `src/cases.rs`, shared with the ceiling test that fails when one
//! of them gets more expensive.
//!
//! Run it with `pnpm cu:bench`.
use ballista_integration_tests::cases::{cases, mollusk, Case};
use mollusk_svm_bencher::MolluskComputeUnitBencher;

/// One case as the JSON `agave-ledger-tool program run -i` reads.
fn trace_input(case: &Case) -> String {
    let byte_list = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| byte.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let accounts: Vec<String> = case
        .instruction
        .accounts
        .iter()
        .map(|meta| {
            let account = case
                .accounts
                .iter()
                .find(|(key, _)| *key == meta.pubkey)
                .map(|(_, account)| account.clone())
                .unwrap_or_default();
            format!(
                "{{\"key\":\"{}\",\"owner\":\"{}\",\"is_signer\":{},\"is_writable\":{},\"lamports\":{},\"data\":[{}]}}",
                meta.pubkey,
                account.owner,
                meta.is_signer,
                meta.is_writable,
                account.lamports,
                byte_list(&account.data),
            )
        })
        .collect();
    format!(
        "{{\"program_id\":\"{}\",\"accounts\":[{}],\"instruction_data\":[{}]}}",
        case.instruction.program_id,
        accounts.join(","),
        byte_list(&case.instruction.data),
    )
}

fn main() {
    let cases = cases();
    let mut mollusk = mollusk();

    // Fail loudly with the case name rather than a bare code from inside the bencher.
    for (name, case) in &cases {
        let result = mollusk.process_instruction(&case.instruction, &case.accounts);
        assert!(
            result.program_result.is_ok(),
            "{name} does not run: {result:#?}"
        );
    }

    // `DUMP_TRACE_INPUTS=1` writes each case in the shape `agave-ledger-tool program run -i`
    // expects, for anyone with a working build of that tool.
    if std::env::var("DUMP_TRACE_INPUTS").as_deref() == Ok("1") {
        let dir = std::path::Path::new("../../target/trace");
        std::fs::create_dir_all(dir).expect("trace directory");
        for (name, case) in &cases {
            let slug: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            std::fs::write(dir.join(format!("{slug}.json")), trace_input(case))
                .expect("write trace input");
        }
    }

    let mut bencher = MolluskComputeUnitBencher::new(mollusk)
        .must_pass(true)
        .out_dir("../../benches");
    for (name, case) in &cases {
        bencher = bencher.bench((name, &case.instruction, &case.accounts));
    }
    bencher.execute();
}
