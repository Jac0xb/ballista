//! The compiler fuzzer's corpus, run in Mollusk.
//!
//! `clients/js/src/compiler-fuzz.test.ts` generates template documents from seeds, with the
//! accounts and inputs to run each one, and compiles each three ways: naturally, with register
//! reuse forced (when the natural compile does not reuse registers already), and with every `let`
//! alias materialized into a register of its own. For each case this test:
//!
//! 1. verifies every payload, and uploads it, so the chain's own finalization accepts it too;
//! 2. runs each program in a fresh copy of the same world, in a transaction whose first
//!    instruction is a memo, so introspection has something to read;
//! 3. requires the forced-reuse run to match the natural run exactly: the result and error code,
//!    every account, the return data, the CPIs made and the logs (the failure log's register
//!    operands aside, since they name registers);
//! 4. requires the materialized run to match too, except that program counters may differ, so a
//!    failure is compared by its kind and the step it happened in;
//! 5. requires the natural run not to fail with a structural error, which the verifier exists to
//!    rule out.
//!
//! A large corpus runs the same way:
//!
//! ```text
//! FUZZ_SEEDS=5000 FUZZ_CORPUS=/tmp/compiler-corpus.json pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts
//! pnpm build:program
//! COMPILER_FUZZ_CORPUS=/tmp/compiler-corpus.json cargo test --manifest-path tests/ballista/Cargo.toml compiler_fuzz -- --nocapture
//! ```

use super::*;
use serde_json::Value;

const COMMITTED: &str = include_str!("../../../../fixtures/compiler-fuzz-corpus.json");
const TEMPLATE_ID: u16 = 7;
/// Runtime errors a verified program must never reach: an invalid program, an unset or out-of-range
/// register, a type mismatch, or CPI data longer than the verifier computed.
const STRUCTURAL: [u32; 4] = [6002, 6011, 6012, 6016];

type Context = MolluskContext<HashMap<Pubkey, Account>>;

/// What one run did, as far as a template can make it observable.
#[derive(Debug, PartialEq)]
struct Outcome {
    /// `Success`, or the failing instruction and error, with a Ballista code split into kind and
    /// context.
    result: String,
    code: Option<u32>,
    return_data: Vec<u8>,
    /// Every account the transaction names, after the run, the template's own aside: its data is
    /// the payload, which differs between the programs compared.
    accounts: Vec<(Pubkey, Account)>,
    inner_instructions: String,
    logs: Vec<String>,
}

