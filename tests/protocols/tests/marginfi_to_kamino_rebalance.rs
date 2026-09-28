//! `marginfiToKaminoRebalance` against marginfi and Kamino: empty a 100 USDC marginfi balance and
//! deposit exactly what it released into Kamino's USDC reserve, in one transaction.

use {
    ballista_protocol_tests::{
        kamino,
        lending::{self, MARGINFI_GROUP, MARKET, USDC_BANK, USDC_MINT, USDC_RESERVE},
        marginfi,
        template::{self, Run},
        tx::{self, Failure, Outcome},
        wallet::{self, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const NAME: &str = "marginfiToKaminoRebalance";
/// USDC base units deposited into marginfi.
const DEPOSIT: u64 = 100_000_000;

struct Scene {
    owner: Keypair,
    /// The owner's USDC account, which the assets pass through.
    wallet_ata: Address,
    marginfi_account: Address,
    obligation: Address,
    template: Address,
}

/// An owner (seed `ballista-protocol-tests-rebalanc`) with 100 USDC in a marginfi account (seed
/// `ballista-protocol-tests-rebal-mf`), deposited from its USDC account (rule 1), which is left
/// empty; a Kamino obligation with its USDC farm user state; the template, uploaded.
fn scene() -> (LiteSVM, Scene) {
    let mut svm = lending::svm();
    let owner = wallet::keypair(b"ballista-protocol-tests-rebalanc");
    let o = owner.pubkey();
    wallet::fund(&mut svm, &o, 10 * SOL);
    let wallet_ata = wallet::token_account(&mut svm, &o, &USDC_MINT, DEPOSIT);
    let marginfi_account = lending::marginfi_account(
        &mut svm,
        &owner,
        &wallet::keypair(b"ballista-protocol-tests-rebal-mf"),
        &[(USDC_BANK, wallet_ata, DEPOSIT)],
    );
    assert_eq!(wallet::token_balance(&svm, &wallet_ata), 0);
    let obligation = lending::open_obligation(&mut svm, &owner, &[USDC_RESERVE]);
    let template = lending::upload(&mut svm, &template::examples()[NAME]);
    (
        svm,
        Scene {
            owner,
            wallet_ata,
            marginfi_account,
            obligation,
            template,
        },
    )
}

fn run(svm: &LiteSVM, scene: &Scene, minimum_moved: u64) -> Instruction {
    let examples = template::examples();
    let usdc = kamino::reserve_accounts(svm, &USDC_RESERVE);
    Run::new(scene.template, &examples[NAME])
        .account("marginfi", marginfi::MARGINFI, false, false)
        .account("kamino", kamino::KLEND, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account(
            "instructionsSysvar",
            kamino::INSTRUCTIONS_SYSVAR,
            false,
            false,
        )
        .account("owner", scene.owner.pubkey(), true, true)
        .account("walletAta", scene.wallet_ata, true, false)
        .account("marginfiGroup", MARGINFI_GROUP, false, false)
        .account("marginfiAccount", scene.marginfi_account, true, false)
        .account("marginfiBank", USDC_BANK, true, false)
        .account(
            "marginfiVault",
            marginfi::bank(svm, &USDC_BANK).liquidity_vault,
            true,
            false,
        )
        .account(
            "marginfiVaultAuthority",
            marginfi::vault_authority(&USDC_BANK),
            false,
            false,
        )
        .account("obligation", scene.obligation, true, false)
        .account("lendingMarket", MARKET, false, false)
        .account(
            "lendingMarketAuthority",
            kamino::lending_market_authority(&MARKET),
            false,
            false,
        )
        .account("reserve", USDC_RESERVE, true, false)
        .account("reserveLiquidityMint", usdc.liquidity_mint, false, false)
        .account("reserveLiquiditySupply", usdc.supply_vault, true, false)
        .account("reserveCollateralMint", usdc.collateral_mint, true, false)
        .account(
            "reserveDestinationDepositCollateral",
            usdc.collateral_supply,
            true,
            false,
        )
        .input_u64("minimumMoved", minimum_moved)
        .group(
            "healthAccounts",
            marginfi::health_accounts(svm, &scene.marginfi_account, &USDC_BANK),
        )
        .group(
            "farmAccounts",
            kamino::deposit_farm_accounts(svm, &scene.obligation, &USDC_RESERVE),
        )
        .build()
}

/// The run in a new slot, behind the refreshes klend needs for the deposit.
fn send_run(svm: &mut LiteSVM, scene: &Scene, run: Instruction) -> Result<Outcome, Failure> {
    lending::next_slot(svm);
    let mut instructions = vec![lending::compute_limit()];
    instructions.extend(kamino::refreshes(svm, &scene.obligation, &[USDC_RESERVE]));
    instructions.push(run);
    tx::send(svm, &scene.owner, &[], &instructions, &[])
}

#[test]
fn deposits_into_kamino_exactly_what_marginfi_released() {
    let (mut svm, scene) = scene();
    let usdc = kamino::reserve_accounts(&svm, &USDC_RESERVE);
    let marginfi_vault = marginfi::bank(&svm, &USDC_BANK).liquidity_vault;
    assert!(marginfi::health_accounts(&svm, &scene.marginfi_account, &USDC_BANK).is_empty());
    let vault_before = wallet::token_balance(&svm, &marginfi_vault);
    let supply_before = wallet::token_balance(&svm, &usdc.supply_vault);
    let collateral_before = wallet::token_balance(&svm, &usdc.collateral_supply);

    let run = run(&svm, &scene, 1);
    let outcome = send_run(&mut svm, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
    eprintln!(
        "{NAME}: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );

    let released = vault_before - wallet::token_balance(&svm, &marginfi_vault);
    assert!(
        (DEPOSIT - 1..=DEPOSIT).contains(&released),
        "marginfi released {released}"
    );
    let taken = wallet::token_balance(&svm, &usdc.supply_vault) - supply_before;
    let left = wallet::token_balance(&svm, &scene.wallet_ata);
    let minted = wallet::token_balance(&svm, &usdc.collateral_supply) - collateral_before;
    eprintln!("{NAME}: released {released}, deposited {taken}, left {left}, minted {minted}");
    assert!(minted > 0);
    assert_eq!(
        kamino::deposited(&svm, &scene.obligation, &USDC_RESERVE),
        minted
    );
    // The run asked Kamino for all of it; Kamino kept back only its cToken rounding.
    assert_eq!(
        kamino::deposits_requested(&outcome.logs),
        [(USDC_RESERVE, released)]
    );
    kamino::assert_deposit_took_all_but_rounding(released, taken, left, minted);
    assert_eq!(marginfi::active_banks(&svm, &scene.marginfi_account), []);
}

#[test]
fn a_floor_above_the_position_refuses_at_worth_rebalancing() {
    let (mut svm, scene) = scene();
    let run = run(&svm, &scene, DEPOSIT + 1);
    let failure = send_run(&mut svm, &scene, run).unwrap_err();
    tx::assert_requirement_failed(&failure, &template::examples()[NAME], "worthRebalancing");
    assert_eq!(
        marginfi::active_banks(&svm, &scene.marginfi_account),
        [USDC_BANK]
    );
    assert_eq!(wallet::token_balance(&svm, &scene.wallet_ata), 0);
    assert_eq!(kamino::deposited(&svm, &scene.obligation, &USDC_RESERVE), 0);
}
