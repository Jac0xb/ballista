//! Compute-unit benchmarks for the example cookbook.
//!
//! `pnpm benchmarks` compiles every example in `docs/examples/` and writes `fixtures/benchmarks.json`
//! with each template's bytes, its runtime account roles, and the plain instruction sequence a
//! caller would send without Ballista. This module materializes those accounts, runs both under
//! Mollusk, and writes `fixtures/benchmark-results.json`, which `scripts/benchmark-tables.mjs`
//! renders into the tables under each example.
//!
//! Where an example names a third-party protocol, the callee is the System Program with a padded
//! transfer: the CPI is real, and the protocol's own work is out of scope on both sides of the
//! comparison.
#![cfg(test)]

use std::collections::HashMap;

use ballista_common::instruction::{IX_CREATE_TEMPLATE, IX_RUN};
use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk, MolluskContext};
use mollusk_svm_programs_token::{associated_token, token};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_option::COption;
use solana_pubkey::{pubkey, Pubkey};
use solana_sdk_ids::system_program;
use spl_token_interface::state::{Account as TokenAccount, AccountState, Mint};

const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
const TEMPLATE_SEED: &[u8] = b"template";
const MANIFEST: &str = include_str!("../../../fixtures/benchmarks.json");
/// A timestamp late enough that every example's deadline and gate comparison is satisfiable.
const NOW: i64 = 1_800_000_000;

/// One runtime account the manifest asked for, and the account state it starts from.
struct Materialized {
    address: Pubkey,
    account: Option<Account>,
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn find_template_pda(creator: &Pubkey, template_id: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    )
}

fn token_account(mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    let state = TokenAccount {
        mint,
        owner,
        amount,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    token::create_account_for_token_account(state)
}

/// A protocol state account: System-owned bytes with a `true` flag at offset 0 and `1` at offset 8,
/// which satisfies both the oracle band and the governance gate the examples read.
fn state_account() -> Account {
    let mut account = Account::new(1_000_000, 128, &system_program::id());
    account.data[0] = 1;
    account.data[8..16].copy_from_slice(&1i64.to_le_bytes());
    account
}

fn associated_token_address(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), token::ID.as_ref(), mint.as_ref()],
        &associated_token::ID,
    )
    .0
}

/// Turns the manifest's role list into concrete accounts. Every `signer` role resolves to the same
/// key, which lets one account own the token accounts it transfers from and closes.
fn materialize(roles: &[String], signer: Pubkey, mint: Pubkey) -> Vec<Materialized> {
    let mut out: Vec<Materialized> = Vec::with_capacity(roles.len());
    for (index, role) in roles.iter().enumerate() {
        let fresh = Pubkey::new_unique();
        let entry = match role.as_str() {
            "system-program" => Materialized { address: system_program::id(), account: None },
            "token-program" => Materialized { address: token::ID, account: None },
            "ata-program" => Materialized { address: associated_token::ID, account: None },
            "signer" => Materialized { address: signer, account: None },
            "mint" => Materialized { address: mint, account: None },
            "wallet" | "recipient" => Materialized {
                address: fresh,
                account: Some(Account::new(1_000_000_000, 0, &system_program::id())),
            },
            "source-tokens" => Materialized {
                address: fresh,
                account: Some(token_account(mint, signer, 1_000_000_000)),
            },
            "recipient-tokens" | "empty-tokens" => Materialized {
                address: fresh,
                account: Some(token_account(mint, signer, 0)),
            },
            "recipient-ata" => {
                let owner = out[..index]
                    .iter()
                    .rev()
                    .zip(roles[..index].iter().rev())
                    .find(|(_, role)| role.as_str() == "recipient" || role.as_str() == "wallet")
                    .map(|(entry, _)| entry.address)
                    .expect("an ATA role follows the wallet it belongs to");
                Materialized {
                    address: associated_token_address(&owner, &mint),
                    // Empty: the template's guard is what decides whether it gets created.
                    account: Some(Account::new(0, 0, &system_program::id())),
                }
            }
            "state-account" => Materialized { address: fresh, account: Some(state_account()) },
            "position-pda" => {
                let (address, _) = Pubkey::find_program_address(
                    &[b"position", signer.as_ref(), &7u64.to_le_bytes()],
                    &system_program::id(),
                );
                Materialized { address, account: Some(state_account()) }
            }
            other => panic!("unknown benchmark role {other}"),
        };
        out.push(entry);
    }
    out
}

fn benchmark_context(accounts: HashMap<Pubkey, Account>) -> MolluskContext<HashMap<Pubkey, Account>> {
    let mut mollusk = Mollusk::default();
    mollusk.sysvars.clock.unix_timestamp = NOW;
    mollusk.add_program_with_loader_and_elf(&ID, &LOADER_V3, BALLISTA_ELF);
    token::add_program(&mut mollusk);
    associated_token::add_program(&mut mollusk);
    mollusk.with_context(accounts)
}

fn program_address(role: &str) -> Pubkey {
    match role {
        "system-program" => system_program::id(),
        "token-program" => token::ID,
        "ata-program" => associated_token::ID,
        other => panic!("unknown baseline program role {other}"),
    }
}

