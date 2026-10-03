//! Test wallets: their SOL and their token balances. Write rule 1 lets tests set these directly;
//! nothing else is written here.

use {
    ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_account::Account,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

/// Lamports per SOL.
pub const SOL: u64 = 1_000_000_000;

/// The seed of the wallet Jupiter built every snapshotted route for. Its associated token
/// accounts are the routes' source and destination; they are not in the snapshot, so a test
/// creates them, through the route's setup instructions or with [`token_account`].
pub const WALLET_SEED: &[u8; 32] = b"ballista-protocol-tests-wallet-1";

/// Wrapped SOL, whose token accounts hold their balance as lamports too.
pub const WSOL_MINT: Address =
    Address::from_str_const("So11111111111111111111111111111111111111112");

/// An SPL Token account's size.
pub const TOKEN_ACCOUNT_LEN: usize = 165;

/// A keypair from a fixed seed. Fixed keys keep compute units deterministic: deriving an address
/// from a key costs more for each bump the search tries.
pub fn keypair(seed: &[u8; 32]) -> Keypair {
    Keypair::new_from_array(*seed)
}

/// The wallet the snapshotted routes were built for (see [`WALLET_SEED`]), unfunded.
pub fn wallet() -> Keypair {
    keypair(WALLET_SEED)
}

/// Sets a wallet's balance to exactly `lamports`, creating the account if needed.
///
/// This writes the account rather than airdropping: an airdrop is a transaction, and a second
/// identical one is rejected as `AlreadyProcessed`.
///
/// # Panics
///
/// If the address is off the curve, a program's rather than a wallet's, or holds anything but a
/// plain System account: only wallets may be written.
pub fn fund(svm: &mut LiteSVM, address: &Address, lamports: u64) {
    assert!(
        address.is_on_curve(),
        "{address} is off the curve, so no wallet signs for it; only wallets may be funded"
    );
    let mut account = svm.get_account(address).unwrap_or(Account {
        lamports: 0,
        data: vec![],
        owner: SYSTEM_PROGRAM_ID,
        executable: false,
        rent_epoch: u64::MAX,
    });
    assert!(
        account.owner == SYSTEM_PROGRAM_ID && account.data.is_empty(),
        "{address} is not a wallet (owner {}, {} bytes of data); only wallets may be funded",
        account.owner,
        account.data.len()
    );
    account.lamports = lamports;
    svm.set_account(*address, account)
        .unwrap_or_else(|error| panic!("funding {address} failed: {error:?}"));
}

/// A new keypair holding `lamports`.
///
/// It is random, so do not use it where the key seeds an address (a template's creator) and
/// compute units are compared; use [`keypair`] and [`fund`] there.
pub fn funded(svm: &mut LiteSVM, lamports: u64) -> Keypair {
    let keypair = Keypair::new();
    fund(svm, &keypair.pubkey(), lamports);
    keypair
}

/// The associated token account of `owner` for `mint`, under the SPL Token program.
pub fn associated_token_address(owner: &Address, mint: &Address) -> Address {
    Address::find_program_address(
        &[owner.as_ref(), TOKEN_PROGRAM_ID.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}

/// Creates or overwrites `owner`'s associated token account for `mint` so it holds `amount`, and
/// returns its address.
///
/// The account is written in the SPL Token layout and owned by the Token program, as the
/// Associated Token program would create it: initialized, no delegate, no close authority, and
/// rent-exempt. A wrapped SOL account is native: its lamports are the rent-exempt reserve plus
/// `amount`, as `SyncNative` would leave them.
///
/// # Panics
///
/// If `owner` is off the curve, so that the account would be a program's rather than a wallet's,
/// or `mint` is not an SPL Token mint in the SVM.
pub fn token_account(svm: &mut LiteSVM, owner: &Address, mint: &Address, amount: u64) -> Address {
    assert!(
        owner.is_on_curve(),
        "{owner} is off the curve, so its token accounts belong to a program; only wallets' may be written"
    );
    let mint_account = svm
        .get_account(mint)
        .unwrap_or_else(|| panic!("mint {mint} is not in the SVM"));
    assert_eq!(
        mint_account.owner, TOKEN_PROGRAM_ID,
        "{mint} is not an SPL Token mint"
    );
    let address = associated_token_address(owner, mint);
    let reserve = svm.minimum_balance_for_rent_exemption(TOKEN_ACCOUNT_LEN);
    let native = *mint == WSOL_MINT;

    let mut data = vec![0; TOKEN_ACCOUNT_LEN];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // 72..108: delegate, `COption::None`.
    data[108] = 1; // AccountState::Initialized
    if native {
        // 109..121: is_native, `COption::Some(rent-exempt reserve)`.
        data[109..113].copy_from_slice(&1u32.to_le_bytes());
        data[113..121].copy_from_slice(&reserve.to_le_bytes());
    }
    // 121..129: delegated amount; 129..165: close authority, `COption::None`.

    let lamports = if native {
        reserve
            .checked_add(amount)
            .expect("wrapped SOL amount overflows the lamport count")
    } else {
        reserve
    };
    svm.set_account(
        address,
        Account {
            lamports,
            data,
            owner: TOKEN_PROGRAM_ID,
            executable: false,
            rent_epoch: u64::MAX,
        },
    )
    .unwrap_or_else(|error| panic!("writing token account {address} failed: {error:?}"));
    address
}

/// Creates `account` as a second SPL Token account of `owner`'s for `mint`, which [`token_account`]
/// cannot write: it writes only the associated one. These are instructions, not a write: the
/// System program's `CreateAccount` (paid by `payer`, signed by `account`), then the Token
/// program's `InitializeAccount3`. The account starts empty.
pub fn create_token_account(
    svm: &LiteSVM,
    payer: &Address,
    account: &Address,
    mint: &Address,
    owner: &Address,
) -> Vec<Instruction> {
    let lamports = svm.minimum_balance_for_rent_exemption(TOKEN_ACCOUNT_LEN);
    let mut create = vec![0, 0, 0, 0]; // SystemInstruction::CreateAccount
    create.extend_from_slice(&lamports.to_le_bytes());
    create.extend_from_slice(&(TOKEN_ACCOUNT_LEN as u64).to_le_bytes());
    create.extend_from_slice(TOKEN_PROGRAM_ID.as_ref());
    let mut initialize = vec![18]; // TokenInstruction::InitializeAccount3
    initialize.extend_from_slice(owner.as_ref());
    vec![
        Instruction {
            program_id: SYSTEM_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(*payer, true),
                AccountMeta::new(*account, true),
            ],
            data: create,
        },
        Instruction {
            program_id: TOKEN_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(*account, false),
                AccountMeta::new_readonly(*mint, false),
            ],
            data: initialize,
        },
    ]
}

/// SPL Token's `Approve`: `owner` lets `delegate` move up to `amount` out of `account`. An
/// instruction for the owner to sign, not a write: a delegate is not a balance.
pub fn approve(account: &Address, delegate: &Address, owner: &Address, amount: u64) -> Instruction {
    let mut data = vec![4]; // TokenInstruction::Approve
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*account, false),
            AccountMeta::new_readonly(*delegate, false),
            AccountMeta::new_readonly(*owner, true),
        ],
        data,
    }
}

