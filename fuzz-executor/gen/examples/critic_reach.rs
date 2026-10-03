//! Critic tooling: what a template generator actually produces.
//!
//! - `critic_reach fv3 N`: N templates and scenarios from this crate's generators (seeds 0..N, as
//!   the Mollusk fuzz loop draws them), with the scenario's aliasing shapes.
//! - `critic_reach hex FILE...`: one hex payload per line (FV2 and FV4 dumps, corpora).
//!
//! Only payloads that parse and verify are counted. Prints which of the 76 opcodes never appear,
//! the limits the generator reaches, and the CPI, registry and constant shapes.

use std::collections::BTreeMap;

use ballista_common::template::*;
use ballista_fuzz_gen::scenario::{generate_scenario, Kind};
use ballista_fuzz_gen::source::{Gen, SplitMix64};
use ballista_fuzz_gen::template::{generate_template, Role, World};

const OPCODES: &[(u8, &str)] = &[
    (OP_LOAD_INPUT, "LOAD_INPUT"),
    (OP_CONST_BOOL, "CONST_BOOL"),
    (OP_CONST_U64, "CONST_U64"),
    (OP_CONST_I64, "CONST_I64"),
    (OP_CONST_U128, "CONST_U128"),
    (OP_CONST_PUBKEY, "CONST_PUBKEY"),
    (OP_CONST_BYTES, "CONST_BYTES"),
    (OP_ACCOUNT_KEY, "ACCOUNT_KEY"),
    (OP_ACCOUNT_OWNER, "ACCOUNT_OWNER"),
    (OP_ACCOUNT_LAMPORTS, "ACCOUNT_LAMPORTS"),
    (OP_ACCOUNT_DATA_LEN, "ACCOUNT_DATA_LEN"),
    (OP_ACCOUNT_IS_EMPTY, "ACCOUNT_IS_EMPTY"),
    (OP_READ_U64, "READ_U64"),
    (OP_READ_I64, "READ_I64"),
    (OP_READ_U128, "READ_U128"),
    (OP_READ_PUBKEY, "READ_PUBKEY"),
    (OP_CLOCK_SLOT, "CLOCK_SLOT"),
    (OP_CLOCK_TIMESTAMP, "CLOCK_TIMESTAMP"),
    (OP_ADD, "ADD"),
    (OP_SUB, "SUB"),
    (OP_MUL, "MUL"),
    (OP_DIV, "DIV"),
    (OP_EQ, "EQ"),
    (OP_NE, "NE"),
    (OP_LT, "LT"),
    (OP_LTE, "LTE"),
    (OP_GT, "GT"),
    (OP_GTE, "GTE"),
    (OP_AND, "AND"),
    (OP_OR, "OR"),
    (OP_NOT, "NOT"),
    (OP_MIN, "MIN"),
    (OP_MAX, "MAX"),
    (OP_SELECT, "SELECT"),
    (OP_CAST_U64, "CAST_U64"),
    (OP_CAST_I64, "CAST_I64"),
    (OP_CAST_U128, "CAST_U128"),
    (OP_LOOP_INDEX, "LOOP_INDEX"),
    (OP_REQUIRE, "REQUIRE"),
    (OP_INVOKE, "INVOKE"),
    (OP_FOREACH, "FOREACH"),
    (OP_READ_U8, "READ_U8"),
    (OP_READ_U16, "READ_U16"),
    (OP_READ_U32, "READ_U32"),
    (OP_READ_BOOL, "READ_BOOL"),
    (OP_DERIVE_PDA, "DERIVE_PDA"),
    (OP_RETURN_DATA, "RETURN_DATA"),
    (OP_MOVE, "MOVE"),
    (OP_CREATE_PDA, "CREATE_PDA"),
    (OP_MUL_DIV, "MUL_DIV"),
    (OP_MUL_DIV_CEIL, "MUL_DIV_CEIL"),
    (OP_REM, "REM"),
    (OP_SHL, "SHL"),
    (OP_SHR, "SHR"),
    (OP_BIT_AND, "BIT_AND"),
    (OP_BIT_OR, "BIT_OR"),
    (OP_BIT_XOR, "BIT_XOR"),
    (OP_POW10, "POW10"),
    (OP_READ_I32, "READ_I32"),
    (OP_REPEAT, "REPEAT"),
    (OP_EMIT, "EMIT"),
    (OP_SET_RETURN_DATA, "SET_RETURN_DATA"),
    (OP_INSTRUCTION_COUNT, "INSTRUCTION_COUNT"),
    (OP_INSTRUCTION_INDEX, "INSTRUCTION_INDEX"),
    (OP_INSTRUCTION_PROGRAM, "INSTRUCTION_PROGRAM"),
    (OP_INSTRUCTION_ACCOUNT_COUNT, "INSTRUCTION_ACCOUNT_COUNT"),
    (OP_INSTRUCTION_ACCOUNT, "INSTRUCTION_ACCOUNT"),
    (OP_INSTRUCTION_ACCOUNT_FLAGS, "INSTRUCTION_ACCOUNT_FLAGS"),
    (OP_INSTRUCTION_DATA_LEN, "INSTRUCTION_DATA_LEN"),
    (OP_READ_INSTRUCTION_DATA, "READ_INSTRUCTION_DATA"),
    (OP_READ_INSTRUCTION_BYTES, "READ_INSTRUCTION_BYTES"),
    (OP_READ_ACCOUNT_BYTES, "READ_ACCOUNT_BYTES"),
    (OP_BYTES_LEN, "BYTES_LEN"),
    (OP_OPEN_REGISTRY, "OPEN_REGISTRY"),
    (OP_READ_REGISTRY, "READ_REGISTRY"),
    (OP_WRITE_REGISTRY, "WRITE_REGISTRY"),
];

