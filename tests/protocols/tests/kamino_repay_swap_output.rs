//! `kaminoRepaySwapOutput` against Jupiter and Kamino: sell 1 SOL of collateral through the
//! snapshotted route, and repay exactly what it paid against a USDC borrow.

use {
    ballista_protocol_tests::{
        kamino,
        lending::{self, JUPITER, MARKET, SOL_RESERVE, SOL_TO_USDC, USDC_MINT, USDC_RESERVE},
        snapshot::Leg,
        template::{self, Run},
        tx::{self, Failure, Outcome},
        wallet::{self, SOL, WSOL_MINT},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const NAME: &str = "kaminoRepaySwapOutput";

struct Scene {
    borrower: Keypair,
    leg: Leg,
    obligation: Address,
    /// USDC base units borrowed.
    debt: u64,
    template: Address,
}

/// Three times the route's quote, so one repayment cannot clear it.
fn three_quotes(leg: &Leg) -> u64 {
    3 * leg.out_amount
}

/// The wallet the route was built for, as a Kamino borrower:
/// - 10 SOL deposited as collateral, then [`three_quotes`] borrowed in USDC into its USDC account,
///   the route's destination;
/// - then its wrapped SOL account holding exactly what the route sells (rule 1).
fn scene() -> (LiteSVM, Scene) {
    scene_owing(three_quotes)
}

/// [`scene`], having borrowed `debt(leg)`.
fn scene_owing(debt: fn(&Leg) -> u64) -> (LiteSVM, Scene) {
    let leg = lending::snapshot().route(SOL_TO_USDC).legs[0].clone();
    let mut svm = lending::svm();
    let borrower = wallet::wallet();
    let b = borrower.pubkey();
    wallet::fund(&mut svm, &b, 100 * SOL);
    let collateral = wallet::token_account(&mut svm, &b, &WSOL_MINT, 10 * SOL);
    assert_eq!(collateral, leg.source_token_account);
    assert_eq!(
        wallet::token_account(&mut svm, &b, &USDC_MINT, 0),
        leg.destination_token_account
    );
    let obligation = lending::open_obligation(&mut svm, &borrower, &[SOL_RESERVE]);
    lending::deposit(
        &mut svm,
        &borrower,
        &obligation,
        &SOL_RESERVE,
        &collateral,
        10 * SOL,
    );
    let debt = debt(&leg);
    lending::borrow(
        &mut svm,
        &borrower,
        &obligation,
        &USDC_RESERVE,
        &leg.destination_token_account,
        debt,
    );
    wallet::token_account(&mut svm, &b, &WSOL_MINT, leg.in_amount);
    let template = lending::upload(&mut svm, &template::examples()[NAME]);
    (
        svm,
        Scene {
            borrower,
            leg,
            obligation,
            debt,
            template,
        },
    )
}

fn run(svm: &LiteSVM, scene: &Scene, minimum_repayment: u64) -> Instruction {
    run_paying(
        svm,
        scene,
        scene.leg.destination_token_account,
        minimum_repayment,
    )
}

/// [`run`] with the swap paying `destination`, which a hostile builder may name: as
/// `borrowedAssetAta` and wherever the route's own accounts name the borrower's USDC account.
fn run_paying(
    svm: &LiteSVM,
    scene: &Scene,
    destination: Address,
    minimum_repayment: u64,
) -> Instruction {
    let examples = template::examples();
    let usdc = kamino::reserve_accounts(svm, &USDC_RESERVE);
    let route_accounts = lending::route_group(&scene.leg).into_iter().map(|meta| {
        if meta.pubkey == scene.leg.destination_token_account {
            AccountMeta {
                pubkey: destination,
                ..meta
            }
        } else {
            meta
        }
    });
    Run::new(scene.template, &examples[NAME])
        .account("jupiter", JUPITER, false, false)
        .account("kamino", kamino::KLEND, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account(
            "instructionsSysvar",
            kamino::INSTRUCTIONS_SYSVAR,
            false,
            false,
        )
        .account("borrower", scene.borrower.pubkey(), false, true)
        .account("collateralAta", scene.leg.source_token_account, true, false)
        .account("borrowedAssetAta", destination, true, false)
        .account("obligation", scene.obligation, true, false)
        .account("lendingMarket", MARKET, false, false)
        .account("repayReserve", USDC_RESERVE, true, false)
        .account("reserveLiquidityMint", usdc.liquidity_mint, false, false)
        .account("reserveLiquiditySupply", usdc.supply_vault, true, false)
        .input_bytes("routeArgs", &scene.leg.route.args)
        .input_u64("minimumRepayment", minimum_repayment)
        .group("routeAccounts", route_accounts)
        .group(
            "farmAccounts",
            kamino::repay_farm_accounts(svm, &scene.obligation, &USDC_RESERVE),
        )
        .build()
}

/// The run in a new slot, behind the refreshes klend needs in it: both reserves the obligation
/// holds, then the obligation.
fn send_run(svm: &mut LiteSVM, scene: &Scene, run: Instruction) -> Result<Outcome, Failure> {
    lending::send_run(
        svm,
        &scene.obligation,
        &[],
        &scene.borrower,
        run,
        &scene.leg.lookup_tables,
    )
}

#[test]
fn repays_exactly_what_the_swap_produced() {
    let (mut svm, scene) = scene();
    let usdc = kamino::reserve_accounts(&svm, &USDC_RESERVE);
    let produced = lending::swap_output(&svm, &scene.leg);
    assert!(
        produced < scene.debt,
        "{produced} would clear {}",
        scene.debt
    );
    // The debt as the run will see it: in the next slot, after the same refreshes.
    let debt_before = {
        let mut refreshed = svm.clone();
        lending::next_slot(&mut refreshed);
        let refreshes = kamino::refreshes(&refreshed, &scene.obligation, &[]);
        lending::setup(&mut refreshed, &scene.borrower, &[], refreshes);
        kamino::borrowed_sf(&refreshed, &scene.obligation, &USDC_RESERVE)
    };
    let supply_before = wallet::token_balance(&svm, &usdc.supply_vault);

    let run = run(&svm, &scene, produced);
    let outcome = send_run(&mut svm, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
    eprintln!(
        "{NAME}: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );

    assert_eq!(
        debt_before - kamino::borrowed_sf(&svm, &scene.obligation, &USDC_RESERVE),
        u128::from(produced) << 60,
        "the debt fell by exactly the swap's output"
    );
    assert_eq!(
        wallet::token_balance(&svm, &usdc.supply_vault) - supply_before,
        produced
    );
    assert_eq!(
        wallet::token_balance(&svm, &scene.leg.destination_token_account),
        scene.debt,
        "the output went to the debt, not the wallet"
    );
}

#[test]
fn a_floor_above_the_swap_refuses_at_swap_worth_repaying() {
    let (mut svm, scene) = scene();
    let produced = lending::swap_output(&svm, &scene.leg);
    let debt_before = kamino::borrowed_sf(&svm, &scene.obligation, &USDC_RESERVE);

    let run = run(&svm, &scene, produced + 1);
    let failure = send_run(&mut svm, &scene, run).unwrap_err();
    tx::assert_requirement_failed(&failure, &template::examples()[NAME], "swapWorthRepaying");
    assert_eq!(
        kamino::borrowed_sf(&svm, &scene.obligation, &USDC_RESERVE),
        debt_before
    );
    assert_eq!(
        wallet::token_balance(&svm, &scene.leg.source_token_account),
        scene.leg.in_amount,
        "the swap reverted with the run"
    );
}

/// klend's repayment takes any USDC account the borrower can spend from, including one whose owner
/// approved the borrower as its delegate. So a builder can have the route pay an attacker's account
/// approved that way. klend repays only the debt, here half what the swap produced, and the rest
/// would stay with the attacker.
#[test]
fn an_attackers_swap_destination_is_refused_at_swap_pays_the_borrower() {
    let (mut svm, scene) = scene_owing(|leg| leg.out_amount / 2);
    let attacker = lending::attacker_account(&mut svm, &USDC_MINT, Some(&scene.borrower.pubkey()));
    let debt_before = kamino::borrowed_sf(&svm, &scene.obligation, &USDC_RESERVE);
    let collateral_before = wallet::token_balance(&svm, &scene.leg.source_token_account);

    let run = run_paying(&svm, &scene, attacker, 1);
    let failure = match send_run(&mut svm, &scene, run) {
        Err(failure) => failure,
        Ok(_) => panic!(
            "the run landed: it sold {} lamports of the borrower's SOL, the debt went from {} to \
             {} USDC units, and the attacker kept {}",
            collateral_before - wallet::token_balance(&svm, &scene.leg.source_token_account),
            debt_before >> 60,
            kamino::borrowed_sf(&svm, &scene.obligation, &USDC_RESERVE) >> 60,
            wallet::token_balance(&svm, &attacker)
        ),
    };
    tx::assert_requirement_failed(&failure, &template::examples()[NAME], "swapPaysTheBorrower");
    assert_eq!(
        kamino::borrowed_sf(&svm, &scene.obligation, &USDC_RESERVE),
        debt_before
    );
    assert_eq!(wallet::token_balance(&svm, &attacker), 0);
}