fn decode(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn pubkey(value: &str) -> Pubkey {
    Pubkey::new_from_array(decode(value).try_into().expect("32 bytes"))
}

/// The runtime accounts of `case`, and the accounts the world must store before the run.
fn world(case: &Value, template: &Pubkey) -> (Vec<AccountMeta>, Vec<(Pubkey, Account)>) {
    let mut metas = Vec::new();
    let mut stored = Vec::new();
    for entry in case["accounts"].as_array().expect("accounts") {
        let address = if let Some(program) = entry["program"].as_str() {
            match program {
                "system" => system_program::id(),
                "memo" => memo::ID,
                "token" => token::ID,
                other => panic!("unknown program {other}"),
            }
        } else if entry.get("sysvar").is_some() {
            sysvar::instructions::id()
        } else if let Some(registry) = entry.get("registry") {
            let index = registry["index"].as_u64().expect("index") as u8;
            let key = pubkey(registry["key"].as_str().expect("key"));
            Pubkey::find_program_address(
                &[b"registry", template.as_ref(), &[index], key.as_ref()],
                &ID,
            )
            .0
        } else {
            let address = pubkey(entry["address"].as_str().expect("address"));
            stored.push((
                address,
                Account {
                    lamports: entry["lamports"]
                        .as_str()
                        .expect("lamports")
                        .parse()
                        .expect("u64"),
                    data: decode(entry["data"].as_str().expect("data")),
                    owner: pubkey(entry["owner"].as_str().expect("owner")),
                    executable: false,
                    rent_epoch: 0,
                },
            ));
            address
        };
        let signer = entry["signer"].as_bool().expect("signer");
        metas.push(if entry["writable"].as_bool().expect("writable") {
            AccountMeta::new(address, signer)
        } else {
            AccountMeta::new_readonly(address, signer)
        });
    }
    (metas, stored)
}

/// Drops what legitimately differs between two programs that compute the same thing: compute
/// units, and, when `exact_pcs` is off, the failure lines that carry a program counter. The
/// failure log `0xpc, 0xopcode, 0xa, 0xb, 0xdst` names registers, so only its first two words are
/// kept even then.
fn normalize_logs(lines: &[String], exact_pcs: bool) -> Vec<String> {
    lines
        .iter()
        .filter(|line| !line.contains(" compute units"))
        .filter_map(|line| {
            let words: Vec<&str> = line
                .strip_prefix("Program log: ")
                .map(|rest| rest.split(", ").collect())
                .unwrap_or_default();
            let failure_words = words.len() == 5 && words.iter().all(|word| word.starts_with("0x"));
            if failure_words {
                return exact_pcs.then(|| format!("failure at {} opcode {}", words[0], words[1]));
            }
            if !exact_pcs && line.contains("failed: custom program error") {
                return None;
            }
            Some(line.clone())
        })
        .collect()
}

/// Uploads `payload` into a fresh world and runs it.
fn run_variant(context: &mut Context, case: &Value, payload: &[u8]) -> Result<Outcome, String> {
    let creator = Pubkey::new_from_array([0xc7; 32]);
    let (template, _) = find_template_pda(&creator, TEMPLATE_ID);
    let (metas, stored) = world(case, &template);
    let mut accounts: HashMap<Pubkey, Account> = stored.into_iter().collect();
    accounts.insert(
        creator,
        Account::new(100_000_000_000, 0, &system_program::id()),
    );
    *context.account_store.borrow_mut() = accounts;
    context.mollusk.logger = None;
    let created =
        context.process_instruction(&create_template_instruction(creator, TEMPLATE_ID, payload));
    if created.program_result.is_err() {
        return Err(format!(
            "the upload was refused: {:?}",
            created.program_result
        ));
    }
    let logger = LogCollector::new_ref();
    context.mollusk.logger = Some(logger.clone());
    let memo = Instruction {
        program_id: memo::ID,
        accounts: vec![],
        data: b"compiler-fuzz".to_vec(),
    };
    let data = decode(case["data"].as_str().expect("data"));
    let run = run_instruction(template, metas, &data);
    let result = context.process_transaction_instructions(&[memo, run]);
    context.mollusk.logger = None;
    let code = match &result.program_result {
        TransactionProgramResult::Failure(_, solana_program_error::ProgramError::Custom(code)) => {
            Some(*code)
        }
        _ => None,
    };
    let mut accounts: Vec<(Pubkey, Account)> = result
        .resulting_accounts
        .iter()
        .filter(|(address, _)| *address != template)
        .cloned()
        .collect();
    accounts.sort_by_key(|(address, _)| *address);
    let logs = logger.borrow().get_recorded_content().to_vec();
    Ok(Outcome {
        result: format!("{:?}", result.program_result),
        code,
        return_data: result.return_data.clone(),
        accounts,
        inner_instructions: format!("{:?}", result.inner_instructions),
        logs,
    })
}

/// Whether the instruction at `pc` reads a `bool` from bytes: account data, instruction data,
/// return data or a registry field. Such a read fails with `TypeMismatch` when the byte is neither
/// 0 nor 1, which the language documents; it is the one value-dependent `TypeMismatch`.
fn reads_a_bool(payload: &[u8], pc: usize) -> bool {
    use ballista_common::template::{
        OP_READ_BOOL, OP_READ_INSTRUCTION_DATA, OP_READ_REGISTRY, OP_RETURN_DATA,
    };
    let program = ProgramView::parse(payload).expect("the payload parses");
    let Some(record) = program.instructions.get(pc) else {
        return false;
    };
    match record.opcode {
        OP_READ_BOOL => true,
        OP_READ_INSTRUCTION_DATA => record.immediate() == u64::from(OP_READ_BOOL),
        OP_RETURN_DATA => record.a == OP_READ_BOOL,
        OP_READ_REGISTRY => (record.immediate() >> 16) as u8 == OP_READ_BOOL,
        _ => false,
    }
}

/// The step a failure code names: its kind, and the source path of the program counter in its
/// context bits when there is one.
fn failure_step(code: u32, paths: &[String]) -> (u32, String) {
    let context = (code >> 16) as usize;
    let place = paths
        .get(context)
        .cloned()
        .unwrap_or_else(|| format!("context {context}"));
    (code & 0xffff, place)
}

/// The step path of each program counter of `payload`, from the variant's run-length source map.
fn paths(variant: &Value, payload: &[u8]) -> Vec<String> {
    let runs: Vec<(usize, &str)> = variant["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .map(|run| {
            (
                run[0].as_u64().expect("pc") as usize,
                run[1].as_str().expect("path"),
            )
        })
        .collect();
    (0..payload[10] as usize)
        .map(|pc| {
            runs.iter()
                .rev()
                .find(|(start, _)| *start <= pc)
                .map_or(String::new(), |(_, path)| (*path).to_owned())
        })
        .collect()
}