#[test]
fn measure_every_example() {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).expect("benchmark manifest");
    let cases = manifest.as_object().expect("manifest is an object");
    let mut results = serde_json::Map::new();
    let mut failures: Vec<String> = Vec::new();

    for (template_id, (name, case)) in cases.iter().enumerate() {
        let signer = Pubkey::new_unique();
        let mint_address = Pubkey::new_unique();
        let roles: Vec<String> = case["runtimeAccounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect();
        let accounts = materialize(&roles, signer, mint_address);

        let mut store: HashMap<Pubkey, Account> = HashMap::new();
        store.insert(signer, Account::new(100_000_000_000, 0, &system_program::id()));
        store.insert(
            mint_address,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(signer),
                supply: 1_000_000_000,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        for entry in &accounts {
            if let Some(account) = &entry.account {
                store.insert(entry.address, account.clone());
            }
        }
        let run_context = benchmark_context(store);

        // Upload the template. Every example fits one-shot at these sizes.
        let payload = decode_hex(case["templateHex"].as_str().unwrap());
        let template_id = template_id as u16 + 1;
        let (template, _) = find_template_pda(&signer, template_id);
        let mut create_data = Vec::with_capacity(35 + payload.len());
        create_data.push(IX_CREATE_TEMPLATE);
        create_data.extend_from_slice(&template_id.to_le_bytes());
        create_data.extend_from_slice(&solana_sha256_hasher::hash(&payload).to_bytes());
        create_data.extend_from_slice(&payload);
        let created = run_context.process_instruction(&Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(signer, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: create_data,
        });
        if created.program_result.is_err() {
            failures.push(format!("{name}: create failed: {created:#?}"));
            continue;
        }

        // The Ballista run.
        let flags = case["runtimeAccountFlags"].as_array().unwrap();
        let metas: Vec<AccountMeta> = accounts
            .iter()
            .zip(flags)
            .map(|(entry, flag)| AccountMeta {
                pubkey: entry.address,
                is_signer: flag["signer"].as_bool().unwrap(),
                is_writable: flag["writable"].as_bool().unwrap(),
            })
            .collect();
        let mut run_data = vec![IX_RUN];
        run_data.extend_from_slice(&decode_hex(case["runData"].as_str().unwrap()));
        let mut run_metas = vec![AccountMeta::new_readonly(template, false)];
        run_metas.extend(metas);
        let run = run_context.process_instruction(&Instruction {
            program_id: ID,
            accounts: run_metas,
            data: run_data,
        });
        if run.program_result.is_err() {
            failures.push(format!("{name}: run failed: {run:#?}"));
            continue;
        }
        let ballista_units = run.compute_units_consumed;

        // The same work as plain instructions, in a fresh context so both start from one state.
        let mut baseline_store: HashMap<Pubkey, Account> = HashMap::new();
        baseline_store.insert(signer, Account::new(100_000_000_000, 0, &system_program::id()));
        baseline_store.insert(
            mint_address,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(signer),
                supply: 1_000_000_000,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        for entry in &accounts {
            if let Some(account) = &entry.account {
                baseline_store.insert(entry.address, account.clone());
            }
        }
        let baseline_context = benchmark_context(baseline_store);
        let mut baseline_units = 0u64;
        let mut baseline_failed = None;
        for instruction in case["baseline"]["instructions"].as_array().unwrap() {
            let metas: Vec<AccountMeta> = instruction["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| AccountMeta {
                    pubkey: accounts[entry["index"].as_u64().unwrap() as usize].address,
                    is_signer: entry["signer"].as_bool().unwrap(),
                    is_writable: entry["writable"].as_bool().unwrap(),
                })
                .collect();
            let result = baseline_context.process_instruction(&Instruction {
                program_id: program_address(instruction["program"].as_str().unwrap()),
                accounts: metas,
                data: decode_hex(instruction["data"].as_str().unwrap()),
            });
            baseline_units += result.compute_units_consumed;
            if result.program_result.is_err() {
                baseline_failed = Some(format!("{result:#?}"));
                break;
            }
        }

        let mut entry = serde_json::Map::new();
        entry.insert("ballistaComputeUnits".into(), ballista_units.into());
        entry.insert("baselineComputeUnits".into(), baseline_units.into());
        entry.insert("baselineSucceeded".into(), baseline_failed.is_none().into());
        results.insert(name.clone(), entry.into());
        if let Some(error) = baseline_failed {
            eprintln!("{name}: baseline sequence failed (recorded): {error}");
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert_eq!(results.len(), cases.len());
    // Running the examples is a regression check on every build; the recorded numbers are only
    // rewritten on request, so a CI run leaves the tree clean.
    if std::env::var("UPDATE_BENCHMARKS").as_deref() == Ok("1") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/benchmark-results.json");
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&results).unwrap()),
        )
        .expect("write benchmark results");
    }
    for (name, entry) in &results {
        eprintln!(
            "{name}: ballista {} CU, baseline {} CU",
            entry["ballistaComputeUnits"], entry["baselineComputeUnits"]
        );
    }
}
