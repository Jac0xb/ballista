//! marginfi v2: the instructions setup and the contract tests send, and the account fields the
//! scenarios read. Everything here is written from the deployed program's source,
//! `mrgnlabs/marginfi-v2@33c67987a6` (0.1.11-rc1); each item cites its file there.
//!
//! No crate for it:
//! - marginfi publishes no current instruction crate.
//! - Its type crate cannot be a dependency here: at that commit it pins
//!   `solana-instruction = "=3.4.0"`, and LiteSVM needs `~3.5`.
//!
//! The offsets below were printed by `offset_of!` against the type crate (`Bank` at
//! `type-crate/src/types/bank.rs:30`, `MarginfiAccount` and `Balance` at
//! `type-crate/src/types/user_account.rs:27` and `:286`). `the_offsets_read_the_snapshots_banks`
//! checks them against the real accounts, so a layout change fails there first.

use {
    ballista_sdk::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
};

pub const MARGINFI: Address =
    Address::from_str_const("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA");
/// `OracleSetup::PythPushOracle` (`type-crate/src/types/bank.rs:367`): a bank priced by one Pyth
/// `PriceUpdateV2`, `oracle_keys[0]`.
pub const PYTH_PUSH_ORACLE: u8 = 3;
/// `MarginfiError::InvalidBankAccount` (`programs/marginfi/src/errors.rs:21-22`): the health check
/// was not given a remaining balance's bank.
pub const INVALID_BANK_ACCOUNT: u32 = 6008;

/// Anchor discriminators, `sha256("global:<handler>")[..8]`, of `marginfi_account_initialize`,
/// `lending_account_deposit` and `lending_account_withdraw` (`programs/marginfi/src/lib.rs:295`,
/// `:396`, `:418`). `crate::tests` derives them.
pub(crate) const INITIALIZE: [u8; 8] = [0x2b, 0x4e, 0x3d, 0xff, 0x94, 0x34, 0xf9, 0x9a];
pub(crate) const DEPOSIT: [u8; 8] = [0xab, 0x5e, 0xeb, 0x67, 0x52, 0x40, 0xd4, 0x8c];
pub(crate) const WITHDRAW: [u8; 8] = [0x24, 0x48, 0x4a, 0x13, 0xd2, 0xd2, 0xc0, 0xc0];

/// `Bank`, 1,864 bytes.
const BANK_LEN: usize = 1_864;
const BANK_MINT: usize = 8;
const BANK_GROUP: usize = 41;
const BANK_LIQUIDITY_VAULT: usize = 112;
const BANK_ORACLE_SETUP: usize = 609;
const BANK_ORACLE_KEYS: usize = 610;
/// `MarginfiAccount`, 2,312 bytes: 16 balances of 104 bytes from 72, with `active` at +0 and
/// `bank_pk` at +1.
const ACCOUNT_LEN: usize = 2_312;
const BALANCES: usize = 72;
const BALANCE_LEN: usize = 104;
const BALANCE_COUNT: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bank {
    pub mint: Address,
    pub group: Address,
    pub liquidity_vault: Address,
    pub oracle_setup: u8,
    /// `oracle_keys[0]`: the only oracle account the banks this suite uses have.
    pub oracle: Address,
}

pub fn bank(svm: &LiteSVM, address: &Address) -> Bank {
    let data = owned_data(svm, address, BANK_LEN);
    Bank {
        mint: key_at(&data, BANK_MINT),
        group: key_at(&data, BANK_GROUP),
        liquidity_vault: key_at(&data, BANK_LIQUIDITY_VAULT),
        oracle_setup: data[BANK_ORACLE_SETUP],
        oracle: key_at(&data, BANK_ORACLE_KEYS),
    }
}

/// The banks `account` holds an active balance in, in the account's own order.
pub fn active_banks(svm: &LiteSVM, account: &Address) -> Vec<Address> {
    let data = owned_data(svm, account, ACCOUNT_LEN);
    (0..BALANCE_COUNT)
        .map(|index| BALANCES + index * BALANCE_LEN)
        .filter(|&at| data[at] != 0)
        .map(|at| key_at(&data, at + 1))
        .collect()
}

/// The PDA that owns `bank`'s liquidity vault: `[LIQUIDITY_VAULT_AUTHORITY_SEED, bank]`
/// (`type-crate/src/constants.rs:4`), as `LendingAccountWithdraw` derives it
/// (`programs/marginfi/src/instructions/marginfi_account/withdraw.rs:323-331`).
pub fn vault_authority(bank: &Address) -> Address {
    Address::find_program_address(&[b"liquidity_vault_auth", bank.as_ref()], &MARGINFI).0
}