/// Differences between two outcomes, as messages; `exact_pcs` off compares failures by step.
fn differences(
    left: &Outcome,
    right: &Outcome,
    left_paths: &[String],
    right_paths: &[String],
    exact_pcs: bool,
) -> Vec<String> {
    let mut found = Vec::new();
    match (left.code, right.code) {
        (Some(left_code), Some(right_code)) if !exact_pcs => {
            let (left_step, right_step) = (
                failure_step(left_code, left_paths),
                failure_step(right_code, right_paths),
            );
            if left_step != right_step {
                found.push(format!("failure {left_step:?} vs {right_step:?}"));
            }
        }
        _ => {
            if left.result != right.result {
                found.push(format!("result {} vs {}", left.result, right.result));
            }
        }
    }
    if left.return_data != right.return_data {
        found.push(format!(
            "return data {:?} vs {:?}",
            left.return_data, right.return_data
        ));
    }
    if left.accounts != right.accounts {
        let changed: Vec<String> = left
            .accounts
            .iter()
            .zip(&right.accounts)
            .filter(|(a, b)| a != b)
            .map(|((address, a), (_, b))| {
                format!(
                    "{address}: {} lamports {:?} vs {} lamports {:?}",
                    a.lamports, a.data, b.lamports, b.data
                )
            })
            .collect();
        found.push(format!("accounts differ: {}", changed.join("; ")));
    }
    if left.inner_instructions != right.inner_instructions {
        found.push(format!(
            "CPIs {} vs {}",
            left.inner_instructions, right.inner_instructions
        ));
    }
    let (left_logs, right_logs) = (
        normalize_logs(&left.logs, exact_pcs),
        normalize_logs(&right.logs, exact_pcs),
    );
    if left_logs != right_logs {
        found.push(format!("logs {left_logs:#?} vs {right_logs:#?}"));
    }
    found
}

#[derive(Default)]
struct Tally {
    cases: usize,
    payloads: usize,
    runs: usize,
    succeeded: usize,
    failures: std::collections::BTreeMap<String, usize>,
    forced_compared: usize,
    materialized_compared: usize,
    known: std::collections::BTreeMap<String, usize>,
    findings: Vec<String>,
}

