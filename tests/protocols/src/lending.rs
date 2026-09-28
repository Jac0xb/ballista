//! The lending snapshot, and the setup the Kamino and marginfi scenarios share.
//!
//! `snapshot-lending/` (`scripts/snapshot/manifests/lending.json`) holds, at one slot:
//! - Kamino Lend's main market, with its SOL and USDC reserves, their vaults, collateral mints and
//!   farms;
//! - Scope's price account and Kamino Farms;
//! - marginfi's main group, with its USDC and SOL banks and their oracles;
//! - the Jupiter route `solToUsdc`.

use {
    crate::{
        kamino, marginfi, oracle,
        snapshot::{self, Leg, Snapshot},
        template::{self, Example},
        tx::{self, Outcome},
        wallet::{self, SOL},
    },
    litesvm::LiteSVM,
    solana_address::Address,
    solana_clock::Clock,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
    std::sync::OnceLock,
};

pub const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot-lending");

pub const MARKET: Address = Address::from_str_const("7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF");
pub const SOL_RESERVE: Address =
    Address::from_str_const("d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q");
pub const USDC_RESERVE: Address =
    Address::from_str_const("D6q6wuQSrifJKZYpR1M8R4YawnLDtDsMmWM1NbBmgJ59");
pub const SCOPE_PRICES: Address =
    Address::from_str_const("3t4JZcueEzTbVP6kLxXrL3VpWx45jDer4eqysweBchNH");
/// The Scope entries the reserves read: each reserve's `price_chain` and `twap_chain`.
pub const SOL_SPOT: usize = 3;
pub const SOL_TWAP: usize = 455;
pub const USDC_SPOT: usize = 13;
pub const USDC_TWAP: usize = 456;
pub const USDC_MINT: Address =
    Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
pub const MARGINFI_GROUP: Address =
    Address::from_str_const("4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8");
pub const USDC_BANK: Address =
    Address::from_str_const("2s37akK2eyBbp8DZgCm7RtsaEz8eJP3Nxd4urLHQv7yB");
pub const SOL_BANK: Address =
    Address::from_str_const("CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh");
/// Both swap templates' route: 1 SOL for USDC, built for [`wallet::wallet`].
pub const SOL_TO_USDC: &str = "solToUsdc";
/// Uploads every template, as template 1.
pub const CREATOR_SEED: &[u8; 32] = b"ballista-protocol-tests-creator1";
/// `route`'s leading accounts, which a template passes itself (`JUPITER_ROUTE_FIXED_ACCOUNTS`).
const ROUTE_FIXED_ACCOUNTS: usize = 4;

/// The snapshot, loaded and checked once per test binary.
pub fn snapshot() -> &'static Snapshot {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| Snapshot::load(SNAPSHOT_DIR))
}

/// A LiteSVM at the snapshot. The Scope entries the reserves read are stamped with the snapshot's
/// clock (write rule 2; prices unchanged). Each is then fresh, however long before the snapshot
/// Scope last updated it: Kamino's SOL window is 120 seconds.
pub fn svm() -> LiteSVM {
    let mut svm = snapshot().svm();
    let clock = svm.get_sysvar::<Clock>();
    let now = u64::try_from(clock.unix_timestamp).expect("the clock is after 1970");
    for index in [SOL_SPOT, SOL_TWAP, USDC_SPOT, USDC_TWAP] {
        let price = oracle::scope_price(&svm, &SCOPE_PRICES, index);
        oracle::set_scope_price(&mut svm, &SCOPE_PRICES, index, price.value, clock.slot, now);
    }
    svm
}

/// One slot on, and no time. Call it before every template transaction.
/// - LiteSVM never advances the slot by itself, and klend counts a reserve or obligation as fresh
///   for the rest of the slot it was refreshed in. Without this, a refresh sent during setup would
///   hide a template transaction that forgot its own.
/// - The time stays put, so no interest accrues between setup and the run, and no price ages.
pub fn next_slot(svm: &mut LiteSVM) {
    snapshot::warp(svm, 1, 0);
}

/// `SetComputeUnitLimit(1,400,000)`. A run that swaps and then calls Kamino needs more than the
/// default.
pub fn compute_limit() -> Instruction {
    ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)
}

