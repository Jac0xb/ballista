//! Marginal compute cost of each thing a template can do.
//!
//! Every figure is a difference between two templates that differ only in how many times they do
//! one thing, so the fixed cost of a run cancels and what remains is the price of that feature.
//! `fixtures/cu-profile.json` feeds the table in `docs/cu-profile.md`; the point of the table is to
//! show where compute actually goes before anyone tries to golf it.
#![cfg(test)]

use std::collections::HashMap;

use ballista_common::instruction::{IX_CREATE_TEMPLATE, IX_RUN};
use ballista_common::template::{
    ProgramBuilder, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
    NO_INDEX, OP_ACCOUNT_DATA_LEN,
    OP_ACCOUNT_IS_EMPTY, OP_ACCOUNT_KEY, OP_ACCOUNT_LAMPORTS, OP_ADD, OP_AND, OP_CAST_U128,
    OP_CLOCK_SLOT, OP_CLOCK_TIMESTAMP, OP_LT, OP_READ_U64, VALUE_BYTES, VALUE_PUBKEY, VALUE_U64,
};
use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk, MolluskContext};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::{pubkey, Pubkey};
use solana_sdk_ids::system_program;

const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
const TEMPLATE_SEED: &[u8] = b"template";

/// One template, its runtime accounts, and its run data.
struct Case {
    payload: Vec<u8>,
    accounts: Vec<AccountMeta>,
    inputs: Vec<u8>,
}

struct Harness {
    context: MolluskContext<HashMap<Pubkey, Account>>,
    signer: Pubkey,
    /// Plain funded accounts, for templates that just need runtime slots.
    plain: Vec<Pubkey>,
    /// Accounts holding 128 bytes of System-owned data, for reads and length constraints.
    data: Vec<Pubkey>,
    next_id: u16,
}

impl Harness {
    fn new() -> Self {
        let signer = Pubkey::new_unique();
        let plain: Vec<Pubkey> = (0..64).map(|_| Pubkey::new_unique()).collect();
        let data: Vec<Pubkey> = (0..32).map(|_| Pubkey::new_unique()).collect();
        let mut store: HashMap<Pubkey, Account> = HashMap::new();
        store.insert(signer, Account::new(1_000_000_000_000, 0, &system_program::id()));
        for key in &plain {
            store.insert(*key, Account::new(1_000_000_000, 0, &system_program::id()));
        }
        for key in &data {
            let mut account = Account::new(1_000_000_000, 128, &system_program::id());
            account.data[64..72].copy_from_slice(&7u64.to_le_bytes());
            store.insert(*key, account);
        }
        let mut mollusk = Mollusk::default();
        mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
        mollusk.add_program_with_loader_and_elf(&ID, &LOADER_V3, BALLISTA_ELF);
        Self {
            context: mollusk.with_context(store),
            signer,
            plain,
            data,
            next_id: 1,
        }
    }

    /// Uploads and runs one case, returning the compute units the run consumed.
    fn measure(&mut self, case: Case) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let (template, _) = Pubkey::find_program_address(
            &[TEMPLATE_SEED, self.signer.as_ref(), &id.to_le_bytes()],
            &ID,
        );
        let mut create = Vec::with_capacity(35 + case.payload.len());
        create.push(IX_CREATE_TEMPLATE);
        create.extend_from_slice(&id.to_le_bytes());
        create.extend_from_slice(&solana_sha256_hasher::hash(&case.payload).to_bytes());
        create.extend_from_slice(&case.payload);
        let created = self.context.process_instruction(&Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(self.signer, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: create,
        });
        assert!(created.program_result.is_ok(), "create failed: {created:#?}");

        let mut metas = vec![AccountMeta::new_readonly(template, false)];
        metas.extend(case.accounts);
        let mut run = vec![IX_RUN];
        run.extend_from_slice(&case.inputs);
        let result = self.context.process_instruction(&Instruction {
            program_id: ID,
            accounts: metas,
            data: run,
        });
        assert!(result.program_result.is_ok(), "run failed: {result:#?}");
        result.compute_units_consumed
    }

    /// The per-unit cost of whatever `build` repeats, with the fixed cost of a run cancelled out.
    fn marginal(&mut self, low: usize, high: usize, mut build: impl FnMut(&Self, usize) -> Case) -> f64 {
        let small = build(self, low);
        let small_units = self.measure(small);
        let large = build(self, high);
        let large_units = self.measure(large);
        (large_units as f64 - small_units as f64) / (high - low) as f64
    }
}