#[derive(Default)]
struct Tally {
    payloads: usize,
    verified: usize,
    opcode_templates: BTreeMap<u8, usize>,
    max: BTreeMap<&'static str, u64>,
    count: BTreeMap<&'static str, usize>,
    worst_cpis: BTreeMap<u8, usize>,
}

impl Tally {
    fn max(&mut self, key: &'static str, value: u64) {
        let entry = self.max.entry(key).or_default();
        *entry = (*entry).max(value);
    }
    fn hit(&mut self, key: &'static str, yes: bool) {
        *self.count.entry(key).or_default() += usize::from(yes);
    }

    fn payload(&mut self, bytes: &[u8]) {
        self.payloads += 1;
        let Ok(view) = ProgramView::parse(bytes) else {
            return;
        };
        let Ok(stats) = view.verify() else { return };
        self.verified += 1;
        let header = view.header;
        self.max("payload bytes", bytes.len() as u64);
        self.max("instructions", view.instructions.len() as u64);
        self.max("registers", header.register_count() as u64);
        self.max("fixed accounts", header.fixed_account_count() as u64);
        self.max("batch stride", header.batch_stride() as u64);
        self.max("batch max rows", header.batch_max_iterations() as u64);
        self.max("inputs (fixed)", header.input_count() as u64);
        self.max("row inputs", header.row_input_count() as u64);
        self.max("account groups", header.account_group_count() as u64);
        self.max("worst-case CPIs", stats.max_expanded_cpis as u64);
        self.max("CPI data max len", stats.max_cpi_data_len as u64);
        *self.worst_cpis.entry(stats.max_expanded_cpis).or_default() += 1;
        self.hit(
            "templates at 128 instructions",
            view.instructions.len() == MAX_VM_INSTRUCTIONS,
        );
        self.hit(
            "templates with >=120 instructions",
            view.instructions.len() >= 120,
        );
        self.hit(
            "templates at 64 registers",
            header.register_count() == MAX_REGISTERS,
        );
        self.hit(
            "templates at 64 worst-case CPIs",
            stats.max_expanded_cpis as usize == MAX_EXPANDED_CPIS,
        );
        self.hit(
            "templates with an account group",
            header.account_group_count() > 0,
        );
        self.hit(
            "templates with 2+ account groups",
            header.account_group_count() >= 2,
        );
        self.hit("templates with a batch", header.batch_stride() > 0);
        self.hit(
            "templates with the event flag",
            header.flags() & PROGRAM_FLAG_EMIT_EVENT != 0,
        );

        let mut seen = [false; 256];
        let mut loops = 0u64;
        let mut first_invoke = usize::MAX;
        let mut opens_pc = Vec::new();
        let mut open_indexes = Vec::new();
        for (pc, record) in view.instructions.iter().enumerate() {
            seen[record.opcode as usize] = true;
            match record.opcode {
                OP_FOREACH | OP_REPEAT => {
                    loops += 1;
                    let carried = record.immediate().count_ones() as u64;
                    self.max("registers carried by one loop", carried);
                    if record.opcode == OP_REPEAT {
                        self.max("REPEAT max passes", record.c as u64);
                    }
                    self.max("loop body length", record.a as u64);
                }
                OP_INVOKE => {
                    first_invoke = first_invoke.min(pc);
                    self.hit("INVOKE sites", true);
                    self.hit("guarded INVOKE sites", record.b != NO_INDEX);
                }
                OP_OPEN_REGISTRY => {
                    opens_pc.push(pc);
                    // The registry index rides in the immediate's low byte; collect distinct ones.
                    open_indexes.push(record.immediate() & 0xff);
                    self.hit("OPEN_REGISTRY with zero key", record.b == NO_INDEX);
                }
                OP_CONST_U64 => {
                    self.hit("CONST_U64 == u64::MAX", record.immediate() == u64::MAX);
                    self.hit("CONST_U64 > u32::MAX", record.immediate() > u32::MAX as u64);
                }
                OP_CONST_I64 => {
                    let value = record.immediate() as i64;
                    self.hit("CONST_I64 < 0", value < 0);
                    self.hit("CONST_I64 == i64::MIN", value == i64::MIN);
                }
                OP_CONST_U128 => {
                    let (offset, len) = record.blob_range();
                    if let Some(blob) = view.blob.get(offset..offset + len) {
                        if let Ok(array) = <[u8; 16]>::try_from(blob) {
                            let value = u128::from_le_bytes(array);
                            self.hit("CONST_U128 > u64::MAX", value > u64::MAX as u128);
                            self.hit("CONST_U128 == u128::MAX", value == u128::MAX);
                        }
                    }
                }
                _ => {}
            }
        }
        self.max("loops", loops);
        self.max("registry opens", opens_pc.len() as u64);
        open_indexes.sort();
        open_indexes.dedup();
        self.max("distinct registry indexes", open_indexes.len() as u64);
        self.hit("templates with an open", !opens_pc.is_empty());
        self.hit("templates with 2+ opens", opens_pc.len() >= 2);
        self.hit(
            "templates with 2+ distinct registries",
            open_indexes.len() >= 2,
        );
        self.hit(
            "templates with an INVOKE before an open",
            opens_pc.iter().any(|pc| *pc > first_invoke),
        );
        for (opcode, _) in OPCODES {
            if seen[*opcode as usize] {
                *self.opcode_templates.entry(*opcode).or_default() += 1;
            }
        }

        for constraint in view.accounts {
            self.hit("declared accounts", true);
            self.hit("declared signer", constraint.flags & ACCOUNT_SIGNER != 0);
            self.hit(
                "declared writable",
                constraint.flags & ACCOUNT_WRITABLE != 0,
            );
            self.hit(
                "declared executable",
                constraint.flags & ACCOUNT_EXECUTABLE != 0,
            );
            self.hit("address pin", constraint.address_index != NO_INDEX);
            self.hit("owner pin", constraint.owner_index != NO_INDEX);
            self.hit("min data len", constraint.min_data_len() > 0);
        }
        for cpi in view.cpis {
            let records = &view.cpi_accounts
                [cpi.account_start()..cpi.account_start() + cpi.account_len as usize];
            self.max("accounts listed by one CPI", records.len() as u64);
            self.hit("CPI descriptors", true);
            self.hit("CPI forwarding a group", cpi.account_group().is_some());
            self.hit(
                "CPI with a row-account program",
                cpi.program_account & ITERATION_ACCOUNT_BIT != 0,
            );
            let mut slots: Vec<u8> = records.iter().map(|r| r.account).collect();
            let listed = slots.len();
            slots.sort();
            slots.dedup();
            self.hit("CPI listing one slot twice", slots.len() != listed);
            for record in records {
                self.hit("CPI records", true);
                self.hit("CPI record signer", record.flags & ACCOUNT_SIGNER != 0);
                self.hit("CPI record writable", record.flags & ACCOUNT_WRITABLE != 0);
                self.hit("CPI record signer+writable", record.flags & 3 == 3);
                self.hit(
                    "CPI record row account",
                    record.account & ITERATION_ACCOUNT_BIT != 0,
                );
            }
        }
        for input in view.inputs {
            if input.value_type == VALUE_BYTES {
                self.max("bytes input max_len", input.max_len() as u64);
            }
        }
    }

