//! The Mollusk harness: programs, template upload, scenario execution and CPI capture.

use std::collections::HashMap;

use ballista_common::instruction::IX_CREATE_TEMPLATE;
use ballista_fuzz_gen::scenario::{Kind, Scenario};
use ballista_fuzz_gen::template::{self, Program, World};
use mollusk_svm::program::loader_keys::LOADER_V3;
use mollusk_svm::result::types::TransactionResult;
use mollusk_svm::{Mollusk, MolluskContext};
use mollusk_svm_programs_memo::memo;
use mollusk_svm_programs_token::token;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::{pubkey, Pubkey};
use solana_sdk_ids::system_program;

pub const BALLISTA_ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
/// The probe program's id in Mollusk. Fixed, so the generator's `World` can name it. The bytes
/// start with "PROBE" to stand out in a trace. The probe ELF's own declared id is irrelevant:
/// Mollusk maps the ELF to whichever id it is loaded under.
pub const PROBE_ID: Pubkey = pubkey!("6QYQq7JmVu6vjCGkfovra3pBH5zv8o9HiC5uMrEW7PsF");

pub const CLOCK_TIMESTAMP: i64 = 1_800_000_000;
pub const CLOCK_SLOT: u64 = 310_000_000;

/// The world the generators share with the harness.
pub fn world() -> World {
    World::new(BALLISTA_ID.to_bytes(), PROBE_ID.to_bytes(), token::ID.to_bytes())
}

pub struct Harness {
    pub context: MolluskContext<HashMap<Pubkey, Account>>,
    pub has_probe: bool,
}

/// One run's result and where to find the run's own cross-program invocations in it.
pub struct RunOutcome {
    pub result: TransactionResult,
    /// Index of the top-level instruction that is, or wraps, the run.
    pub top_index: usize,
    /// Stack height of the invocations the run makes directly: 2 for a top-level run, 3 when the
    /// run is itself nested inside a probe instruction.
    pub ballista_cpi_height: u32,
    /// Lamports of every stored account just before the run, so conservation can be checked
    /// against their true pre-run balances (the template's rent is not in the generated pool).
    pub before_lamports: HashMap<Pubkey, u64>,
}

/// One cross-program invocation the run made, decoded from the transaction's inner instructions.
#[derive(Clone, Debug)]
pub struct CapturedCpi {
    pub program: Pubkey,
    pub accounts: Vec<Pubkey>,
    pub data: Vec<u8>,
}

impl Harness {
    pub fn new() -> Self {
        // `Mollusk::default()` enables every SVM feature; the critic's `BALLISTA_MAINNET_FEATURES`
        // switch lives in `cases.rs`, which the fuzzer does not need, so it uses the default set.
        let mut mollusk = Mollusk::default();
        mollusk.sysvars.clock.unix_timestamp = CLOCK_TIMESTAMP;
        mollusk.sysvars.clock.slot = CLOCK_SLOT;
        mollusk.add_program_with_loader_and_elf(&BALLISTA_ID, &LOADER_V3, crate::cases::BALLISTA_ELF);
        token::add_program(&mut mollusk);
        memo::add_program(&mut mollusk);
        let has_probe = match probe_elf() {
            Some(elf) => {
                mollusk.add_program_with_loader_and_elf(&PROBE_ID, &LOADER_V3, &elf);
                true
            }
            None => false,
        };
        let context = mollusk.with_context(HashMap::new());
        Self { context, has_probe }
    }

    pub fn rent_minimum(&self, len: usize) -> u64 {
        self.context.mollusk.sysvars.rent.minimum_balance(len)
    }

    /// Empties the account store, so each seed runs against fresh state.
    pub fn reset(&self) {
        self.context.account_store.borrow_mut().clear();
    }

