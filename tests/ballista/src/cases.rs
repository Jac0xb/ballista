//! The compute-unit cases, shared by the bencher and the ceiling test.
//!
//! Each case is one instruction against a fixed account set, built from the same `ProgramBuilder`
//! the SDK compiles to. `benches/compute_units.rs` benches them; `ceilings.rs` asserts none of
//! them costs more than the committed ceiling.
use ballista_common::instruction::{IX_CREATE_TEMPLATE, IX_RUN};
use ballista_common::template::{
    ProgramBuilder, Segment, TemplateAccountHeader, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER,
    ACCOUNT_WRITABLE, DATA_REG_U64, OP_ADD, OP_AND, OP_EQ, OP_GTE, OP_LTE, OP_READ_U64, VALUE_U64,
};
use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::{pubkey, Pubkey};
use solana_sdk_ids::system_program;

pub const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
pub const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
const TEMPLATE_SEED: &[u8] = b"template";

/// A Mollusk instance with the program and a late clock, matching the other harnesses.
pub fn mollusk() -> Mollusk {
    let mut mollusk = Mollusk::default();
    mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
    mollusk.add_program_with_loader_and_elf(&ID, &LOADER_V3, BALLISTA_ELF);
    mollusk
}

/// Distinct addresses drawn from a fixed sequence. `Pubkey::new_unique` counts globally and is
/// shared between parallel tests, which would make a template's PDA bump — and so its compute
/// cost — depend on what else ran first. These cases are compared against a recorded ceiling, so
/// they have to be reproducible.
fn key(index: u16) -> Pubkey {
    let mut bytes = [7u8; 32];
    bytes[..2].copy_from_slice(&index.to_le_bytes());
    Pubkey::new_from_array(bytes)
}

/// Every case, in a stable order.
pub fn cases() -> Vec<(&'static str, Case)> {
    let creator = key(0);
    vec![
        ("run, empty template", empty_run(creator)),
        ("run, one system transfer", one_transfer(creator, 2)),
        ("run, payroll 8 rows", payroll(creator, 3, 8)),
        ("run, payroll 30 rows", payroll(creator, 4, 30)),
        ("run, sum 30 rows, no cpi", sum_rows(creator, 5, 30)),
        ("run, oracle band, no cpi", oracle_band(creator, 6)),
        ("run, pda derivation, bump search", pda_case(creator, 7, false)),
        ("run, pda derivation, bump supplied", pda_case(creator, 8, true)),
        ("create template, payroll 30 rows", upload(9)),
    ]
}

/// One benchmark case: the instruction to run and the accounts it runs against.
pub struct Case {
    pub instruction: Instruction,
    pub accounts: Vec<(Pubkey, Account)>,
}

/// Builds the finalized template account directly, so each case measures the run alone.
fn finalized_template(creator: &Pubkey, template_id: u16, payload: &[u8]) -> (Pubkey, Account) {
    let (address, bump) = Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    );
    let hash = solana_sha256_hasher::hash(payload).to_bytes();
    let mut header =
        TemplateAccountHeader::new_uploading(creator.to_bytes(), template_id, bump, payload.len(), hash)
            .expect("header");
    header.set_written_len(payload.len()).expect("written len");
    header.finalize().expect("finalize");
    let mut data = header.as_bytes().to_vec();
    data.extend_from_slice(payload);
    let mut account = Account::new(10_000_000, data.len(), &ID);
    account.data = data;
    (address, account)
}

fn run_case(
    creator: Pubkey,
    template_id: u16,
    payload: Vec<u8>,
    runtime: Vec<(AccountMeta, Account)>,
    inputs: Vec<u8>,
) -> Case {
    let (template, template_account) = finalized_template(&creator, template_id, &payload);
    let mut metas = vec![AccountMeta::new_readonly(template, false)];
    let mut accounts = vec![(template, template_account)];
    for (meta, account) in runtime {
        accounts.push((meta.pubkey, account));
        metas.push(meta);
    }
    let mut data = vec![IX_RUN];
    data.extend_from_slice(&inputs);
    Case {
        instruction: Instruction {
            program_id: ID,
            accounts: metas,
            data,
        },
        accounts,
    }
}

fn system_program_account() -> Account {
    mollusk_svm::program::keyed_account_for_system_program().1
}

fn funded(index: u16) -> (Pubkey, Account) {
    (
        key(index),
        Account::new(1_000_000_000, 0, &system_program::id()),
    )
}

fn with_data(index: u16) -> (Pubkey, Account) {
    let mut account = Account::new(1_000_000_000, 128, &system_program::id());
    account.data[64..72].copy_from_slice(&7u64.to_le_bytes());
    (key(index), account)
}

/// A template that asserts a constant: the floor any run pays before doing work.
fn empty_run(creator: Pubkey) -> Case {
    let mut builder = ProgramBuilder::new();
    let flag = builder.const_bool(true);
    builder.require(flag);
    run_case(creator, 1, builder.build().expect("builds"), Vec::new(), Vec::new())
}