/// Sends a setup transaction behind [`compute_limit`], and panics if it fails.
pub fn setup(
    svm: &mut LiteSVM,
    payer: &Keypair,
    signers: &[&Keypair],
    instructions: Vec<Instruction>,
) -> Outcome {
    let mut all = vec![compute_limit()];
    all.extend(instructions);
    tx::send(svm, payer, signers, &all, &[])
        .unwrap_or_else(|failure| panic!("setup failed: {failure:?}"))
}

/// Uploads `example` as the tests' creator's template 1.
pub fn upload(svm: &mut LiteSVM, example: &Example) -> Address {
    let creator = wallet::keypair(CREATOR_SEED);
    wallet::fund(svm, &creator.pubkey(), 10 * SOL);
    template::upload(svm, &creator, 1, &example.payload)
}

/// `route`'s own accounts, after the four a template passes itself: the `routeAccounts` group.
pub fn route_group(leg: &Leg) -> Vec<AccountMeta> {
    leg.instructions.swap.accounts[ROUTE_FIXED_ACCOUNTS..].to_vec()
}

/// What `leg` pays into its destination when it runs alone, from `svm`'s state, in the next slot:
/// the slot a template transaction runs in. `svm` is left as it was.
pub fn swap_output(svm: &LiteSVM, leg: &Leg) -> u64 {
    let mut alone = svm.clone();
    next_slot(&mut alone);
    let before = wallet::token_balance(&alone, &leg.destination_token_account);
    tx::send(
        &mut alone,
        &wallet::wallet(),
        &[],
        &[compute_limit(), leg.instructions.swap.clone()],
        &leg.lookup_tables,
    )
    .unwrap_or_else(|failure| panic!("the route alone failed: {failure:?}"));
    wallet::token_balance(&alone, &leg.destination_token_account) - before
}

/// Opens `owner`'s user metadata and vanilla obligation in the main market, plus the obligation's
/// user state in each of `farmed`'s collateral farms. Returns the obligation.
pub fn open_obligation(svm: &mut LiteSVM, owner: &Keypair, farmed: &[Address]) -> Address {
    let o = owner.pubkey();
    let obligation = kamino::vanilla_obligation(&o, &MARKET);
    let mut instructions = vec![
        kamino::init_user_metadata(&o),
        kamino::init_obligation(&o, &MARKET),
    ];
    instructions.extend(
        farmed
            .iter()
            .map(|reserve| kamino::init_obligation_farm(svm, &o, &o, &obligation, reserve)),
    );
    setup(svm, owner, &[], instructions);
    obligation
}

/// Deposits `amount` of `reserve`'s liquidity from `source`, behind the refreshes it needs.
pub fn deposit(
    svm: &mut LiteSVM,
    owner: &Keypair,
    obligation: &Address,
    reserve: &Address,
    source: &Address,
    amount: u64,
) {
    let mut instructions = kamino::refreshes(svm, obligation, &[*reserve]);
    instructions.push(kamino::deposit(
        svm,
        &owner.pubkey(),
        obligation,
        reserve,
        source,
        amount,
    ));
    setup(svm, owner, &[], instructions);
}

/// Borrows `amount` of `reserve`'s liquidity into `destination`, behind the refreshes it needs:
/// every held reserve and the borrowed one, then the obligation.
pub fn borrow(
    svm: &mut LiteSVM,
    owner: &Keypair,
    obligation: &Address,
    reserve: &Address,
    destination: &Address,
    amount: u64,
) {
    let mut instructions = kamino::refreshes(svm, obligation, &[*reserve]);
    instructions.push(kamino::borrow(
        svm,
        &owner.pubkey(),
        obligation,
        reserve,
        destination,
        amount,
    ));
    setup(svm, owner, &[], instructions);
}

/// Opens the marginfi account `account` for `authority` in the main group, and deposits each
/// `(bank, source token account, amount)`. Returns the account's address.
pub fn marginfi_account(
    svm: &mut LiteSVM,
    authority: &Keypair,
    account: &Keypair,
    deposits: &[(Address, Address, u64)],
) -> Address {
    let a = authority.pubkey();
    let mut instructions = vec![marginfi::initialize_account(
        &MARGINFI_GROUP,
        &account.pubkey(),
        &a,
        &a,
    )];
    for (bank, source, amount) in deposits {
        instructions.push(marginfi::deposit(
            svm,
            &MARGINFI_GROUP,
            &account.pubkey(),
            &a,
            bank,
            source,
            *amount,
        ));
    }
    setup(svm, authority, &[account], instructions);
    account.pubkey()
}

