//! The compute-unit cases, shared by the bencher and the ceiling test.
//!
//! Each case is one instruction against a fixed account set, built from the same `ProgramBuilder`
//! the SDK compiles to. `benches/compute_units.rs` benches them; `ceilings.rs` asserts none of
//! them costs more than the committed ceiling.
use ballista_common::instruction::{IX_CREATE_TEMPLATE, IX_RUN};
use ballista_common::template::{
    ProgramBuilder, Segment, TemplateAccountHeader, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER,
    ACCOUNT_WRITABLE, DATA_REG_PUBKEY, DATA_REG_U64, INSTRUCTIONS_SYSVAR_ID, NO_INDEX, OP_ADD, OP_AND, OP_BIT_AND,
    OP_BIT_OR, OP_BIT_XOR, OP_CAST_U64, OP_EQ, OP_GTE, OP_INSTRUCTION_ACCOUNT,
    OP_INSTRUCTION_ACCOUNT_COUNT, OP_INSTRUCTION_ACCOUNT_FLAGS, OP_INSTRUCTION_COUNT,
    OP_INSTRUCTION_DATA_LEN, OP_INSTRUCTION_INDEX, OP_INSTRUCTION_PROGRAM, OP_LT, OP_LTE,
    OP_READ_I32, OP_READ_I64, OP_READ_U64, OP_READ_U8, OP_REM, OP_SHL, OP_SHR, VALUE_U64,
};
use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::{pubkey, Pubkey};
use solana_sdk_ids::{system_program, sysvar};

pub const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
pub const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
const TEMPLATE_SEED: &[u8] = b"template";

/// Critic tooling. `Mollusk::default()` turns on every SVM feature, 15 of which mainnet does not
/// have (LiteSVM's `MAINNET_ACTIVE_FEATURES`, sourced 2026-09-15), and allows SIMD-0268's CPI depth
/// of 9 where mainnet allows 5. With `BALLISTA_MAINNET_FEATURES=1` this rebuilds the feature set,
/// the compute budget and the program cache to match mainnet; otherwise it is `Mollusk::default()`.
pub fn base_mollusk() -> Mollusk {
    let mut mollusk = Mollusk::default();
    if std::env::var("BALLISTA_MAINNET_FEATURES").as_deref() == Ok("1") {
        let mut features = solana_svm_feature_set::SVMFeatureSet::all_enabled();
        features.account_data_direct_mapping = false;
        features.blake3_syscall_enabled = false;
        features.block_revenue_sharing = false;
        features.deprecate_legacy_vote_ixs = false;
        features.direct_account_pointers_in_program_input = false;
        features.disable_sbpf_v0_execution = false;
        features.disable_sbpf_v0_v1_v2_deployment = false;
        features.enable_big_mod_exp_syscall = false;
        features.enable_sha512_syscall = false;
        features.increase_tx_account_lock_limit = false;
        features.raise_cpi_nesting_limit_to_8 = false;
        features.reenable_sbpf_v0_execution = false;
        features.remaining_compute_units_syscall_enabled = false;
        features.virtual_address_space_adjustments = false;
        features.vote_account_initialize_v2 = false;
        let budget = solana_compute_budget::compute_budget::ComputeBudget::new_with_defaults(false);
        mollusk.program_cache = mollusk_svm::program::ProgramCache::new(&features, &budget, false);
        mollusk.feature_set = features;
        mollusk.compute_budget = budget;
    }
    mollusk
}

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
        ("run, log and return 16 bytes", output_case(creator, 12)),
        ("create template, payroll 30 rows", upload(9)),
        ("run, math opcodes, no cpi", math_ops(creator, 10)),
        ("run, count loop 30 passes, no cpi", count_loop(creator, 11, 30)),
        ("run, introspection, no cpi", introspection(creator, 13)),
        ("run, registry open and update", registry_update(creator, 14)),
        ("run, group filter over 8 token accounts, no cpi", group_filter(creator, 15)),
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

