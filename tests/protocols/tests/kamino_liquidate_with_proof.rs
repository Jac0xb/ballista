//! `kaminoLiquidateWithProof` against Kamino: liquidate an obligation whose SOL collateral fell 12%,
//! repaying part of its USDC debt, and require the SOL it pays out to cover a bounty.

use {
    ballista_protocol_tests::{
        kamino,
        lending::{self, Unhealthy, MARKET, SOL_RESERVE, USDC_RESERVE},
        template::{self, Run},
        tx::{self, Failure, Outcome},
        wallet::{self, WSOL_MINT},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const NAME: &str = "kaminoLiquidateWithProof";

struct Scene {
    unhealthy: Unhealthy,
    liquidator: Keypair,
    accounts: kamino::Liquidator,
    template: Address,
}

/// An obligation klend will liquidate, a liquidator holding its whole debt in USDC, and the
/// template, uploaded.
fn scene() -> (LiteSVM, Scene) {
    let mut svm = lending::svm();
    let unhealthy = lending::unhealthy_obligation(&mut svm);
    let (liquidator, accounts) = lending::liquidator(&mut svm, unhealthy.debt);
    let template = lending::upload(&mut svm, &template::examples()[NAME]);
    (
        svm,
        Scene {
            unhealthy,
            liquidator,
            accounts,
            template,
        },
    )
}

fn run(svm: &LiteSVM, scene: &Scene, liquidity_amount: u64, minimum_bounty: u64) -> Instruction {
    run_paying(
        svm,
        scene,
        &scene.accounts,
        liquidity_amount,
        minimum_bounty,
    )
}

/// [`run`] with the liquidator's token accounts in `accounts`, which a hostile builder may swap.
fn run_paying(
    svm: &LiteSVM,
    scene: &Scene,
    accounts: &kamino::Liquidator,
    liquidity_amount: u64,
    minimum_bounty: u64,
) -> Instruction {
    let examples = template::examples();
    let repay = kamino::reserve_accounts(svm, &USDC_RESERVE);
    let withdraw = kamino::reserve_accounts(svm, &SOL_RESERVE);
    let obligation = scene.unhealthy.obligation;
    Run::new(scene.template, &examples[NAME])
        .account("kamino", kamino::KLEND, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account(
            "instructionsSysvar",
            kamino::INSTRUCTIONS_SYSVAR,
            false,
            false,
        )
        .account("liquidator", scene.liquidator.pubkey(), true, true)
        .account("obligation", obligation, true, false)
        .account("lendingMarket", MARKET, false, false)
        .account(
            "lendingMarketAuthority",
            kamino::lending_market_authority(&MARKET),
            false,
            false,
        )
        .account("repayReserve", USDC_RESERVE, true, false)
        .account(
            "repayReserveLiquidityMint",
            repay.liquidity_mint,
            false,
            false,
        )
        .account(
            "repayReserveLiquiditySupply",
            repay.supply_vault,
            true,
            false,
        )
        .account("withdrawReserve", SOL_RESERVE, true, false)
        .account(
            "withdrawReserveLiquidityMint",
            withdraw.liquidity_mint,
            false,
            false,
        )
        .account(
            "withdrawReserveCollateralMint",
            withdraw.collateral_mint,
            true,
            false,
        )
        .account(
            "withdrawReserveCollateralSupply",
            withdraw.collateral_supply,
            true,
            false,
        )
        .account(
            "withdrawReserveLiquiditySupply",
            withdraw.supply_vault,
            true,
            false,
        )
        .account(
            "withdrawReserveFeeReceiver",
            withdraw.fee_vault,
            true,
            false,
        )
        .account(
            "userSourceLiquidity",
            accounts.source_liquidity,
            true,
            false,
        )
        .account(
            "userDestinationCollateral",
            accounts.destination_collateral,
            true,
            false,
        )
        .account(
            "userDestinationLiquidity",
            accounts.destination_liquidity,
            true,
            false,
        )
        .input_u64("liquidityAmount", liquidity_amount)
        // Kamino's own floor stays out of the way, so the template's is the one tested.
        .input_u64("minAcceptableReceived", 0)
        .input_u64("minimumBounty", minimum_bounty)
        .group(
            "farmAccounts",
            kamino::liquidation_farm_accounts(svm, &obligation, &USDC_RESERVE, &SOL_RESERVE),
        )
        .build()
}

/// The run in a new slot, behind the refreshes klend needs in it, signed by the liquidator.
fn send_run(svm: &mut LiteSVM, scene: &Scene, run: Instruction) -> Result<Outcome, Failure> {
    lending::next_slot(svm);
    let mut instructions = vec![lending::compute_limit()];
    instructions.extend(kamino::refreshes(svm, &scene.unhealthy.obligation, &[]));
    instructions.push(run);
    tx::send(svm, &scene.liquidator, &[], &instructions, &[])
}

/// What the liquidator holds: USDC to repay with, the SOL paid out, and cTokens.
fn holdings(svm: &LiteSVM, scene: &Scene) -> (u64, u64, u64) {
    (
        wallet::token_balance(svm, &scene.accounts.source_liquidity),
        wallet::token_balance(svm, &scene.accounts.destination_liquidity),
        wallet::token_balance(svm, &scene.accounts.destination_collateral),
    )
}

#[test]
fn the_liquidator_nets_at_least_the_bounty() {
    let (mut svm, scene) = scene();
    let Unhealthy {
        obligation,
        debt,
        sol_price,
        usdc_price,
    } = scene.unhealthy;
    // Not vacuous: refreshed in the run's slot, klend counts the obligation as liquidatable.
    let mut refreshed = svm.clone();
    lending::next_slot(&mut refreshed);
    let refreshes = kamino::refreshes(&refreshed, &obligation, &[]);
    lending::setup(&mut refreshed, &scene.liquidator, &[], refreshes);
    assert!(kamino::is_liquidatable(&refreshed, &obligation));

    // The market's close factor: a tenth of the debt at a time.
    let liquidity_amount = debt / 10;
    // At least what was repaid, valued at the oracle price.
    let minimum_bounty = lending::break_even(liquidity_amount, usdc_price, sol_price);
    let run = run(&svm, &scene, liquidity_amount, minimum_bounty);
    let outcome = send_run(&mut svm, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
    eprintln!(
        "{NAME}: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );

    let (usdc_left, received, ctokens) = holdings(&svm, &scene);
    let repaid = debt - usdc_left;
    eprintln!(
        "{NAME}: repaid {repaid} of {debt}, received {received} lamports, bounty {minimum_bounty}"
    );
    assert!(repaid > 0 && repaid <= liquidity_amount, "repaid {repaid}");
    assert!(received >= minimum_bounty, "{received} < {minimum_bounty}");
    assert!(received > lending::break_even(repaid, usdc_price, sol_price));
    assert_eq!(
        ctokens, 0,
        "klend redeems the seized cTokens in the same instruction"
    );
}

#[test]
fn a_bounty_above_the_payout_refuses_at_liquidation_paid_the_bounty() {
    let (mut svm, scene) = scene();
    let liquidity_amount = scene.unhealthy.debt / 10;
    let payout = {
        let mut probe = svm.clone();
        let run = run(&probe, &scene, liquidity_amount, 0);
        send_run(&mut probe, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
        holdings(&probe, &scene).1
    };
    assert!(payout > 0);
    let before = holdings(&svm, &scene);

    let run = run(&svm, &scene, liquidity_amount, payout + 1);
    let failure = send_run(&mut svm, &scene, run).unwrap_err();
    tx::assert_requirement_failed(
        &failure,
        &template::examples()[NAME],
        "liquidationPaidTheBounty",
    );
    assert_eq!(holdings(&svm, &scene), before);
}

/// Sends a run whose builder put the attacker's `account` into `accounts`, at the bounty the
/// honest run clears, and asserts it stops at `label` with nothing moved. Before the template
/// checked owners it landed, and the panic says what the attacker got.
fn assert_hostile_run_refused(
    mut svm: LiteSVM,
    scene: &Scene,
    accounts: &kamino::Liquidator,
    account: Address,
    label: &str,
) {
    let Unhealthy {
        debt,
        sol_price,
        usdc_price,
        ..
    } = scene.unhealthy;
    let liquidity_amount = debt / 10;
    let minimum_bounty = lending::break_even(liquidity_amount, usdc_price, sol_price);
    let before = (holdings(&svm, scene), wallet::token_balance(&svm, &account));
    let run = run_paying(&svm, scene, accounts, liquidity_amount, minimum_bounty);
    let failure = match send_run(&mut svm, scene, run) {
        Err(failure) => failure,
        Ok(_) => panic!(
            "the run landed: the attacker's account went from {} to {}, and the liquidator's \
             (USDC, SOL, cTokens) from {:?} to {:?}",
            before.1,
            wallet::token_balance(&svm, &account),
            before.0,
            holdings(&svm, scene)
        ),
    };
    tx::assert_requirement_failed(&failure, &template::examples()[NAME], label);
    assert_eq!(
        (holdings(&svm, scene), wallet::token_balance(&svm, &account)),
        before
    );
}

/// klend checks that `userDestinationLiquidity` holds the seized reserve's mint, not whose it is,
/// so a builder can name the attacker's wrapped SOL account and the bounty is measured there.
/// Having approved the liquidator, the attacker also passes klend's fee transfer out of it.
#[test]
fn an_attackers_payout_account_is_refused_at_bounty_goes_to_the_liquidator() {
    let (mut svm, scene) = scene();
    let attacker =
        lending::attacker_account(&mut svm, &WSOL_MINT, Some(&scene.liquidator.pubkey()));
    let hostile = kamino::Liquidator {
        destination_liquidity: attacker,
        ..scene.accounts
    };
    assert_hostile_run_refused(svm, &scene, &hostile, attacker, "bountyGoesToTheLiquidator");
}

/// The seized cTokens pass through `userDestinationCollateral`, and whatever klend cannot redeem
/// stays there. Having approved the liquidator, the attacker's cToken account passes klend's burn.
#[test]
fn an_attackers_collateral_account_is_refused_at_seized_collateral_goes_to_the_liquidator() {
    let (mut svm, scene) = scene();
    let collateral_mint = kamino::reserve_accounts(&svm, &SOL_RESERVE).collateral_mint;
    let attacker =
        lending::attacker_account(&mut svm, &collateral_mint, Some(&scene.liquidator.pubkey()));
    let hostile = kamino::Liquidator {
        destination_collateral: attacker,
        ..scene.accounts
    };
    assert_hostile_run_refused(
        svm,
        &scene,
        &hostile,
        attacker,
        "seizedCollateralGoesToTheLiquidator",
    );
}