    /// Uploads and finalizes `payload` in one `CreateTemplate`, returning the template address and
    /// its finalized account, or the instruction result when create failed (a
    /// verifier/generator disagreement the caller surfaces).
    pub fn upload(&self, creator: &Pubkey, template_id: u16, payload: &[u8]) -> Result<(Pubkey, Account), String> {
        let (template, _) = Pubkey::find_program_address(
            &[b"template", creator.as_ref(), &template_id.to_le_bytes()],
            &BALLISTA_ID,
        );
        let mut data = Vec::with_capacity(35 + payload.len());
        data.push(IX_CREATE_TEMPLATE);
        data.extend_from_slice(&template_id.to_le_bytes());
        data.extend_from_slice(&solana_sha256_hasher::hash(payload).to_bytes());
        data.extend_from_slice(payload);
        let instruction = Instruction {
            program_id: BALLISTA_ID,
            accounts: vec![
                AccountMeta::new(*creator, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        };
        {
            let mut store = self.context.account_store.borrow_mut();
            store.insert(*creator, Account::new(10_000_000_000, 0, &system_program::id()));
            store.insert(template, Account::new(0, 0, &system_program::id()));
        }
        let result = self.context.process_instruction(&instruction);
        if !result.program_result.is_ok() {
            return Err(format!("{:?}", result.program_result));
        }
        // The context persists the finalized template to the store on success.
        let account = self
            .context
            .account_store
            .borrow()
            .get(&template)
            .cloned()
            .ok_or_else(|| "template account missing from store after create".to_string())?;
        Ok((template, account))
    }

    /// Runs `scenario` against the finalized template as one transaction: the `before` probe
    /// instructions, the run (optionally wrapped in a probe CPI), then the `after` instructions.
    pub fn run(&self, scenario: &Scenario, template: &Pubkey, finalized: &Account) -> RunOutcome {
        self.seed_store(scenario, template, finalized);
        let mut instructions = Vec::new();
        for extra in &scenario.before {
            instructions.push(self.probe_instruction(scenario, extra));
        }
        let top_index = instructions.len();
        let ballista_cpi_height;
        if let Some(policy) = scenario.wrap {
            instructions.push(self.wrapped_run(scenario, template, policy));
            ballista_cpi_height = 3;
        } else {
            instructions.push(self.run_instruction(scenario, template));
            ballista_cpi_height = 2;
        }
        for extra in &scenario.after {
            instructions.push(self.probe_instruction(scenario, extra));
        }
        let before_lamports: HashMap<Pubkey, u64> = self
            .context
            .account_store
            .borrow()
            .iter()
            .map(|(key, account)| (*key, account.lamports))
            .collect();
        let result = self.context.process_transaction_instructions(&instructions);
        RunOutcome { result, top_index, ballista_cpi_height, before_lamports }
    }

    /// Seeds the store with the scenario's accounts. Programs and the Instructions sysvar are left
    /// to the context to hydrate; the template keeps the finalized account from upload.
    fn seed_store(&self, scenario: &Scenario, template: &Pubkey, finalized: &Account) {
        let mut store = self.context.account_store.borrow_mut();
        store.insert(*template, finalized.clone());
        for spec in &scenario.pool {
            if matches!(spec.kind, Kind::Program(_) | Kind::Sysvar | Kind::Template) {
                continue;
            }
            store.insert(
                Pubkey::new_from_array(spec.address),
                Account {
                    lamports: spec.lamports,
                    data: spec.data.clone(),
                    owner: Pubkey::new_from_array(spec.owner),
                    executable: spec.executable,
                    rent_epoch: 0,
                },
            );
        }
    }

    fn meta(&self, scenario: &Scenario, index: usize) -> AccountMeta {
        let spec = &scenario.pool[index];
        AccountMeta {
            pubkey: Pubkey::new_from_array(spec.address),
            is_signer: spec.signer,
            is_writable: spec.writable,
        }
    }

    fn run_instruction(&self, scenario: &Scenario, template: &Pubkey) -> Instruction {
        let mut accounts = vec![AccountMeta {
            pubkey: *template,
            is_signer: false,
            is_writable: scenario.pool[scenario.template].writable,
        }];
        for &index in &scenario.slots {
            accounts.push(self.meta(scenario, index));
        }
        Instruction { program_id: BALLISTA_ID, accounts, data: scenario.run_data.clone() }
    }

    /// The run as a CPI made by a top-level probe `INVOKE`: the probe forwards the template and the
    /// runtime accounts to Ballista with the run data, demoting writables per `policy`.
    fn wrapped_run(&self, scenario: &Scenario, template: &Pubkey, policy: u8) -> Instruction {
        let mut accounts = vec![
            AccountMeta::new_readonly(BALLISTA_ID, false),
            AccountMeta { pubkey: *template, is_signer: false, is_writable: scenario.pool[scenario.template].writable },
        ];
        for &index in &scenario.slots {
            accounts.push(self.meta(scenario, index));
        }
        let mut data = vec![template::probe::INVOKE, policy];
        data.extend_from_slice(&scenario.run_data);
        Instruction { program_id: PROBE_ID, accounts, data }
    }

    fn probe_instruction(&self, scenario: &Scenario, extra: &ballista_fuzz_gen::scenario::Extra) -> Instruction {
        let accounts = extra.accounts.iter().map(|&index| self.meta(scenario, index)).collect();
        Instruction { program_id: PROBE_ID, accounts, data: extra.data.clone() }
    }
}

/// The run's direct cross-program invocations, decoded from the transaction's inner instructions.
pub fn captured_cpis(outcome: &RunOutcome) -> Vec<CapturedCpi> {
    let result = &outcome.result;
    let Some(message) = &result.message else { return Vec::new() };
    let keys = message.account_keys();
    let Some(inner) = result.inner_instructions.get(outcome.top_index) else { return Vec::new() };
    inner
        .iter()
        .filter(|entry| entry.stack_height == Some(outcome.ballista_cpi_height))
        .map(|entry| {
            let compiled = &entry.instruction;
            CapturedCpi {
                program: *keys.get(compiled.program_id_index as usize).expect("program key"),
                accounts: compiled
                    .accounts
                    .iter()
                    .map(|&index| *keys.get(index as usize).expect("account key"))
                    .collect(),
                data: compiled.data.clone(),
            }
        })
        .collect()
}

/// The program a `Program` maps to, as a `Pubkey`.
pub fn program_pubkey(world: &World, program: Program) -> Pubkey {
    Pubkey::new_from_array(world.program(program))
}

/// Reads the probe ELF the fuzzer calls, built with
/// `cargo build-sbf --manifest-path fuzz-executor/probe/Cargo.toml`. Absent, probe CPIs fail as a
/// runtime error (an allowed outcome) and coverage of the CPI paths drops; present, they run.
fn probe_elf() -> Option<Vec<u8>> {
    if let Ok(path) = std::env::var("BALLISTA_PROBE_SO") {
        return std::fs::read(path).ok();
    }
    let default = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fuzz-executor/target/deploy/ballista_probe.so");
    std::fs::read(default).ok()
}
