//! Every Whirlpool call the Orca templates make passes the accounts that Orca's own client passes
//! for the same instruction: the same accounts, in the same order, with the same signer and
//! writable flags.
//!
//! A run cannot check the flags. The runtime accepts an account passed with more privilege than
//! the callee declares, so a writable flag the callee does not need is never refused; it only
//! write-locks the account. So this reads the compiled templates.

use {
    ballista_protocol_tests::{orca::WHIRLPOOL, template::examples},
    ballista_sdk::ballista_common::template::{
        ProgramView, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, ITERATION_ACCOUNT_BIT,
    },
    orca_whirlpools_client as oc,
    solana_address::Address,
    solana_instruction::AccountMeta,
    std::collections::HashMap,
};

/// A stand-in address per account name, so a list built from names compares by name.
fn key(name: &str) -> Address {
    assert!(name.len() <= 32, "{name} is longer than an address");
    let mut bytes = [0; 32];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    Address::new_from_array(bytes)
}

fn name_of(address: &Address) -> String {
    String::from_utf8_lossy(address.as_ref())
        .trim_end_matches('\0')
        .to_string()
}

/// The instruction's name and the accounts Orca's client builds for it, each named as the
/// templates name it.
fn orca_accounts(discriminator: &[u8]) -> (&'static str, Vec<AccountMeta>) {
    let k = key;
    if discriminator == oc::COLLECT_FEES_DISCRIMINATOR {
        let instruction = oc::CollectFees {
            whirlpool: k("whirlpool"),
            position_authority: k("positionAuthority"),
            position: k("position"),
            position_token_account: k("positionTokenAccount"),
            token_owner_account_a: k("tokenOwnerAccountA"),
            token_vault_a: k("tokenVaultA"),
            token_owner_account_b: k("tokenOwnerAccountB"),
            token_vault_b: k("tokenVaultB"),
            token_program: k("tokenProgram"),
        }
        .instruction();
        ("collect_fees", instruction.accounts)
    } else if discriminator == oc::UPDATE_FEES_AND_REWARDS_DISCRIMINATOR {
        let instruction = oc::UpdateFeesAndRewards {
            whirlpool: k("whirlpool"),
            position: k("position"),
            tick_array_lower: k("tickArrayLower"),
            tick_array_upper: k("tickArrayUpper"),
        }
        .instruction();
        ("update_fees_and_rewards", instruction.accounts)
    } else if discriminator == oc::INCREASE_LIQUIDITY_DISCRIMINATOR {
        let instruction = oc::IncreaseLiquidity {
            whirlpool: k("whirlpool"),
            token_program: k("tokenProgram"),
            position_authority: k("positionAuthority"),
            position: k("position"),
            position_token_account: k("positionTokenAccount"),
            token_owner_account_a: k("tokenOwnerAccountA"),
            token_owner_account_b: k("tokenOwnerAccountB"),
            token_vault_a: k("tokenVaultA"),
            token_vault_b: k("tokenVaultB"),
            tick_array_lower: k("tickArrayLower"),
            tick_array_upper: k("tickArrayUpper"),
        }
        .instruction(oc::IncreaseLiquidityInstructionArgs {
            liquidity_amount: 0,
            token_max_a: 0,
            token_max_b: 0,
        });
        ("increase_liquidity", instruction.accounts)
    } else if discriminator == oc::INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2_DISCRIMINATOR {
        let instruction = oc::IncreaseLiquidityByTokenAmountsV2 {
            whirlpool: k("whirlpool"),
            token_program_a: k("tokenProgram"),
            token_program_b: k("tokenProgram"),
            memo_program: k("memoProgram"),
            position_authority: k("positionAuthority"),
            position: k("position"),
            position_token_account: k("positionTokenAccount"),
            token_mint_a: k("tokenMintA"),
            token_mint_b: k("tokenMintB"),
            token_owner_account_a: k("tokenOwnerAccountA"),
            token_owner_account_b: k("tokenOwnerAccountB"),
            token_vault_a: k("tokenVaultA"),
            token_vault_b: k("tokenVaultB"),
            tick_array_lower: k("tickArrayLower"),
            tick_array_upper: k("tickArrayUpper"),
        }
        .instruction(oc::IncreaseLiquidityByTokenAmountsV2InstructionArgs {
            method: oc::IncreaseLiquidityMethod::ByTokenAmounts {
                token_max_a: 0,
                token_max_b: 0,
                min_sqrt_price: 0,
                max_sqrt_price: 0,
            },
            remaining_accounts_info: None,
        });
        (
            "increase_liquidity_by_token_amounts_v2",
            instruction.accounts,
        )
    } else {
        panic!("a Whirlpool instruction with discriminator {discriminator:02x?}: add it here");
    }
}

fn flags(meta: &AccountMeta) -> &'static str {
    match (meta.is_signer, meta.is_writable) {
        (true, true) => "a writable signer",
        (true, false) => "a read-only signer",
        (false, true) => "writable",
        (false, false) => "read-only",
    }
}

#[test]
fn every_whirlpool_call_passes_the_accounts_orcas_client_does() {
    let examples = examples();
    let mut differences = Vec::new();
    for name in ["orcaCompoundFees", "orcaHarvestManyPositions"] {
        let example = &examples[name];
        let view = ProgramView::parse(&example.payload).unwrap();
        let names: HashMap<u8, &str> = example
            .fixed_accounts
            .iter()
            .enumerate()
            .map(|(index, name)| (index as u8, name.as_str()))
            .chain(
                example
                    .batch_accounts
                    .iter()
                    .enumerate()
                    .map(|(index, name)| (index as u8 | ITERATION_ACCOUNT_BIT, name.as_str())),
            )
            .collect();
        for cpi in view.cpis {
            let program = &view.accounts[usize::from(cpi.program_account)];
            let pinned = view.pubkeys[usize::from(program.address_index)].bytes;
            assert_eq!(
                pinned,
                WHIRLPOOL.to_bytes(),
                "{name} calls a program other than Whirlpools"
            );
            let segment = &view.data_segments[cpi.segment_start()];
            let discriminator = &view.blob[segment.offset()..segment.offset() + 8];
            let (instruction, expected) = orca_accounts(discriminator);
            let records = &view.cpi_accounts[cpi.account_start()..][..usize::from(cpi.account_len)];
            let passed: Vec<AccountMeta> = records
                .iter()
                .map(|record| AccountMeta {
                    pubkey: key(names[&record.account]),
                    is_signer: record.flags & ACCOUNT_SIGNER != 0,
                    is_writable: record.flags & ACCOUNT_WRITABLE != 0,
                })
                .collect();
            if passed.len() != expected.len() {
                differences.push(format!(
                    "{name} {instruction}: {} accounts, and Orca's client passes {}",
                    passed.len(),
                    expected.len()
                ));
                continue;
            }
            for (index, (passed, expected)) in passed.iter().zip(&expected).enumerate() {
                if passed.pubkey != expected.pubkey {
                    differences.push(format!(
                        "{name} {instruction}: account {index} is {}, and Orca's client passes {}",
                        name_of(&passed.pubkey),
                        name_of(&expected.pubkey)
                    ));
                } else if (passed.is_signer, passed.is_writable)
                    != (expected.is_signer, expected.is_writable)
                {
                    differences.push(format!(
                        "{name} {instruction}: {} is {} here and {} in Orca's client",
                        name_of(&passed.pubkey),
                        flags(passed),
                        flags(expected)
                    ));
                }
            }
        }
    }
    assert!(differences.is_empty(), "\n{}", differences.join("\n"));
}