/// A template that does nothing but assert a constant, which is the floor for any run.
fn empty_template() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let flag = builder.const_bool(true);
    builder.require(flag);
    builder.build().expect("builds")
}

#[test]
fn profile_compute_units() {
    let mut harness = Harness::new();
    let mut rows: Vec<(String, String, f64, String)> = Vec::new();
    let mut record = |group: &str, name: &str, value: f64, note: &str| {
        rows.push((group.to_string(), name.to_string(), value, note.to_string()));
    };

    // The floor: parse the stored template, validate zero accounts, allocate, run one assertion.
    let floor = harness.measure(Case {
        payload: empty_template(),
        accounts: Vec::new(),
        inputs: Vec::new(),
    });
    record("Run", "Fixed cost of any run", floor as f64, "parse, validate, allocate, one assertion");

    // ---- runtime accounts -------------------------------------------------
    let per_account = harness.marginal(1, 21, |harness, n| {
        let mut builder = ProgramBuilder::new();
        for _ in 0..n {
            builder.account(0, None, None, 0);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: harness.plain[..n]
                .iter()
                .map(|key| AccountMeta::new_readonly(*key, false))
                .collect(),
            inputs: Vec::new(),
        }
    });
    record("Accounts", "Unconstrained account", per_account, "declared and supplied, nothing checked");

    let per_pinned_address = harness.marginal(1, 21, |harness, n| {
        let mut builder = ProgramBuilder::new();
        for index in 0..n {
            builder.account(0, Some(harness.plain[index].to_bytes()), None, 0);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: harness.plain[..n]
                .iter()
                .map(|key| AccountMeta::new_readonly(*key, false))
                .collect(),
            inputs: Vec::new(),
        }
    });
    record("Accounts", "Account pinned to an address", per_pinned_address, "32-byte comparison");

    let per_pinned_owner = harness.marginal(1, 21, |harness, n| {
        let mut builder = ProgramBuilder::new();
        for _ in 0..n {
            builder.account(0, None, Some(system_program::id().to_bytes()), 0);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: harness.plain[..n]
                .iter()
                .map(|key| AccountMeta::new_readonly(*key, false))
                .collect(),
            inputs: Vec::new(),
        }
    });
    record("Accounts", "Account pinned to an owner", per_pinned_owner, "32-byte comparison");

    let per_min_len = harness.marginal(1, 21, |harness, n| {
        let mut builder = ProgramBuilder::new();
        for _ in 0..n {
            builder.account(0, None, None, 64);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: (0..n)
                .map(|index| AccountMeta::new_readonly(harness.data[index % harness.data.len()], false))
                .collect(),
            inputs: Vec::new(),
        }
    });
    record("Accounts", "Account with a minimum length", per_min_len, "length comparison");

    // ---- inputs -----------------------------------------------------------
    for (label, value_type, max_len, width) in [
        ("u64 input", VALUE_U64, 0u16, 8usize),
        ("pubkey input", VALUE_PUBKEY, 0, 32),
        ("32-byte bytes input", VALUE_BYTES, 32, 34),
    ] {
        let per_input = harness.marginal(1, 11, |_, n| {
            let mut builder = ProgramBuilder::new();
            for _ in 0..n {
                builder.input(value_type, max_len);
            }
            let flag = builder.const_bool(true);
            builder.require(flag);
            let mut inputs = Vec::new();
            for _ in 0..n {
                if value_type == VALUE_BYTES {
                    inputs.extend_from_slice(&32u16.to_le_bytes());
                    inputs.extend_from_slice(&[9u8; 32]);
                } else {
                    inputs.extend_from_slice(&vec![0u8; width]);
                }
            }
            Case {
                payload: builder.build().expect("builds"),
                accounts: Vec::new(),
                inputs,
            }
        });
        record("Inputs", label, per_input, "decoded before execution");
    }

    // ---- instructions -----------------------------------------------------
    // Each probe repeats one opcode over registers that already hold values.
    macro_rules! per_instruction {
        ($group:literal, $label:literal, $note:literal, $emit:expr) => {{
            let value = harness.marginal(2, 22, |harness, n| {
                let mut builder = ProgramBuilder::new();
                let account = builder.account(0, None, None, 64);
                let seed = builder.const_u64(3);
                let other = builder.const_u64(5);
                #[allow(clippy::redundant_closure_call)]
                for _ in 0..n {
                    ($emit)(&mut builder, seed, other, account);
                }
                let flag = builder.const_bool(true);
                builder.require(flag);
                Case {
                    payload: builder.build().expect("builds"),
                    accounts: vec![AccountMeta::new_readonly(harness.data[0], false)],
                    inputs: Vec::new(),
                }
            });
            record($group, $label, value, $note);
        }};
    }

    per_instruction!("Instructions", "Constant", "const u64", |b: &mut ProgramBuilder, _s, _o, _a| {
        b.const_u64(9);
    });
    per_instruction!("Instructions", "Move", "copy a register", |b: &mut ProgramBuilder, s, o, _a| {
        b.mov(o, s);
    });
    per_instruction!("Instructions", "Checked add", "u64 + u64", |b: &mut ProgramBuilder, s, o, _a| {
        b.binary(OP_ADD, s, o);
    });
    per_instruction!("Instructions", "Comparison", "u64 < u64", |b: &mut ProgramBuilder, s, o, _a| {
        b.binary(OP_LT, s, o);
    });
    per_instruction!("Instructions", "Cast", "u64 to u128", |b: &mut ProgramBuilder, s, _o, _a| {
        b.cast(OP_CAST_U128, s);
    });
    per_instruction!("Instructions", "Account key read", "32 bytes into a register", |b: &mut ProgramBuilder, _s, _o, a| {
        b.op(OP_ACCOUNT_KEY, a, NO_INDEX, NO_INDEX, 0);
    });
    per_instruction!("Instructions", "Account lamports read", "u64 header field", |b: &mut ProgramBuilder, _s, _o, a| {
        b.op(OP_ACCOUNT_LAMPORTS, a, NO_INDEX, NO_INDEX, 0);
    });
    per_instruction!("Instructions", "Account length read", "u64 header field", |b: &mut ProgramBuilder, _s, _o, a| {
        b.op(OP_ACCOUNT_DATA_LEN, a, NO_INDEX, NO_INDEX, 0);
    });
    per_instruction!("Instructions", "Account emptiness read", "bool header field", |b: &mut ProgramBuilder, _s, _o, a| {
        b.op(OP_ACCOUNT_IS_EMPTY, a, NO_INDEX, NO_INDEX, 0);
    });
    per_instruction!("Instructions", "Account data read", "u64 at a fixed offset", |b: &mut ProgramBuilder, _s, _o, a| {
        b.op(OP_READ_U64, a, NO_INDEX, NO_INDEX, 0);
    });
    per_instruction!("Instructions", "Clock slot", "sysvar read", |b: &mut ProgramBuilder, _s, _o, _a| {
        b.op(OP_CLOCK_SLOT, NO_INDEX, NO_INDEX, NO_INDEX, 0);
    });
    per_instruction!("Instructions", "Clock timestamp", "sysvar read", |b: &mut ProgramBuilder, _s, _o, _a| {
        b.op(OP_CLOCK_TIMESTAMP, NO_INDEX, NO_INDEX, NO_INDEX, 0);
    });

    // A boolean AND needs two booleans, so it gets its own probe.
    let per_and = harness.marginal(2, 22, |_, n| {
        let mut builder = ProgramBuilder::new();
        let left = builder.const_bool(true);
        let right = builder.const_bool(true);
        for _ in 0..n {
            builder.binary(OP_AND, left, right);
        }
        builder.require(left);
        Case {
            payload: builder.build().expect("builds"),
            accounts: Vec::new(),
            inputs: Vec::new(),
        }
    });
    record("Instructions", "Boolean and", per_and, "bool && bool");

    // A dynamic-offset read takes its offset from a register.
    let per_dynamic_read = harness.marginal(2, 22, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(64);
        for _ in 0..n {
            builder.read_dynamic(OP_READ_U64, account, offset);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: vec![AccountMeta::new_readonly(harness.data[0], false)],
            inputs: Vec::new(),
        }
    });
    record("Instructions", "Account data read, dynamic offset", per_dynamic_read, "offset from a register");

    // PDA derivation runs the address search, which dominates any template that uses it.
    let per_pda = harness.marginal(1, 6, |_harness, n| {
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let seed = builder.blob(b"seed");
        for _ in 0..n {
            builder.derive_pda(program, &[Segment::Literal(seed)]);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: vec![AccountMeta::new_readonly(system_program::id(), false)],
            inputs: Vec::new(),
        }
    });
    record("Instructions", "PDA derivation", per_pda, "one literal seed, bump search");

    // With the bump supplied, the same derivation runs once instead of searching down from 255.
    let per_create_pda = harness.marginal(1, 6, |_harness, n| {
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let seed = builder.blob(b"seed");
        let canonical = Pubkey::find_program_address(&[b"seed"], &system_program::id()).1;
        let bump = builder.const_u64(canonical as u64);
        for _ in 0..n {
            builder.create_pda(program, bump, &[Segment::Literal(seed)]);
        }
        let flag = builder.const_bool(true);
        builder.require(flag);
        Case {
            payload: builder.build().expect("builds"),
            accounts: vec![AccountMeta::new_readonly(system_program::id(), false)],
            inputs: Vec::new(),
        }
    });
    record(
        "Instructions",
        "PDA derivation, supplied bump",
        per_create_pda,
        "one literal seed, no search",
    );

    // An assertion is its own dispatch, on top of the comparison that feeds it.
    let per_require = harness.marginal(1, 21, |_harness, n| {
        let mut builder = ProgramBuilder::new();
        let flag = builder.const_bool(true);
        for _ in 0..n {
            builder.require(flag);
        }
        Case {
            payload: builder.build().expect("builds"),
            accounts: Vec::new(),
            inputs: Vec::new(),
        }
    });
    record("Instructions", "Assertion", per_require, "reads a bool register and continues");

    // ---- cross-program invocations ----------------------------------------
    let per_cpi = harness.marginal(1, 11, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let amount = builder.const_u64(1);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        for _ in 0..n {
            let cpi = builder.cpi(
                system,
                &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (to, ACCOUNT_WRITABLE)],
                &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
            );
            builder.invoke(cpi, None);
        }
        Case {
            payload: builder.build().expect("builds"),
            accounts: vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(harness.signer, true),
                AccountMeta::new(harness.plain[0], false),
            ],
            inputs: Vec::new(),
        }
    });
    record("Invocations", "Cross-program invocation", per_cpi, "System transfer, 2 accounts, 12 bytes");

    let per_cpi_account = harness.marginal(2, 22, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let mut declared = vec![(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)];
        for _ in 0..n {
            let extra = builder.account(ACCOUNT_WRITABLE, None, None, 0);
            declared.push((extra, ACCOUNT_WRITABLE));
        }
        let amount = builder.const_u64(1);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &declared,
            &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
        );
        builder.invoke(cpi, None);
        let mut accounts = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(harness.signer, true),
        ];
        accounts.extend(harness.plain[..n].iter().map(|key| AccountMeta::new(*key, false)));
        Case {
            payload: builder.build().expect("builds"),
            accounts,
            inputs: Vec::new(),
        }
    });
    record("Invocations", "Account passed to an invocation", per_cpi_account, "resolved and given a meta");

    let per_cpi_byte = harness.marginal(12, 1_012, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        // A padded transfer: bincode ignores the trailing bytes, so only the size varies.
        let mut payload = vec![2u8, 0, 0, 0];
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.resize(n, 0);
        let blob = builder.blob(&payload);
        let cpi = builder.cpi(
            system,
            &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (to, ACCOUNT_WRITABLE)],
            &[Segment::Literal(blob)],
        );
        builder.invoke(cpi, None);
        Case {
            payload: builder.build().expect("builds"),
            accounts: vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(harness.signer, true),
                AccountMeta::new(harness.plain[0], false),
            ],
            inputs: Vec::new(),
        }
    });
    record("Invocations", "Byte of invocation data", per_cpi_byte, "measured across 1,000 extra bytes");

    // ---- batch ------------------------------------------------------------
    let per_row_empty = harness.marginal(1, 21, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(30, 0);
        builder.for_each(0, |body| {
            let key = body.op(OP_ACCOUNT_KEY, row, NO_INDEX, NO_INDEX, 0);
            let same = body.binary(ballista_common::template::OP_EQ, key, key);
            body.require(same);
        });
        Case {
            payload: builder.build().expect("builds"),
            accounts: harness.plain[..n]
                .iter()
                .map(|key| AccountMeta::new_readonly(*key, false))
                .collect(),
            inputs: Vec::new(),
        }
    });
    record("Batches", "Iteration, three instructions", per_row_empty, "one account read, compare, assert");

    let per_row_cpi = harness.marginal(1, 11, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let row = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(30, 0);
        let amount = builder.const_u64(1);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (row, ACCOUNT_WRITABLE)],
            &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
        );
        builder.for_each(0, |body| body.invoke(cpi, None));
        let mut accounts = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(harness.signer, true),
        ];
        accounts.extend(harness.plain[..n].iter().map(|key| AccountMeta::new(*key, false)));
        Case {
            payload: builder.build().expect("builds"),
            accounts,
            inputs: Vec::new(),
        }
    });
    record("Batches", "Iteration with one invocation", per_row_cpi, "the shape a payroll run repeats");

    // A fixed account in a batched invocation's list is resolved again on every iteration, even
    // though nothing about it changes. This is what hoisting it out of the loop would recover.
    const HOIST_ROWS: usize = 20;
    let per_fixed_account_per_row = harness.marginal(2, 8, |harness, n| {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let row = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        // Extra fixed accounts, read-only, so the invoked transfer still behaves the same.
        let spare: Vec<u8> = (0..n - 2)
            .map(|_| builder.account(0, None, None, 0))
            .collect();
        builder.batch(HOIST_ROWS as u8, 0);
        let amount = builder.const_u64(1);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let mut cpi_accounts = vec![
            (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (row, ACCOUNT_WRITABLE),
        ];
        cpi_accounts.extend(spare.iter().map(|account| (*account, 0)));
        let cpi = builder.cpi(
            system,
            &cpi_accounts,
            &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
        );
        builder.for_each(0, |body| body.invoke(cpi, None));
        let mut accounts = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(harness.signer, true),
        ];
        accounts.extend(
            harness.plain[32..32 + n - 2]
                .iter()
                .map(|key| AccountMeta::new_readonly(*key, false)),
        );
        accounts.extend(
            harness.plain[..HOIST_ROWS]
                .iter()
                .map(|key| AccountMeta::new(*key, false)),
        );
        Case {
            payload: builder.build().expect("builds"),
            accounts,
            inputs: Vec::new(),
        }
    });
    record(
        "Batches",
        "Fixed invocation account, per iteration",
        per_fixed_account_per_row / HOIST_ROWS as f64,
        "resolved again every row",
    );

    // Every iteration restores the register file from a snapshot taken before the loop, so a
    // template pays for registers it declared even when the loop body never touches them. Two
    // per-row costs, measured at different register counts, isolate that copy.
    let per_row_at = |harness: &mut Harness, registers: usize| {
        harness.marginal(2, 20, |harness, rows| {
            let mut builder = ProgramBuilder::new();
            let row = builder.row_account(0, None, None, 0);
            for value in 0..registers {
                builder.const_u64(value as u64);
            }
            builder.batch(rows as u8, 0);
            builder.for_each(0, |body| {
                let key = body.op(OP_ACCOUNT_KEY, row, NO_INDEX, NO_INDEX, 0);
                let same = body.binary(ballista_common::template::OP_EQ, key, key);
                body.require(same);
            });
            Case {
                payload: builder.build().expect("builds"),
                accounts: harness.plain[..rows]
                    .iter()
                    .map(|key| AccountMeta::new_readonly(*key, false))
                    .collect(),
                inputs: Vec::new(),
            }
        })
    };
    let few = per_row_at(&mut harness, 4);
    let many = per_row_at(&mut harness, 40);
    record(
        "Batches",
        "Declared register, per iteration",
        (many - few) / 36.0,
        "restored from the pre-loop snapshot whether or not the body writes it",
    );

    let mut out = serde_json::Map::new();
    for (group, name, value, note) in &rows {
        let mut entry = serde_json::Map::new();
        entry.insert("group".into(), group.clone().into());
        let rounded = if *value < 10.0 {
            serde_json::Value::from((value * 100.0).round() / 100.0)
        } else {
            serde_json::Value::from(value.round() as i64)
        };
        entry.insert("computeUnits".into(), rounded);
        entry.insert("note".into(), note.clone().into());
        out.insert(name.clone(), entry.into());
        eprintln!("{group:12} {name:38} {:>8.2} CU  ({note})", value);
    }
    if std::env::var("UPDATE_BENCHMARKS").as_deref() == Ok("1") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/cu-profile.json");
        std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&out).unwrap()))
            .expect("write cu profile");
    }
}