/// Every opcode from `OP_MUL_DIV` up, as a price check scaled by an oracle's exponent would use
/// them. They all run in `extended_instruction`, behind the dispatch loop's fallback arm, and no
/// other case reaches it.
fn math_ops(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let oracle = builder.account(0, None, None, 128);
    // `with_data` stores 7 at offset 64: read as the oracle's i32 exponent, a scale of 10^7.
    let exponent = builder.read(OP_READ_I32, oracle, 64);
    let exponent = builder.cast(OP_CAST_U64, exponent);
    let scale = builder.pow10(exponent);
    // 3.0 units at that scale, priced at 12,345 each: 37,035.
    let amount = builder.const_u128(3 * 10u128.pow(7));
    let price = builder.const_u128(12_345);
    let value = builder.mul_div(amount, price, scale);
    // A 30 basis-point fee on 1,000,003, rounded down and up: 3,000 and 3,001.
    let gross = builder.const_u64(1_000_003);
    let bps = builder.const_u64(30);
    let basis = builder.const_u64(10_000);
    let fee_floor = builder.mul_div(gross, bps, basis);
    let fee_ceiling = builder.mul_div_ceil(gross, bps, basis);
    // What is left over past whole lots of 1,000: 3.
    let lot = builder.const_u64(1_000);
    let odd_lot = builder.binary(OP_REM, gross, lot);
    // A flag byte taken out of a word and put back: 0xab00 twice, which cancel.
    let flags = builder.const_u64(0xabcd);
    let mask = builder.const_u64(0xff00);
    let eight = builder.const_u64(8);
    let high = builder.binary(OP_BIT_AND, flags, mask);
    let byte = builder.binary(OP_SHR, high, eight);
    let back = builder.binary(OP_SHL, byte, eight);
    let cancelled = builder.binary(OP_BIT_XOR, back, high);
    let merged = builder.binary(OP_BIT_OR, cancelled, odd_lot);
    let expected_value = builder.const_u128(37_035);
    let value_right = builder.binary(OP_EQ, value, expected_value);
    let rounded_up = builder.binary(OP_LT, fee_floor, fee_ceiling);
    let three = builder.const_u64(3);
    let bits_right = builder.binary(OP_EQ, merged, three);
    let both = builder.binary(OP_AND, value_right, rounded_up);
    let all = builder.binary(OP_AND, both, bits_right);
    builder.require(all);
    let (account_key, account) = with_data(template_id * 100 + 1);
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(AccountMeta::new_readonly(account_key, false), account)],
        Vec::new(),
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

/// A read and an input, logged with a tag and then returned: the two output opcodes and their
/// syscalls, `sol_log_data` and `sol_set_return_data`.
fn output_case(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let oracle = builder.account(0, None, None, 128);
    let price = builder.read(OP_READ_U64, oracle, 64);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let tag = builder.blob(b"BOUT");
    builder.emit_data(&[
        Segment::Literal(tag),
        Segment::Register(DATA_REG_U64, price),
        Segment::Register(DATA_REG_U64, amount),
    ]);
    builder.set_return_data(&[
        Segment::Register(DATA_REG_U64, price),
        Segment::Register(DATA_REG_U64, amount),
    ]);
    let (account_key, account) = with_data(template_id * 100 + 1);
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(AccountMeta::new_readonly(account_key, false), account)],
        1_000u64.to_le_bytes().to_vec(),
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

/// Adding up the pass index over thirty passes of a count loop: what a pass costs without rows.
fn count_loop(creator: Pubkey, template_id: u16, passes: u8) -> Case {
    let mut builder = ProgramBuilder::new();
    let count_input = builder.input(VALUE_U64, 0);
    let total = builder.const_u64(0);
    let count = builder.load_input(count_input);
    builder.repeat(count, passes, 1u64 << total, |body| {
        let index = body.loop_index();
        let next = body.binary(OP_ADD, total, index);
        body.mov(total, next);
    });
    let limit = builder.const_u64(u64::MAX);
    let under = builder.binary(OP_LTE, total, limit);
    builder.require(under);
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        Vec::new(),
        (passes as u64).to_le_bytes().to_vec(),
    )
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

