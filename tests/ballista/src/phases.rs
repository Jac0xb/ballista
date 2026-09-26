//! Where a run's compute units actually go, phase by phase.
//!
//! This runs the `cu-profile` build of the program, which samples
//! `sol_remaining_compute_units` at each phase boundary and returns the samples as return data.
//! The first two samples are taken back to back, so the gap between them is the price of reading
//! the counter; every other interval has that subtracted.
//!
//! Build the instrumented ELF and run the table with:
//!
//! ```sh
//! cargo build-sbf --manifest-path programs/ballista/Cargo.toml --features cu-profile
//! cp target/deploy/ballista.so target/deploy/ballista-cu-profile.so
//! cargo build-sbf --manifest-path programs/ballista/Cargo.toml
//! cargo test --manifest-path tests/ballista/Cargo.toml --features cu-profile -- --nocapture phase
//! ```
#![cfg(all(test, feature = "cu-profile"))]

use std::collections::HashMap;

use ballista_common::instruction::{IX_CREATE_TEMPLATE, IX_RUN};
use ballista_common::template::{
    ProgramBuilder, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
    OP_ADD, OP_LTE, VALUE_U64,
};
use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk, MolluskContext};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::{pubkey, Pubkey};
use solana_sdk_ids::system_program;

const PROFILE_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista-cu-profile.so");
const CLEAN_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
const TEMPLATE_SEED: &[u8] = b"template";
const MAGIC: [u8; 4] = *b"BCU1";

/// The phases, in the order the program marks them. Each name describes the work that ended at
/// that mark.
const TAG_EXECUTED: u8 = 7;

const PHASES: [(u8, &str); 7] = [
    (2, "Instruction parse and dispatch"),
    (3, "Template account load and header parse"),
    (4, "Runtime account validation"),
    (5, "Run input parse"),
    (6, "Register file allocation"),
    (8, "Invocation scratch allocation"),
    (7, "Interpreter, including invokes"),
];

struct Profile {
    /// Units attributed to each phase, calibration removed.
    phases: Vec<(String, u64)>,
    /// Units the runtime and callees charged inside `invoke`, summed over every CPI.
    cpi_units: u64,
    cpi_count: u32,
    /// Units spent resolving accounts and encoding data for those invokes.
    setup_units: u64,
    /// The cost of one `sol_remaining_compute_units` call, measured in the run itself.
    calibration: u64,
    /// Everything the VM charged before the first mark: entrypoint and account deserialization.
    prologue: u64,
    /// What the same run costs on the ordinary build, with no instrumentation compiled in.
    clean_total: u64,
    total: u64,
    budget: u64,
}

fn decode(return_data: &[u8], consumed: u64, clean_total: u64, budget: u64) -> Profile {
    assert_eq!(&return_data[..4], &MAGIC, "profile record magic");
    let count = return_data[4] as usize;
    let mut marks: Vec<(u8, u64)> = Vec::with_capacity(count);
    for index in 0..count {
        let at = 5 + index * 9;
        let tag = return_data[at];
        let value = u64::from_le_bytes(return_data[at + 1..at + 9].try_into().unwrap());
        marks.push((tag, value));
    }
    let at = 5 + count * 9;
    let cpi_units = u64::from_le_bytes(return_data[at..at + 8].try_into().unwrap());
    let cpi_count = u32::from_le_bytes(return_data[at + 8..at + 12].try_into().unwrap());
    let setup_units = u64::from_le_bytes(return_data[at + 12..at + 20].try_into().unwrap());

    let calibration = marks[0].1 - marks[1].1;
    let value_of = |tag: u8| marks.iter().find(|(mark, _)| *mark == tag).map(|(_, v)| *v);
    let mut phases = Vec::new();
    let mut previous = marks[1].1;
    for (tag, name) in PHASES {
        let Some(value) = value_of(tag) else { continue };
        // Each interval ends with one counter read; the interpreter also carries the pair of
        // reads that bracket every invoke.
        let overhead = if tag == TAG_EXECUTED {
            calibration * (1 + 4 * cpi_count as u64)
        } else {
            calibration
        };
        phases.push((name.to_string(), (previous - value).saturating_sub(overhead)));
        previous = value;
    }
    // Each window closes with a counter read of its own.
    let cpi_units = cpi_units.saturating_sub(calibration * cpi_count as u64);
    let setup_units = setup_units.saturating_sub(calibration * cpi_count as u64);
    Profile {
        phases,
        cpi_units,
        cpi_count,
        setup_units,
        calibration,
        // The first counter read already charged itself, so the prologue excludes it.
        prologue: budget - marks[0].1 - calibration,
        clean_total,
        total: consumed,
        budget,
    }
}