    fn print(&self, label: &str) {
        println!(
            "== {label}: {} payloads, {} verified",
            self.payloads, self.verified
        );
        let never: Vec<&str> = OPCODES
            .iter()
            .filter(|(opcode, _)| !self.opcode_templates.contains_key(opcode))
            .map(|(_, name)| *name)
            .collect();
        println!(
            "opcodes never produced: {} of {}: {:?}",
            never.len(),
            OPCODES.len(),
            never
        );
        let rare: Vec<String> = OPCODES
            .iter()
            .filter_map(|(opcode, name)| self.opcode_templates.get(opcode).map(|n| (name, *n)))
            .filter(|(_, n)| (*n as f64) < self.verified as f64 * 0.005)
            .map(|(name, n)| format!("{name}:{n}"))
            .collect();
        println!("opcodes in <0.5% of templates: {rare:?}");
        for (key, value) in &self.max {
            println!("  max {key}: {value}");
        }
        for (key, value) in &self.count {
            println!("  count {key}: {value}");
        }
        let top: Vec<String> = self
            .worst_cpis
            .iter()
            .rev()
            .take(6)
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        println!("  worst-case CPIs, highest (value:templates): {top:?}");
    }
}

#[derive(Default)]
struct Aliasing {
    scenarios: usize,
    any_duplicate: usize,
    entry_in_two_slots: usize,
    entry_aliased_by_plain_slot: usize,
    entry_aliased_by_plain_before_open_cpi: usize,
    writable_and_readonly_alias: usize,
    wrapped: usize,
    mutated: usize,
    zero_rows: usize,
    with_groups: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("fv3") => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000);
            let ballista = [0xba; 32];
            let probe = [0x9b; 32];
            let token = [0x70; 32];
            let world = World::new(ballista, probe, token);
            let mut tally = Tally::default();
            let mut aliasing = Aliasing::default();
            let mut fell_back = 0;
            let rent = |len: usize| 890_880 + 6_960 * len as u64;
            // Any deterministic stand-in: only the shape of the scenario is counted.
            let pda = |seeds: &[&[u8]], program: &[u8; 32]| {
                let mut address = *program;
                for (i, seed) in seeds.iter().enumerate() {
                    for (j, byte) in seed.iter().enumerate() {
                        address[(i * 7 + j) % 32] ^= byte.rotate_left((i + j) as u32 % 8);
                    }
                }
                (address, 255u8)
            };
            for seed in 0..n {
                let mut source = SplitMix64::new(seed);
                let mut gen = Gen::new(&mut source);
                let plan = generate_template(&mut gen, &world);
                fell_back += usize::from(plan.fell_back);
                tally.payload(&plan.bytes);
                let template = [0x7e; 32];
                let scenario = generate_scenario(&mut gen, &world, &plan, template, &pda, &rent);
                aliasing.scenarios += 1;
                let mut seen: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                for (slot, &pool) in scenario.slots.iter().enumerate() {
                    seen.entry(pool).or_default().push(slot);
                }
                let dup = seen.values().any(|slots| slots.len() > 1);
                aliasing.any_duplicate += usize::from(dup);
                let mut entry_two = false;
                let mut entry_plain = false;
                let mut rw_alias = false;
                for slots in seen.values().filter(|slots| slots.len() > 1) {
                    let roles: Vec<Option<&Role>> = slots
                        .iter()
                        .map(|&s| plan.fixed.get(s).map(|slot| &slot.role))
                        .collect();
                    let entries = roles
                        .iter()
                        .filter(|r| matches!(r, Some(Role::Entry(_))))
                        .count();
                    if entries >= 1 && slots.len() >= 2 {
                        entry_two = true;
                    }
                    if entries >= 1 && roles.iter().any(|r| !matches!(r, Some(Role::Entry(_)))) {
                        entry_plain = true;
                    }
                    let flags: Vec<u8> = slots
                        .iter()
                        .filter_map(|&s| plan.fixed.get(s).map(|slot| slot.flags))
                        .collect();
                    if flags.iter().any(|f| f & ACCOUNT_WRITABLE != 0)
                        && flags.iter().any(|f| f & ACCOUNT_WRITABLE == 0)
                    {
                        rw_alias = true;
                    }
                }
                let entry_kinds = scenario
                    .pool
                    .iter()
                    .filter(|spec| matches!(spec.kind, Kind::Entry { .. }))
                    .count();
                let _ = entry_kinds;
                aliasing.entry_in_two_slots += usize::from(entry_two);
                aliasing.entry_aliased_by_plain_slot += usize::from(entry_plain);
                // The FV3 finding needs the alias plus a CPI before the open.
                let view = ProgramView::parse(&plan.bytes).ok();
                let invoke_before_open = view.is_some_and(|view| {
                    let first_invoke = view.instructions.iter().position(|r| r.opcode == OP_INVOKE);
                    let last_open = view
                        .instructions
                        .iter()
                        .rposition(|r| r.opcode == OP_OPEN_REGISTRY);
                    matches!((first_invoke, last_open), (Some(i), Some(o)) if i < o)
                });
                aliasing.entry_aliased_by_plain_before_open_cpi +=
                    usize::from(entry_plain && invoke_before_open);
                aliasing.writable_and_readonly_alias += usize::from(rw_alias);
                aliasing.wrapped += usize::from(scenario.wrap.is_some());
                aliasing.mutated += usize::from(scenario.mutation.is_some());
                aliasing.zero_rows += usize::from(plan.batch_max > 0 && scenario.iterations == 0);
                aliasing.with_groups += usize::from(scenario.group_lengths.iter().any(|&n| n > 0));
            }
            tally.print("FV3 fuzz-executor/gen");
            println!("  generator fallbacks: {fell_back}");
            let a = &aliasing;
            println!(
                "  scenarios {}: any duplicate account {}, an entry in two slots {}, an entry also in a non-entry slot {}, \
                 of those with an INVOKE before an open {}, one account declared writable in one slot and read-only in another {}, \
                 wrapped {}, mutated {}, batch with zero rows {}, non-empty groups {}",
                a.scenarios, a.any_duplicate, a.entry_in_two_slots, a.entry_aliased_by_plain_slot,
                a.entry_aliased_by_plain_before_open_cpi, a.writable_and_readonly_alias, a.wrapped, a.mutated, a.zero_rows, a.with_groups
            );
        }
        Some("hex") => {
            let mut tally = Tally::default();
            for path in &args[2..] {
                let text = std::fs::read_to_string(path).expect("readable");
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let bytes: Vec<u8> = (0..line.len() / 2)
                        .filter_map(|i| u8::from_str_radix(&line[2 * i..2 * i + 2], 16).ok())
                        .collect();
                    tally.payload(&bytes);
                }
            }
            tally.print(&args[2..].join(","));
        }
        _ => eprintln!("usage: critic_reach fv3 N | critic_reach hex FILE..."),
    }
}