/// One System transfer of an input amount: the smallest useful template.
fn one_transfer(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let to = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let selector = builder.blob(&2u32.to_le_bytes());
    let cpi = builder.cpi(
        system,
        &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (to, ACCOUNT_WRITABLE)],
        &[Segment::Literal(selector), Segment::Register(DATA_REG_U64, amount)],
    );
    builder.invoke(cpi, None);
    let (signer, signer_account) = funded(template_id * 100 + 1);
    let (recipient, recipient_account) = funded(template_id * 100 + 2);
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![
            (AccountMeta::new_readonly(system_program::id(), false), system_program_account()),
            (AccountMeta::new(signer, true), signer_account),
            (AccountMeta::new(recipient, false), recipient_account),
        ],
        1_000u64.to_le_bytes().to_vec(),
    )
}

/// A batch of `rows` System transfers, the payroll shape from the cookbook.
fn payroll(creator: Pubkey, template_id: u16, rows: u8) -> Case {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let to = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(rows, 1);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let selector = builder.blob(&2u32.to_le_bytes());
    let cpi = builder.cpi(
        system,
        &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (to, ACCOUNT_WRITABLE)],
        &[Segment::Literal(selector), Segment::Register(DATA_REG_U64, amount)],
    );
    builder.for_each(0, |body| body.invoke(cpi, None));
    let (signer, signer_account) = funded(template_id * 100 + 1);
    let mut runtime = vec![
        (AccountMeta::new_readonly(system_program::id(), false), system_program_account()),
        (AccountMeta::new(signer, true), signer_account),
    ];
    for row in 0..rows {
        let (recipient, account) = funded(template_id * 100 + 2 + row as u16);
        runtime.push((AccountMeta::new(recipient, false), account));
    }
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        runtime,
        1_000u64.to_le_bytes().to_vec(),
    )
}

/// Two account-data reads and a band check: guardrails with no CPI at all.
fn oracle_band(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let oracle = builder.account(0, None, None, 128);
    let price = builder.read(OP_READ_U64, oracle, 64);
    let low_input = builder.input(VALUE_U64, 0);
    let high_input = builder.input(VALUE_U64, 0);
    let low = builder.load_input(low_input);
    let high = builder.load_input(high_input);
    let above = builder.binary(OP_GTE, price, low);
    let below = builder.binary(OP_LTE, price, high);
    let within = builder.binary(OP_AND, above, below);
    builder.require(within);
    let (account_key, account) = with_data(template_id * 100 + 1);
    let mut inputs = 1u64.to_le_bytes().to_vec();
    inputs.extend_from_slice(&1_000u64.to_le_bytes());
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(AccountMeta::new_readonly(account_key, false), account)],
        inputs,
    )
}

/// The canonical-bump search against a single derivation with the bump supplied.
fn pda_case(creator: Pubkey, template_id: u16, supply_bump: bool) -> Case {
    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let seed = builder.blob(b"seed");
    let derived = if supply_bump {
        let canonical = Pubkey::find_program_address(&[b"seed"], &system_program::id()).1;
        let bump = builder.const_u64(canonical as u64);
        builder.create_pda(program, bump, &[Segment::Literal(seed)])
    } else {
        builder.derive_pda(program, &[Segment::Literal(seed)])
    };
    let same = builder.binary(OP_EQ, derived, derived);
    builder.require(same);
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(
            AccountMeta::new_readonly(system_program::id(), false),
            system_program_account(),
        )],
        Vec::new(),
    )
}

/// Reading and comparing a field on every row, with no CPI: the cost of iteration by itself.
fn sum_rows(creator: Pubkey, template_id: u16, rows: u8) -> Case {
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
    let mut runtime = Vec::new();
    for row in 0..rows {
        let (address, account) = funded(template_id * 100 + 1 + row as u16);
        runtime.push((AccountMeta::new_readonly(address, false), account));
    }
    run_case(creator, template_id, builder.build().expect("builds"), runtime, Vec::new())
}

/// Uploading a template in one instruction: the one-time cost a template pays before any run.
fn upload(template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let to = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(30, 1);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let selector = builder.blob(&2u32.to_le_bytes());
    let cpi = builder.cpi(
        system,
        &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (to, ACCOUNT_WRITABLE)],
        &[Segment::Literal(selector), Segment::Register(DATA_REG_U64, amount)],
    );
    builder.for_each(0, |body| body.invoke(cpi, None));
    let bytes = builder.build().expect("builds");
    let (signer, signer_account) = funded(template_id * 100 + 1);
    let (template, _) = Pubkey::find_program_address(
        &[TEMPLATE_SEED, signer.as_ref(), &template_id.to_le_bytes()],
        &ID,
    );
    let mut data = Vec::with_capacity(35 + bytes.len());
    data.push(IX_CREATE_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&solana_sha256_hasher::hash(&bytes).to_bytes());
    data.extend_from_slice(&bytes);
    Case {
        instruction: Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(signer, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        },
        accounts: vec![
            (signer, signer_account),
            (template, Account::new(0, 0, &system_program::id())),
            mollusk_svm::program::keyed_account_for_system_program(),
        ],
    }
}