struct Harness {
    /// The same accounts under both builds, so each case is measured twice.
    profiled: MolluskContext<HashMap<Pubkey, Account>>,
    clean: MolluskContext<HashMap<Pubkey, Account>>,
    signer: Pubkey,
    rows: Vec<Pubkey>,
    next_id: u16,
    budget: u64,
}

impl Harness {
    fn new() -> Self {
        let signer = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..32).map(|_| Pubkey::new_unique()).collect();
        let store = || {
            let mut store: HashMap<Pubkey, Account> = HashMap::new();
            store.insert(signer, Account::new(1_000_000_000_000, 0, &system_program::id()));
            for key in &rows {
                store.insert(*key, Account::new(1_000_000_000, 0, &system_program::id()));
            }
            store
        };
        let context = |elf: &[u8]| {
            let mut mollusk = Mollusk::default();
            mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
            mollusk.add_program_with_loader_and_elf(&ID, &LOADER_V3, elf);
            let budget = mollusk.compute_budget.compute_unit_limit;
            (mollusk.with_context(store()), budget)
        };
        let (profiled, budget) = context(PROFILE_ELF);
        let (clean, _) = context(CLEAN_ELF);
        Self {
            profiled,
            clean,
            signer,
            rows,
            next_id: 1,
            budget,
        }
    }

    fn profile(&mut self, payload: Vec<u8>, accounts: Vec<AccountMeta>, inputs: Vec<u8>) -> Profile {
        let id = self.next_id;
        self.next_id += 1;
        let (template, _) = Pubkey::find_program_address(
            &[TEMPLATE_SEED, self.signer.as_ref(), &id.to_le_bytes()],
            &ID,
        );
        let mut create = Vec::with_capacity(35 + payload.len());
        create.push(IX_CREATE_TEMPLATE);
        create.extend_from_slice(&id.to_le_bytes());
        create.extend_from_slice(&solana_sha256_hasher::hash(&payload).to_bytes());
        create.extend_from_slice(&payload);
        let create = Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(self.signer, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: create,
        };
        for context in [&self.profiled, &self.clean] {
            let created = context.process_instruction(&create);
            assert!(created.program_result.is_ok(), "create failed: {created:#?}");
        }

        let mut metas = vec![AccountMeta::new_readonly(template, false)];
        metas.extend(accounts);
        let mut data = vec![IX_RUN];
        data.extend_from_slice(&inputs);
        let run = Instruction {
            program_id: ID,
            accounts: metas,
            data,
        };
        let result = self.profiled.process_instruction(&run);
        assert!(result.program_result.is_ok(), "run failed: {result:#?}");
        let clean = self.clean.process_instruction(&run);
        assert!(clean.program_result.is_ok(), "clean run failed: {clean:#?}");
        decode(
            &result.return_data,
            result.compute_units_consumed,
            clean.compute_units_consumed,
            self.budget,
        )
    }
}

fn empty(harness: &mut Harness) -> Profile {
    let mut builder = ProgramBuilder::new();
    let flag = builder.const_bool(true);
    builder.require(flag);
    harness.profile(builder.build().expect("builds"), Vec::new(), Vec::new())
}

fn payroll(harness: &mut Harness, rows: u8) -> Profile {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let row = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(rows, 1);
    let input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(input);
    let selector = builder.blob(&2u32.to_le_bytes());
    let cpi = builder.cpi(
        system,
        &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (row, ACCOUNT_WRITABLE)],
        &[Segment::Literal(selector), Segment::Register(DATA_REG_U64, amount)],
    );
    builder.for_each(0, |body| body.invoke(cpi, None));
    let mut accounts = vec![
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new(harness.signer, true),
    ];
    accounts.extend(
        harness.rows[..rows as usize]
            .iter()
            .map(|key| AccountMeta::new(*key, false)),
    );
    harness.profile(
        builder.build().expect("builds"),
        accounts,
        1_000u64.to_le_bytes().to_vec(),
    )
}