/// A token account's balance.
///
/// # Panics
///
/// If the account does not exist or is not an SPL Token account.
pub fn token_balance(svm: &LiteSVM, account: &Address) -> u64 {
    let account_data = svm
        .get_account(account)
        .unwrap_or_else(|| panic!("token account {account} does not exist"));
    assert!(
        account_data.owner == TOKEN_PROGRAM_ID && account_data.data.len() == TOKEN_ACCOUNT_LEN,
        "{account} is not an SPL Token account"
    );
    u64::from_le_bytes(account_data.data[64..72].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{snapshot::Snapshot, tx},
    };

    const USDC_MINT: Address =
        Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

    #[test]
    fn the_wallet_and_its_token_accounts_are_the_ones_the_routes_use() {
        let owner = wallet().pubkey();
        assert_eq!(
            owner.to_string(),
            "ApS3VQAaUaEMsUKzoyt26G2wUM5Xpta57WbQEN17tyq1"
        );
        assert_eq!(
            associated_token_address(&owner, &WSOL_MINT).to_string(),
            "FVzTfvuUeymGxLRNzTQb5f9nUskrnavdWdtQ6nL9bY4o"
        );
        assert_eq!(
            associated_token_address(&owner, &USDC_MINT).to_string(),
            "7tdUJ8RiSeco4nTH6pfLTVtJJKuKd82msFyahnDggCnX"
        );
    }

    #[test]
    fn funding_sets_the_balance_exactly_and_repeats() {
        let mut svm = LiteSVM::new();
        let key = keypair(b"ballista-protocol-tests-funded-1").pubkey();
        fund(&mut svm, &key, 3 * SOL);
        fund(&mut svm, &key, SOL);
        assert_eq!(svm.get_balance(&key), Some(SOL));
        let random = funded(&mut svm, 2 * SOL);
        assert_eq!(svm.get_balance(&random.pubkey()), Some(2 * SOL));
    }

    #[test]
    #[should_panic(expected = "is not a wallet (owner BPFLoaderUpgradeab1e")]
    fn funding_refuses_a_program_owned_account() {
        let mut svm = LiteSVM::new();
        fund(&mut svm, &TOKEN_PROGRAM_ID, SOL);
    }

    /// A System account with data is a nonce account or the like, not a wallet.
    #[test]
    #[should_panic(expected = "is not a wallet (owner 11111111111111111111111111111111, 80 bytes")]
    fn funding_refuses_a_system_account_with_data() {
        let mut svm = LiteSVM::new();
        let key = keypair(b"ballista-protocol-tests-nonce-01").pubkey();
        let nonce = Account {
            lamports: SOL,
            data: vec![0; 80],
            owner: SYSTEM_PROGRAM_ID,
            executable: false,
            rent_epoch: u64::MAX,
        };
        svm.set_account(key, nonce).unwrap();
        fund(&mut svm, &key, 2 * SOL);
    }

    fn program_address() -> Address {
        Address::find_program_address(&[b"vault"], &TOKEN_PROGRAM_ID).0
    }

    #[test]
    #[should_panic(expected = "is off the curve, so no wallet signs for it")]
    fn funding_refuses_a_program_address() {
        let mut svm = LiteSVM::new();
        fund(&mut svm, &program_address(), SOL);
    }

    #[test]
    #[should_panic(expected = "is off the curve, so its token accounts belong to a program")]
    fn token_accounts_refuse_a_program_owner() {
        let mut svm = LiteSVM::new();
        token_account(&mut svm, &program_address(), &WSOL_MINT, 1);
    }

    fn token_instruction(accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
        Instruction {
            program_id: TOKEN_PROGRAM_ID,
            accounts,
            data,
        }
    }

    /// What `token_account` writes is, byte for byte, what the Associated Token program creates,
    /// and for wrapped SOL what a transfer and `SyncNative` then leave: `is_native` records the
    /// rent-exempt reserve, and the lamports are the reserve plus the amount.
    #[test]
    fn written_token_accounts_are_the_ones_the_programs_make() {
        let mut svm = Snapshot::load(crate::snapshot::SNAPSHOT_DIR).into_svm();
        let owner = wallet();
        fund(&mut svm, &owner.pubkey(), 10 * SOL);
        for (mint, amount) in [(USDC_MINT, 0), (WSOL_MINT, 2 * SOL)] {
            let address = associated_token_address(&owner.pubkey(), &mint);
            // CreateIdempotent.
            let mut instructions = vec![Instruction {
                program_id: ASSOCIATED_TOKEN_PROGRAM_ID,
                accounts: vec![
                    AccountMeta::new(owner.pubkey(), true),
                    AccountMeta::new(address, false),
                    AccountMeta::new_readonly(owner.pubkey(), false),
                    AccountMeta::new_readonly(mint, false),
                    AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                    AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                ],
                data: vec![1],
            }];
            if mint == WSOL_MINT {
                // A System transfer into the account, then SyncNative.
                let mut transfer = vec![2, 0, 0, 0];
                transfer.extend_from_slice(&amount.to_le_bytes());
                instructions.push(Instruction {
                    program_id: SYSTEM_PROGRAM_ID,
                    accounts: vec![
                        AccountMeta::new(owner.pubkey(), true),
                        AccountMeta::new(address, false),
                    ],
                    data: transfer,
                });
                instructions.push(token_instruction(
                    vec![AccountMeta::new(address, false)],
                    vec![17],
                ));
            }
            tx::send(&mut svm, &owner, &[], &instructions, &[]).unwrap();
            let made = svm.get_account(&address).unwrap();

            token_account(&mut svm, &owner.pubkey(), &mint, amount);
            assert_eq!(svm.get_account(&address), Some(made), "{mint}");
        }
    }

    /// Mainnet's Token program accepts the accounts as written: it moves a balance between two
    /// of them, syncing wrapped SOL leaves its balance alone, and unwrapping pays out exactly the
    /// reserve plus the amount.
    #[test]
    fn written_token_accounts_work_with_the_mainnet_token_program() {
        let mut svm = Snapshot::load(crate::snapshot::SNAPSHOT_DIR).into_svm();
        let owner = wallet();
        let other = keypair(b"ballista-protocol-tests-owner-02").pubkey();
        fund(&mut svm, &owner.pubkey(), SOL);

        let source = token_account(&mut svm, &owner.pubkey(), &USDC_MINT, 150_000_000);
        let destination = token_account(&mut svm, &other, &USDC_MINT, 5);
        // Transfer: tag 3, then the amount.
        let mut transfer = vec![3];
        transfer.extend_from_slice(&40_000_000u64.to_le_bytes());
        let transfer = token_instruction(
            vec![
                AccountMeta::new(source, false),
                AccountMeta::new(destination, false),
                AccountMeta::new_readonly(owner.pubkey(), true),
            ],
            transfer,
        );
        tx::send(&mut svm, &owner, &[], &[transfer], &[]).unwrap();
        assert_eq!(token_balance(&svm, &source), 110_000_000);
        assert_eq!(token_balance(&svm, &destination), 40_000_005);

        // Overwriting sets the balance again.
        token_account(&mut svm, &owner.pubkey(), &USDC_MINT, 7);
        assert_eq!(token_balance(&svm, &source), 7);

        let wrapped = token_account(&mut svm, &owner.pubkey(), &WSOL_MINT, 2 * SOL);
        assert_eq!(token_balance(&svm, &wrapped), 2 * SOL);
        let reserve = svm.minimum_balance_for_rent_exemption(TOKEN_ACCOUNT_LEN);
        assert_eq!(svm.get_balance(&wrapped), Some(reserve + 2 * SOL));
        // SyncNative: tag 17. It sets the balance to the lamports above the reserve, which it
        // recomputes from the Rent sysvar, so it cannot tell whether the recorded one was right.
        let sync = token_instruction(vec![AccountMeta::new(wrapped, false)], vec![17]);
        tx::send(&mut svm, &owner, &[], &[sync], &[]).unwrap();
        assert_eq!(token_balance(&svm, &wrapped), 2 * SOL);
        let before = svm.get_balance(&owner.pubkey()).unwrap();
        // CloseAccount: tag 9. The lamports go to the owner.
        let close = token_instruction(
            vec![
                AccountMeta::new(wrapped, false),
                AccountMeta::new(owner.pubkey(), false),
                AccountMeta::new_readonly(owner.pubkey(), true),
            ],
            vec![9],
        );
        let outcome = tx::send(&mut svm, &owner, &[], &[close], &[]).unwrap();
        assert_eq!(svm.get_account(&wrapped), None);
        assert_eq!(
            svm.get_balance(&owner.pubkey()),
            Some(before + reserve + 2 * SOL - outcome.fee)
        );
    }
}