/// An obligation klend will liquidate:
/// - Its borrower (seed `ballista-protocol-tests-borrower`) deposited 1 SOL and borrowed 70% of its
///   value in USDC. The SOL reserve's loan-to-value is 74%.
/// - Then SOL's Scope spot and TWAP were both lowered 12% and stamped now (rule 2). That puts the
///   debt at about 80% of the collateral, past the 75% liquidation threshold. Moving the TWAP too
///   keeps every price check passing.
pub struct Unhealthy {
    pub obligation: Address,
    /// USDC base units borrowed.
    pub debt: u64,
    /// The prices it was left at: Scope values with exponent 8.
    pub sol_price: u64,
    pub usdc_price: u64,
}

pub fn unhealthy_obligation(svm: &mut LiteSVM) -> Unhealthy {
    let borrower = wallet::keypair(b"ballista-protocol-tests-borrower");
    let b = borrower.pubkey();
    wallet::fund(svm, &b, 10 * SOL);
    let collateral = wallet::token_account(svm, &b, &wallet::WSOL_MINT, SOL);
    let proceeds = wallet::token_account(svm, &b, &USDC_MINT, 0);
    let obligation = open_obligation(svm, &borrower, &[SOL_RESERVE]);
    deposit(svm, &borrower, &obligation, &SOL_RESERVE, &collateral, SOL);

    let sol = oracle::scope_price(svm, &SCOPE_PRICES, SOL_SPOT);
    let usdc = oracle::scope_price(svm, &SCOPE_PRICES, USDC_SPOT);
    assert_eq!((sol.exp, usdc.exp), (8, 8), "Scope prices with exponent 8");
    // 70% of one SOL in USDC base units: value / 10^8 dollars, times 10^6, times 0.7.
    let debt = sol.value * 7 / 1_000;
    borrow(svm, &borrower, &obligation, &USDC_RESERVE, &proceeds, debt);

    let fallen = sol.value * 88 / 100;
    let clock = svm.get_sysvar::<Clock>();
    let now = u64::try_from(clock.unix_timestamp).expect("the clock is after 1970");
    for index in [SOL_SPOT, SOL_TWAP] {
        oracle::set_scope_price(svm, &SCOPE_PRICES, index, fallen, clock.slot, now);
    }
    Unhealthy {
        obligation,
        debt,
        sol_price: fallen,
        usdc_price: usdc.value,
    }
}

/// A liquidator (seed `ballista-protocol-tests-liquid-1`) holding `usdc` in its USDC account, with
/// empty accounts for the SOL reserve's cTokens and for wrapped SOL (rule 1).
pub fn liquidator(svm: &mut LiteSVM, usdc: u64) -> (Keypair, kamino::Liquidator) {
    let liquidator = wallet::keypair(b"ballista-protocol-tests-liquid-1");
    let l = liquidator.pubkey();
    wallet::fund(svm, &l, 10 * SOL);
    let collateral_mint = kamino::reserve_accounts(svm, &SOL_RESERVE).collateral_mint;
    let accounts = kamino::Liquidator {
        signer: l,
        source_liquidity: wallet::token_account(svm, &l, &USDC_MINT, usdc),
        destination_collateral: wallet::token_account(svm, &l, &collateral_mint, 0),
        destination_liquidity: wallet::token_account(svm, &l, &wallet::WSOL_MINT, 0),
    };
    (liquidator, accounts)
}

/// The attacker: a wallet whose token accounts a hostile run builder names in place of the
/// signer's.
pub const ATTACKER_SEED: &[u8; 32] = b"ballista-protocol-tests-attacker";