fn sum_rows(harness: &mut Harness, rows: u8) -> Profile {
    let mut builder = ProgramBuilder::new();
    let row = builder.row_account(0, None, None, 0);
    builder.batch(rows, 1);
    let total = builder.const_u64(0);
    builder.for_each(1u64 << total, |body| {
        let lamports = body.account_lamports(row);
        let next = body.binary(OP_ADD, total, lamports);
        body.mov(total, next);
    });
    let limit = builder.const_u64(u64::MAX);
    let under = builder.binary(OP_LTE, total, limit);
    builder.require(under);
    let accounts = harness.rows[..rows as usize]
        .iter()
        .map(|key| AccountMeta::new_readonly(*key, false))
        .collect();
    harness.profile(builder.build().expect("builds"), accounts, Vec::new())
}

fn print(name: &str, profile: &Profile) {
    println!("\n### {name}");
    println!(
        "{} CU on the ordinary build; {} under instrumentation, which costs {} CU per counter read",
        profile.clean_total, profile.total, profile.calibration
    );
    let _ = profile.budget;
    println!("| Phase | CU | Share |");
    println!("| --- | ---: | ---: |");
    let share = |units: u64| format!("{:.1}%", units as f64 * 100.0 / profile.clean_total as f64);
    for (phase, units) in rows(profile) {
        println!("| {phase} | {units} | {} |", share(units));
    }
    println!(
        "\nInstrumentation overhead, excluded above: {} CU",
        profile.total - profile.clean_total
    );
}

/// The rows of one case, in the order they are printed and published.
fn rows(profile: &Profile) -> Vec<(String, u64)> {
    let mut rows = vec![(
        "Entrypoint and account deserialization".to_string(),
        profile.prologue,
    )];
    for (phase, units) in &profile.phases {
        if phase.starts_with("Interpreter") && profile.cpi_units > 0 {
            rows.push((
                "Interpreter, other instructions".to_string(),
                units
                    .saturating_sub(profile.cpi_units)
                    .saturating_sub(profile.setup_units),
            ));
            rows.push((
                "Invoke build: accounts and data".to_string(),
                profile.setup_units,
            ));
            rows.push((
                "Invoke itself: runtime and callee".to_string(),
                profile.cpi_units,
            ));
        } else {
            rows.push((phase.clone(), *units));
        }
    }
    let accounted: u64 = rows.iter().map(|(_, units)| units).sum();
    rows.push((
        "Return and unattributed remainder".to_string(),
        profile.clean_total.saturating_sub(accounted),
    ));
    rows
}

#[test]
fn phase_profile() {
    let mut harness = Harness::new();
    let cases: Vec<(String, Profile)> = vec![
        ("Empty run".to_string(), empty(&mut harness)),
        ("Payroll, 1 row".to_string(), payroll(&mut harness, 1)),
        ("Payroll, 30 rows".to_string(), payroll(&mut harness, 30)),
        ("Sum 30 rows, no invoke".to_string(), sum_rows(&mut harness, 30)),
    ];
    for (name, profile) in &cases {
        print(name, profile);
    }

    if std::env::var("UPDATE_BENCHMARKS").as_deref() == Ok("1") {
        let published: Vec<serde_json::Value> = cases
            .iter()
            .map(|(name, profile)| {
                serde_json::json!({
                    "case": name,
                    "total": profile.clean_total,
                    "invokes": profile.cpi_count,
                    "instrumentationOverhead": profile.total - profile.clean_total,
                    "counterRead": profile.calibration,
                    "rows": rows(profile)
                        .into_iter()
                        .map(|(phase, units)| serde_json::json!({ "phase": phase, "units": units }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/cu-phases.json");
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&published).unwrap()),
        )
        .expect("write cu phases");
    }

    // The phases must add up to the ordinary run, give or take the sampling noise.
    for (name, profile) in &cases {
        let accounted: u64 =
            profile.prologue + profile.phases.iter().map(|(_, units)| units).sum::<u64>();
        // Subtracting one counter read per interval, and four per invoke, is exact only to
        // within sampling noise, which grows with the number of samples a case takes.
        let tolerance = 250 + profile.clean_total / 100;
        assert!(
            accounted.abs_diff(profile.clean_total) < tolerance,
            "{name} attributes {accounted} units to phases but the ordinary run costs {}",
            profile.clean_total
        );
    }
}