/// `marginfi_account_initialize`: `account` is a new keypair, and signs. Accounts in the order of
/// `MarginfiAccountInitialize` (`programs/marginfi/src/instructions/marginfi_account/initialize.rs:43-62`).
pub fn initialize_account(
    group: &Address,
    account: &Address,
    authority: &Address,
    fee_payer: &Address,
) -> Instruction {
    Instruction {
        program_id: MARGINFI,
        accounts: vec![
            AccountMeta::new_readonly(*group, false),
            AccountMeta::new(*account, true),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*fee_payer, true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: INITIALIZE.to_vec(),
    }
}

/// `lending_account_deposit(amount, deposit_up_to_limit: None)`. It needs no remaining accounts
/// and no oracle. Accounts in the order of `LendingAccountDeposit`
/// (`programs/marginfi/src/instructions/marginfi_account/deposit.rs:153-197`).
pub fn deposit(
    svm: &LiteSVM,
    group: &Address,
    account: &Address,
    authority: &Address,
    bank: &Address,
    source: &Address,
    amount: u64,
) -> Instruction {
    let mut data = DEPOSIT.to_vec();
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(0); // Option::None
    Instruction {
        program_id: MARGINFI,
        accounts: vec![
            AccountMeta::new_readonly(*group, false),
            AccountMeta::new(*account, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*bank, false),
            AccountMeta::new(*source, false),
            AccountMeta::new(self::bank(svm, bank).liquidity_vault, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data,
    }
}

/// `lending_account_withdraw(0, withdraw_all: Some(true))`, followed by `remaining`. Accounts in
/// the order of `LendingAccountWithdraw`
/// (`programs/marginfi/src/instructions/marginfi_account/withdraw.rs:268-337`).
pub fn withdraw_all(
    svm: &LiteSVM,
    group: &Address,
    account: &Address,
    authority: &Address,
    bank: &Address,
    destination: &Address,
    remaining: Vec<AccountMeta>,
) -> Instruction {
    let mut data = WITHDRAW.to_vec();
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&[1, 1]); // Option::Some(true)
    let mut accounts = vec![
        AccountMeta::new_readonly(*group, false),
        AccountMeta::new(*account, false),
        AccountMeta::new_readonly(*authority, true),
        AccountMeta::new(*bank, false),
        AccountMeta::new(*destination, false),
        AccountMeta::new_readonly(vault_authority(bank), false),
        AccountMeta::new(self::bank(svm, bank).liquidity_vault, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
    ];
    accounts.extend(remaining);
    Instruction {
        program_id: MARGINFI,
        accounts,
        data,
    }
}

/// What marginfi's health check reads once `withdrawn` is emptied. For every other balance the
/// account holds, it takes the bank and then its oracle, by bank address from highest to lowest,
/// as `sort_balances` leaves them (`programs/marginfi/src/state/marginfi_account.rs:1725-1728`).
/// This is the `healthAccounts` group; it is empty when `withdrawn` was the only balance.
///
/// # Panics
///
/// If a bank is priced any other way than by one Pyth account. Staked, Kamino and other setups
/// take more accounts per bank.
pub fn health_accounts(svm: &LiteSVM, account: &Address, withdrawn: &Address) -> Vec<AccountMeta> {
    let mut banks: Vec<Address> = active_banks(svm, account)
        .into_iter()
        .filter(|bank| bank != withdrawn)
        .collect();
    banks.sort_by(|a, b| b.as_ref().cmp(a.as_ref()));
    banks
        .iter()
        .flat_map(|address| {
            let bank = bank(svm, address);
            assert_eq!(
                bank.oracle_setup, PYTH_PUSH_ORACLE,
                "bank {address} is not priced by one Pyth account"
            );
            [
                AccountMeta::new_readonly(*address, false),
                AccountMeta::new_readonly(bank.oracle, false),
            ]
        })
        .collect()
}

fn owned_data(svm: &LiteSVM, address: &Address, len: usize) -> Vec<u8> {
    let account = svm
        .get_account(address)
        .unwrap_or_else(|| panic!("{address} is not in the SVM"));
    assert!(
        account.owner == MARGINFI && account.data.len() == len,
        "{address} is not the marginfi account this reads"
    );
    account.data
}

fn key_at(data: &[u8], offset: usize) -> Address {
    Address::new_from_array(data[offset..offset + 32].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            lending::{self, MARGINFI_GROUP, SOL_BANK, USDC_BANK, USDC_MINT},
            wallet::WSOL_MINT,
        },
    };

    /// The offsets below were printed against marginfi's type crate; the real banks check them.
    #[test]
    fn the_offsets_read_the_snapshots_banks() {
        let svm = lending::svm();
        let usdc = Bank {
            mint: USDC_MINT,
            group: MARGINFI_GROUP,
            liquidity_vault: Address::from_str_const(
                "7jaiZR5Sk8hdYN9MxTpczTcwbWpb5WEoxSANuUwveuat",
            ),
            oracle_setup: PYTH_PUSH_ORACLE,
            oracle: Address::from_str_const("6HAuqASbHEh4w4REJEUUUCginTLfj1kwCh215ZLtMkrT"),
        };
        let sol = Bank {
            mint: WSOL_MINT,
            group: MARGINFI_GROUP,
            liquidity_vault: Address::from_str_const(
                "2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe",
            ),
            oracle_setup: PYTH_PUSH_ORACLE,
            oracle: Address::from_str_const("7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE"),
        };
        assert_eq!(bank(&svm, &USDC_BANK), usdc);
        assert_eq!(bank(&svm, &SOL_BANK), sol);
        assert_eq!(
            vault_authority(&USDC_BANK),
            Address::from_str_const("3uxNepDbmkDNq6JhRja5Z8QwbTrfmkKP8AKZV5chYDGG")
        );
        assert_eq!(
            vault_authority(&SOL_BANK),
            Address::from_str_const("DD3AeAssFvjqTvRTrRAtpfjkBF8FpVKnFuwnMLN9haXD")
        );
        for bank in [usdc, sol] {
            for address in [bank.liquidity_vault, bank.oracle] {
                assert!(svm.get_account(&address).is_some(), "{address}");
            }
        }
    }
}