fn run_corpus(corpus: &str) -> Tally {
    let document: Value = serde_json::from_str(corpus).expect("the corpus is JSON");
    let mut tally = Tally::default();
    let mut context = context(HashMap::new());
    for case in document["cases"].as_array().expect("cases") {
        tally.cases += 1;
        let seed = case["seed"].as_u64().expect("seed");
        let hazards: Vec<&str> = case["hazards"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut outcomes = Vec::new();
        for name in ["natural", "forced", "materialized"] {
            let Some(variant) = case.get(name) else {
                continue;
            };
            let payload = decode(variant["payload"].as_str().expect("payload"));
            tally.payloads += 1;
            if let Err(error) = ProgramView::parse(&payload).and_then(|program| program.verify()) {
                tally.findings.push(format!(
                    "seed {seed} {name}: the verifier rejects the compiled payload: {error:?}"
                ));
                continue;
            }
            match run_variant(&mut context, case, &payload) {
                Ok(outcome) => {
                    tally.runs += 1;
                    outcomes.push((name, outcome, paths(variant, &payload)));
                }
                Err(error) => tally.findings.push(format!("seed {seed} {name}: {error}")),
            }
        }
        let Some((_, natural, natural_paths)) =
            outcomes.iter().find(|(name, ..)| *name == "natural")
        else {
            continue;
        };
        if std::env::var("COMPILER_FUZZ_VERBOSE").is_ok() {
            let failure = natural.code.map(|code| failure_step(code, natural_paths));
            eprintln!("seed {seed}: {} {failure:?}", natural.result);
        }
        match natural.code {
            None if natural.result == "Success" => tally.succeeded += 1,
            None => *tally.failures.entry(natural.result.clone()).or_default() += 1,
            Some(code) => {
                let kind = code & 0xffff;
                *tally.failures.entry(format!("custom {kind}")).or_default() += 1;
                let payload = decode(case["natural"]["payload"].as_str().expect("payload"));
                if STRUCTURAL.contains(&kind)
                    && natural.result.contains("Failure(1,")
                    && !reads_a_bool(&payload, (code >> 16) as usize)
                {
                    tally.findings.push(format!(
                        "seed {seed}: a verified program failed with structural error {kind} at {:?}",
                        failure_step(code, natural_paths)
                    ));
                }
            }
        }
        for (name, outcome, other_paths) in &outcomes {
            let exact = match *name {
                "forced" => true,
                "materialized" => false,
                _ => continue,
            };
            if exact {
                tally.forced_compared += 1;
            } else {
                tally.materialized_compared += 1;
            }
            let found = differences(natural, outcome, natural_paths, other_paths, exact);
            if found.is_empty() {
                continue;
            }
            // A known finding, with its own ignored regression test: counted, not failed.
            if !exact && hazards.contains(&"carried-alias") {
                *tally.known.entry("carried-alias".to_owned()).or_default() += 1;
                continue;
            }
            tally.findings.push(format!(
                "seed {seed}: natural and {name} runs differ: {}",
                found.join("; ")
            ));
        }
    }
    tally
}

#[test]
fn compiler_fuzz_corpus_runs_the_same_with_and_without_register_reuse() {
    let external = std::env::var("COMPILER_FUZZ_CORPUS").ok().map(|path| {
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
    });
    let tally = run_corpus(external.as_deref().unwrap_or(COMMITTED));
    eprintln!(
        "compiler fuzz corpus: {} cases, {} payloads verified and uploaded, {} runs; natural runs: {} succeeded, failures {:?}; {} forced-reuse and {} materialized comparisons; known findings {:?}",
        tally.cases,
        tally.payloads,
        tally.runs,
        tally.succeeded,
        tally.failures,
        tally.forced_compared,
        tally.materialized_compared,
        tally.known,
    );
    assert!(
        tally.cases > 0 && tally.forced_compared > 0,
        "the corpus compares nothing"
    );
    assert!(
        tally.findings.is_empty(),
        "{} findings:\n{}",
        tally.findings.len(),
        tally
            .findings
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

const FINDINGS: &str = include_str!("../../../../fixtures/compiler-fuzz-findings.json");

/// Finding `carried-alias` (`carriedAliasDocument` in `clients/js/src/compiler-fuzz.test.ts`):
/// inside a loop body, a `snapshot` of a carried variable shares the variable's register, so the
/// `assign` after it changes the snapshot too. Each pass of the minimized template snapshots the
/// total, adds one, and requires that the total grew by one from the snapshot. By the language's
/// rules a run of two passes succeeds and returns 2; the compiled program compares the new total
/// plus one with itself and fails the first pass with `RequirementFailed`. With the fix,
/// regenerate the fixture (`UPDATE_COMPILER_FUZZ_CORPUS=1`) and remove the `ignore`.
#[test]
#[ignore = "compiler bug carried-alias: a let of a carried variable in a loop body aliases its register"]
fn a_snapshot_of_a_carried_value_keeps_its_value_after_the_assignment() {
    let findings: Value = serde_json::from_str(FINDINGS).expect("the findings fixture is JSON");
    let finding = &findings["carried-alias"];
    let payload = decode(finding["payload"].as_str().expect("payload"));
    let creator = Pubkey::new_unique();
    let context = context(funded_accounts([creator], 10_000_000_000));
    let created = context.process_instruction(&create_template_instruction(creator, 1, &payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    let (template, _) = find_template_pda(&creator, 1);
    let data = decode(finding["data"].as_str().expect("data"));
    let result = context.process_instruction(&run_instruction(template, vec![], &data));
    let failure = custom_code(&result).map(|code| (code >> 16, code & 0xffff));
    assert!(
        result.program_result.is_ok(),
        "each pass grows the total by one from its snapshot, yet the run failed: (pc, kind) {failure:?}"
    );
    assert_eq!(
        result.return_data,
        decode(finding["returns"].as_str().expect("returns"))
    );
}