/// Every introspection and byte opcode once, against the run's own instruction. Alone in its
/// transaction it is instruction 0, the template is its first account, and its data is the
/// `IX_RUN` discriminator and then the one `u64` input, which the template compares with the same
/// eight bytes read from an account.
fn introspection(creator: Pubkey, template_id: u16) -> Case {
    let (template, _) = Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    );
    let mut builder = ProgramBuilder::new();
    let instructions = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
    let oracle = builder.account(0, None, None, 128);
    builder.input(VALUE_U64, 0);
    let zero = builder.const_u64(0);
    let one = builder.const_u64(1);
    let at_price = builder.const_u64(64);
    let current = builder.introspect(OP_INSTRUCTION_INDEX, instructions, NO_INDEX, NO_INDEX);
    let input = builder.read_instruction_bytes(instructions, current, one, 8);
    let checks = [
        (
            builder.introspect(OP_INSTRUCTION_COUNT, instructions, NO_INDEX, NO_INDEX),
            builder.const_u64(1),
        ),
        (
            builder.introspect(OP_INSTRUCTION_PROGRAM, instructions, current, NO_INDEX),
            builder.const_pubkey(ID.to_bytes()),
        ),
        (
            builder.introspect(OP_INSTRUCTION_ACCOUNT_COUNT, instructions, current, NO_INDEX),
            builder.const_u64(3),
        ),
        (
            builder.introspect(OP_INSTRUCTION_ACCOUNT, instructions, current, zero),
            builder.const_pubkey(template.to_bytes()),
        ),
        (
            builder.introspect(OP_INSTRUCTION_ACCOUNT_FLAGS, instructions, current, zero),
            zero,
        ),
        (
            builder.introspect(OP_INSTRUCTION_DATA_LEN, instructions, current, NO_INDEX),
            builder.const_u64(9),
        ),
        (
            builder.read_instruction_data(OP_READ_U8, instructions, current, zero),
            builder.const_u64(IX_RUN as u64),
        ),
        (input, builder.read_account_bytes(oracle, at_price, 8)),
        (builder.bytes_len(input), builder.const_u64(8)),
    ];
    for (value, expected) in checks {
        let same = builder.binary(OP_EQ, value, expected);
        builder.require(same);
    }
    let (oracle_key, oracle_account) = with_data(template_id * 100 + 1);
    let mut case = run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(AccountMeta::new_readonly(oracle_key, false), oracle_account)],
        7u64.to_le_bytes().to_vec(),
    );
    // Mollusk builds the Instructions sysvar from the instruction when the case does not supply it.
    case.instruction
        .accounts
        .insert(1, AccountMeta::new_readonly(sysvar::instructions::id(), false));
    case
}

/// A run that opens an existing registry entry, reads two fields and writes both back: the steady
/// state of a rate limit, after the first run has created the entry.
fn registry_update(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let key = builder.account_key(payer);
    builder.open_registry(entry, Some(key), payer, 0, 16, system);
    let spent = builder.read_registry(entry, 0, OP_READ_U64);
    builder.read_registry(entry, 8, OP_READ_I64);
    let amount = builder.const_u64(100);
    let total = builder.binary(OP_ADD, spent, amount);
    builder.write_registry(entry, 0, OP_READ_U64, total);
    let now = builder.clock_timestamp();
    builder.write_registry(entry, 8, OP_READ_I64, now);

    let (template, _) = Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    );
    let (payer_key, payer_account) = funded(template_id * 100 + 1);
    let (address, _) = Pubkey::find_program_address(
        &[b"registry", template.as_ref(), &[0], payer_key.as_ref()],
        &ID,
    );
    let mut data = vec![0u8; 72 + 16];
    data[..8].copy_from_slice(b"BREG\x01\x00\x00\x00");
    data[8..40].copy_from_slice(template.as_ref());
    data[40..72].copy_from_slice(payer_key.as_ref());
    let mut entry_account = Account::new(1_503_360, data.len(), &ID);
    entry_account.data = data;
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![
            (AccountMeta::new_readonly(system_program::id(), false), system_program_account()),
            (AccountMeta::new(payer_key, true), payer_account),
            (AccountMeta::new(address, false), entry_account),
        ],
        Vec::new(),
    )
}

/// The exclusion check a route needs: no member of the group is a Token account owned by the
/// signer, other than the declared destination. Eight members, the destination among them and
/// seven pool vaults owned by others, so every member reaches the owner-field comparison.
fn group_filter(creator: Pubkey, template_id: u16) -> Case {
    const TOKEN: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    let mut builder = ProgramBuilder::new();
    let user = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let destination = builder.account(0, None, None, 0);
    builder.account_groups(1);
    let user_key = builder.account_key(user);
    let destination_key = builder.account_key(destination);
    let found = builder.group_filter(
        false,
        0,
        &[TOKEN.to_bytes()],
        &[(32, DATA_REG_PUBKEY, user_key)],
        &[destination_key],
        165,
    );
    let none = builder.not(found);
    builder.require(none);

    let token_account = |owner: Pubkey| {
        let mut account = Account::new(2_039_280, 165, &TOKEN);
        account.data[32..64].copy_from_slice(owner.as_ref());
        account
    };
    let (user_key, user_account) = funded(template_id * 100 + 1);
    let destination_key = key(template_id * 100 + 2);
    let mut runtime = vec![
        (AccountMeta::new_readonly(user_key, true), user_account),
        (AccountMeta::new_readonly(destination_key, false), token_account(user_key)),
        (AccountMeta::new(destination_key, false), token_account(user_key)),
    ];
    for index in 0..7 {
        let vault = key(template_id * 100 + 10 + index);
        let pool = key(template_id * 100 + 30 + index);
        runtime.push((AccountMeta::new(vault, false), token_account(pool)));
    }
    run_case(creator, template_id, builder.build().expect("builds"), runtime, vec![8])
}