/// The attacker's empty token account for `mint` (rule 1). With `delegate`, the attacker has also
/// approved `delegate` to move any amount out of it, in a real `Approve`. klend and the templates
/// move tokens out of the accounts they are given with the signer's authority, and a delegate's
/// signature is that authority too, so this is the account an attacker would name.
pub fn attacker_account(svm: &mut LiteSVM, mint: &Address, delegate: Option<&Address>) -> Address {
    let attacker = wallet::keypair(ATTACKER_SEED);
    let a = attacker.pubkey();
    wallet::fund(svm, &a, SOL);
    let account = wallet::token_account(svm, &a, mint, 0);
    if let Some(delegate) = delegate {
        let approve = wallet::approve(&account, delegate, &a, u64::MAX);
        setup(svm, &attacker, &[], vec![approve]);
    }
    account
}

/// `repaid` USDC base units, in lamports, at the given Scope prices (exponent 8). SOL has 9
/// decimals and USDC 6, hence the 1,000.
pub fn break_even(repaid: u64, usdc_price: u64, sol_price: u64) -> u64 {
    u64::try_from(u128::from(repaid) * u128::from(usdc_price) * 1_000 / u128::from(sol_price))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every address a reserve names is in the snapshot under the manifest's name for it, so the
    /// Rust side cannot fall out of step with `lending.json`.
    #[test]
    fn the_snapshot_holds_everything_the_reserves_name() {
        let named = |name: &str| snapshot().named(name);
        for (constant, name) in [
            (MARKET, "kaminoMainMarket"),
            (SOL_RESERVE, "kaminoSolReserve"),
            (USDC_RESERVE, "kaminoUsdcReserve"),
            (SCOPE_PRICES, "scopePrices"),
            (USDC_MINT, "usdcMint"),
            (wallet::WSOL_MINT, "wsolMint"),
            (MARGINFI_GROUP, "marginfiMainGroup"),
            (USDC_BANK, "marginfiUsdcBank"),
            (SOL_BANK, "marginfiSolBank"),
            (kamino::KLEND, "kamino"),
            (kamino::FARMS, "kaminoFarms"),
            (marginfi::MARGINFI, "marginfi"),
        ] {
            assert_eq!(constant, named(name), "{name}");
        }

        let svm = svm();
        for (reserve, asset) in [(SOL_RESERVE, "Sol"), (USDC_RESERVE, "Usdc")] {
            let accounts = kamino::reserve_accounts(&svm, &reserve);
            assert_eq!(accounts.lending_market, MARKET);
            assert_eq!(accounts.scope_prices, Some(SCOPE_PRICES));
            assert_eq!(accounts.farm_debt, None);
            assert_eq!(accounts.token_program, ballista_sdk::TOKEN_PROGRAM_ID);
            let expected = [
                (accounts.supply_vault, "SupplyVault"),
                (accounts.fee_vault, "FeeVault"),
                (accounts.collateral_mint, "CollateralMint"),
                (accounts.collateral_supply, "CollateralSupply"),
            ];
            for (address, role) in expected {
                assert_eq!(
                    address,
                    named(&format!("kamino{asset}{role}")),
                    "{asset} {role}"
                );
            }
            assert_eq!(
                accounts.farm_collateral,
                Some(named(&format!("kamino{asset}CollateralFarm")))
            );
            let every = [
                accounts.lending_market,
                accounts.liquidity_mint,
                accounts.supply_vault,
                accounts.fee_vault,
                accounts.collateral_mint,
                accounts.collateral_supply,
                accounts.token_program,
                accounts.farm_collateral.unwrap(),
                SCOPE_PRICES,
            ];
            for address in every {
                assert!(svm.get_account(&address).is_some(), "{asset}: {address}");
            }
        }
    }

    #[test]
    fn svm_stamps_the_scope_prices_with_the_snapshot_clock() {
        let svm = svm();
        let clock = svm.get_sysvar::<Clock>();
        assert_eq!(clock, snapshot().clock);
        for index in [SOL_SPOT, SOL_TWAP, USDC_SPOT, USDC_TWAP] {
            let price = oracle::scope_price(&svm, &SCOPE_PRICES, index);
            assert_eq!(price.exp, 8, "entry {index}: {price:?}");
            assert!(price.value > 0, "entry {index}: {price:?}");
            assert_eq!(
                (price.slot, i64::try_from(price.unix_timestamp).unwrap()),
                (clock.slot, clock.unix_timestamp),
                "entry {index}"
            );
            let snapshotted = {
                let mut bare = LiteSVM::new();
                let account = snapshot().account(&SCOPE_PRICES).unwrap().clone();
                bare.set_account(SCOPE_PRICES, account).unwrap();
                oracle::scope_price(&bare, &SCOPE_PRICES, index)
            };
            assert_eq!(
                price.value, snapshotted.value,
                "entry {index}: the price is unchanged"
            );
        }
    }

    /// The planning probe's setup: a deposit of SOL, then a USDC borrow against it.
    #[test]
    fn a_kamino_user_deposits_sol_and_borrows_usdc() {
        let mut svm = svm();
        let owner = wallet::wallet();
        let o = owner.pubkey();
        wallet::fund(&mut svm, &o, 100 * SOL);
        let wsol = wallet::token_account(&mut svm, &o, &wallet::WSOL_MINT, 10 * SOL);
        let usdc = wallet::token_account(&mut svm, &o, &USDC_MINT, 0);
        let obligation = open_obligation(&mut svm, &owner, &[SOL_RESERVE]);
        assert_eq!(obligation, kamino::vanilla_obligation(&o, &MARKET));

        deposit(&mut svm, &owner, &obligation, &SOL_RESERVE, &wsol, 10 * SOL);
        assert!(kamino::deposited(&svm, &obligation, &SOL_RESERVE) > 0);

        borrow(
            &mut svm,
            &owner,
            &obligation,
            &USDC_RESERVE,
            &usdc,
            300_000_000,
        );
        assert_eq!(wallet::token_balance(&svm, &usdc), 300_000_000);
        assert_eq!(
            kamino::borrowed_sf(&svm, &obligation, &USDC_RESERVE) >> 60,
            300_000_000
        );
    }

    /// What the swap templates' exact assertions rest on.
    #[test]
    fn the_route_pays_the_same_every_time() {
        let leg = snapshot().route(SOL_TO_USDC).legs[0].clone();
        let mut svm = svm();
        let owner = wallet::wallet().pubkey();
        wallet::fund(&mut svm, &owner, 10 * SOL);
        wallet::token_account(&mut svm, &owner, &wallet::WSOL_MINT, leg.in_amount);
        wallet::token_account(&mut svm, &owner, &USDC_MINT, 0);

        let first = swap_output(&svm, &leg);
        assert!(
            first >= leg.other_amount_threshold,
            "{first} < {}",
            leg.other_amount_threshold
        );
        assert_eq!(swap_output(&svm, &leg), first);
        // The SVM is left as it was.
        assert_eq!(
            wallet::token_balance(&svm, &leg.destination_token_account),
            0
        );
    }

    #[test]
    fn a_marginfi_account_takes_deposits_in_two_banks() {
        let mut svm = svm();
        let authority = wallet::keypair(b"ballista-protocol-tests-mfi-auth");
        let account = wallet::keypair(b"ballista-protocol-tests-mfi-acct");
        let a = authority.pubkey();
        wallet::fund(&mut svm, &a, 10 * SOL);
        let usdc = wallet::token_account(&mut svm, &a, &USDC_MINT, 100_000_000);
        let wsol = wallet::token_account(&mut svm, &a, &wallet::WSOL_MINT, SOL);

        let address = marginfi_account(
            &mut svm,
            &authority,
            &account,
            &[(USDC_BANK, usdc, 100_000_000), (SOL_BANK, wsol, SOL)],
        );
        assert_eq!(address, account.pubkey());
        assert_eq!(wallet::token_balance(&svm, &usdc), 0);
        assert_eq!(wallet::token_balance(&svm, &wsol), 0);
        let mut active = marginfi::active_banks(&svm, &address);
        active.sort();
        let mut both = vec![USDC_BANK, SOL_BANK];
        both.sort();
        assert_eq!(active, both);
        assert_eq!(
            marginfi::health_accounts(&svm, &address, &USDC_BANK),
            [
                AccountMeta::new_readonly(SOL_BANK, false),
                AccountMeta::new_readonly(marginfi::bank(&svm, &SOL_BANK).oracle, false),
            ]
        );
        assert_eq!(
            marginfi::health_accounts(&svm, &address, &SOL_BANK).len(),
            2
        );
    }
}
