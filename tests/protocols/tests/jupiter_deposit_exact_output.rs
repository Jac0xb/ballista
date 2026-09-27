//! `jupiterDepositExactOutput` against Jupiter and Kamino: sell 1 SOL through the snapshotted
//! route, and deposit exactly what it paid into Kamino's USDC reserve.

use {
    ballista_protocol_tests::{
        kamino,
        lending::{self, MARKET, SOL_TO_USDC, USDC_MINT, USDC_RESERVE},
        snapshot::Leg,
        template::{self, Run},
        tx::{self, Failure, Outcome},
        wallet::{self, SOL, WSOL_MINT},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const NAME: &str = "jupiterDepositExactOutput";
const JUPITER: Address = Address::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

struct Scene {
    owner: Keypair,
    leg: Leg,
    obligation: Address,
    template: Address,
}

/// Sets up the wallet the route was built for:
/// - it holds exactly the wrapped SOL the route sells, and an empty USDC account for the route to
///   pay into (rule 1);
/// - it has a Kamino obligation with its USDC farm user state.
fn scene() -> (LiteSVM, Scene) {
    let leg = lending::snapshot().route(SOL_TO_USDC).legs[0].clone();
    let mut svm = lending::svm();
    let owner = wallet::wallet();
    let o = owner.pubkey();
    wallet::fund(&mut svm, &o, 10 * SOL);
    assert_eq!(
        wallet::token_account(&mut svm, &o, &WSOL_MINT, leg.in_amount),
        leg.source_token_account
    );
    assert_eq!(
        wallet::token_account(&mut svm, &o, &USDC_MINT, 0),
        leg.destination_token_account
    );
    let obligation = lending::open_obligation(&mut svm, &owner, &[USDC_RESERVE]);
    let template = lending::upload(&mut svm, &template::examples()[NAME]);
    (
        svm,
        Scene {
            owner,
            leg,
            obligation,
            template,
        },
    )
}

fn run(svm: &LiteSVM, scene: &Scene, minimum_out: u64) -> Instruction {
    let examples = template::examples();
    let usdc = kamino::reserve_accounts(svm, &USDC_RESERVE);
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
        .account("owner", scene.owner.pubkey(), true, true)
        .account("sourceAta", scene.leg.source_token_account, true, false)
        .account(
            "destinationAta",
            scene.leg.destination_token_account,
            true,
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
        .input_bytes("routeArgs", &scene.leg.route.args)
        .input_u64("minimumOut", minimum_out)
        .group("routeAccounts", lending::route_group(&scene.leg))
        .group(
            "farmAccounts",
            kamino::deposit_farm_accounts(svm, &scene.obligation, &USDC_RESERVE),
        )
        .build()
}

/// The run in a new slot, behind the refreshes klend needs in that slot. The deposit refreshes its
/// own reserve too; refreshing it here as well is what the probe measured, and is harmless.
fn send_run(svm: &mut LiteSVM, scene: &Scene, run: Instruction) -> Result<Outcome, Failure> {
    lending::next_slot(svm);
    let mut instructions = vec![lending::compute_limit()];
    instructions.extend(kamino::refreshes(svm, &scene.obligation, &[USDC_RESERVE]));
    instructions.push(run);
    tx::send(
        svm,
        &scene.owner,
        &[],
        &instructions,
        &scene.leg.lookup_tables,
    )
}

#[test]
fn deposits_exactly_what_the_swap_produced() {
    let (mut svm, scene) = scene();
    let usdc = kamino::reserve_accounts(&svm, &USDC_RESERVE);
    // Replayed in the next slot, the slot the run lands in.
    let produced = lending::swap_output(&svm, &scene.leg);
    let supply_before = wallet::token_balance(&svm, &usdc.supply_vault);
    let collateral_before = wallet::token_balance(&svm, &usdc.collateral_supply);

    let run = run(&svm, &scene, produced);
    let outcome = send_run(&mut svm, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
    eprintln!(
        "{NAME}: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );

    assert_eq!(
        wallet::token_balance(&svm, &scene.leg.source_token_account),
        0
    );
    let taken = wallet::token_balance(&svm, &usdc.supply_vault) - supply_before;
    let left = wallet::token_balance(&svm, &scene.leg.destination_token_account);
    let minted = wallet::token_balance(&svm, &usdc.collateral_supply) - collateral_before;
    eprintln!("{NAME}: produced {produced}, deposited {taken}, left {left}, minted {minted}");
    assert!(minted > 0);
    assert_eq!(
        kamino::deposited(&svm, &scene.obligation, &USDC_RESERVE),
        minted
    );
    // The run asked Kamino for all of it; Kamino kept back only its cToken rounding.
    kamino::assert_deposit_took_all_but_rounding(produced, taken, left, minted);
}

#[test]
fn a_floor_above_the_fill_refuses_at_swap_met_its_floor() {
    let (mut svm, scene) = scene();
    let usdc = kamino::reserve_accounts(&svm, &USDC_RESERVE);
    let produced = lending::swap_output(&svm, &scene.leg);
    let supply_before = wallet::token_balance(&svm, &usdc.supply_vault);

    let run = run(&svm, &scene, produced + 1);
    let failure = send_run(&mut svm, &scene, run).unwrap_err();
    tx::assert_requirement_failed(&failure, &template::examples()[NAME], "swapMetItsFloor");
    assert_eq!(
        wallet::token_balance(&svm, &usdc.supply_vault),
        supply_before
    );
    assert_eq!(kamino::deposited(&svm, &scene.obligation, &USDC_RESERVE), 0);
}

/// Why every template transaction starts in a new slot: klend counts an obligation refreshed
/// anywhere earlier in the slot as fresh.
#[test]
fn a_setup_refresh_hides_a_missing_refresh_until_the_slot_moves() {
    let (mut svm, scene) = scene();
    let refreshes = kamino::refreshes(&svm, &scene.obligation, &[]);
    lending::setup(&mut svm, &scene.owner, &[], refreshes);
    let bare = [lending::compute_limit(), run(&svm, &scene, 1)];

    let mut same_slot = svm.clone();
    tx::send(
        &mut same_slot,
        &scene.owner,
        &[],
        &bare,
        &scene.leg.lookup_tables,
    )
    .unwrap_or_else(|failure| panic!("in the setup's slot: {failure:?}"));

    lending::next_slot(&mut svm);
    let failure =
        tx::send(&mut svm, &scene.owner, &[], &bare, &scene.leg.lookup_tables).unwrap_err();
    assert_eq!(
        (failure.program, failure.code),
        (kamino::KLEND, Some(6017)), // ObligationStale
        "{failure:?}"
    );
}
