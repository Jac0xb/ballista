//! `marginfiWithdrawAllWithFloor` against marginfi: empty a 100 USDC balance, require enough came
//! out, and sweep it to a treasury.

use {
    ballista_protocol_tests::{
        lending::{self, MARGINFI_GROUP, SOL_BANK, USDC_BANK, USDC_MINT},
        marginfi,
        template::{self, Run},
        tx::{self, Failure, Outcome},
        wallet,
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const NAME: &str = "marginfiWithdrawAllWithFloor";
/// USDC base units deposited.
const DEPOSIT: u64 = 100_000_000;
/// The runs' `minimumWithdrawn`: 99 USDC, clear of marginfi's share rounding, which can keep one
/// base unit of the deposit.
const FLOOR: u64 = 99_000_000;

struct Scene {
    authority: Keypair,
    account: Address,
    /// The authority's USDC account: the deposit came from it and the withdrawal lands in it.
    usdc: Address,
    /// A second USDC account of the authority's: the template sweeps only to the authority.
    treasury: Address,
    template: Address,
}

/// A marginfi account holding 100 USDC and, with `with_sol`, 1 SOL, deposited from its
/// authority's token accounts (rule 1); an empty treasury, created as a second USDC account of the
/// authority's; the template, uploaded.
fn scene(with_sol: bool) -> (LiteSVM, Scene) {
    let (mut svm, authority, account, usdc) = lending::marginfi_scene(with_sol, DEPOSIT);
    let a = authority.pubkey();
    let treasury = wallet::keypair(b"ballista-protocol-tests-treasury");
    let create = wallet::create_token_account(&svm, &a, &treasury.pubkey(), &USDC_MINT, &a);
    lending::setup(&mut svm, &authority, &[&treasury], create);
    let treasury = treasury.pubkey();
    let template = lending::upload(&mut svm, &template::examples()[NAME]);
    (
        svm,
        Scene {
            authority,
            account,
            usdc,
            treasury,
            template,
        },
    )
}

fn run(svm: &LiteSVM, scene: &Scene, minimum_withdrawn: u64) -> Instruction {
    run_paying(svm, scene, scene.usdc, scene.treasury, minimum_withdrawn)
}

/// [`run`] with the withdrawal paid into `destination` and swept on to `treasury`, which a hostile
/// builder may name.
fn run_paying(
    svm: &LiteSVM,
    scene: &Scene,
    destination: Address,
    treasury: Address,
    minimum_withdrawn: u64,
) -> Instruction {
    let examples = template::examples();
    Run::new(scene.template, &examples[NAME])
        .group(
            "healthAccounts",
            marginfi::health_accounts(svm, &scene.account, &USDC_BANK),
        )
        .account("marginfi", marginfi::MARGINFI, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("marginfiGroup", MARGINFI_GROUP, false, false)
        .account("marginfiAccount", scene.account, true, false)
        .account("authority", scene.authority.pubkey(), false, true)
        .account("bank", USDC_BANK, true, false)
        .account(
            "bankLiquidityVault",
            marginfi::bank(svm, &USDC_BANK).liquidity_vault,
            true,
            false,
        )
        // A PDA marginfi signs for; nothing writes it.
        .account(
            "bankLiquidityVaultAuthority",
            marginfi::vault_authority(&USDC_BANK),
            false,
            false,
        )
        .account("destinationAta", destination, true, false)
        .account("treasuryAta", treasury, true, false)
        .input_u64("minimumWithdrawn", minimum_withdrawn)
        .build()
}

/// The run in a new slot. marginfi has no refreshes to go before it.
fn send_run(svm: &mut LiteSVM, scene: &Scene, run: Instruction) -> Result<Outcome, Failure> {
    lending::next_slot(svm);
    tx::send(
        svm,
        &scene.authority,
        &[],
        &[lending::compute_limit(), run],
        &[],
    )
}

/// The withdrawal was swept whole into the treasury: marginfi's share rounding can keep one base
/// unit of the deposit.
fn assert_swept(svm: &LiteSVM, scene: &Scene) -> u64 {
    let withdrawn = wallet::token_balance(svm, &scene.treasury);
    assert!(
        (DEPOSIT - 1..=DEPOSIT).contains(&withdrawn),
        "withdrew {withdrawn}"
    );
    assert_eq!(wallet::token_balance(svm, &scene.usdc), 0);
    withdrawn
}

#[test]
fn withdraws_a_sole_position_and_sweeps_it() {
    let (mut svm, scene) = scene(false);
    assert!(marginfi::health_accounts(&svm, &scene.account, &USDC_BANK).is_empty());
    let run = run(&svm, &scene, FLOOR);
    let outcome = send_run(&mut svm, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
    eprintln!(
        "{NAME}: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
    let withdrawn = assert_swept(&svm, &scene);
    eprintln!("{NAME}: withdrew {withdrawn} of {DEPOSIT}");
    assert_eq!(marginfi::active_banks(&svm, &scene.account), []);
}

#[test]
fn withdraws_one_position_of_two_with_the_health_group() {
    let (mut svm, scene) = scene(true);
    assert_eq!(
        marginfi::health_accounts(&svm, &scene.account, &USDC_BANK).len(),
        2
    );
    let run = run(&svm, &scene, FLOOR);
    let outcome = send_run(&mut svm, &scene, run).unwrap_or_else(|failure| panic!("{failure:?}"));
    eprintln!(
        "{NAME} with a second balance: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
    assert_swept(&svm, &scene);
    assert_eq!(marginfi::active_banks(&svm, &scene.account), [SOL_BANK]);
}

#[test]
fn a_floor_above_the_position_refuses_at_withdrawal_met_its_floor() {
    let (mut svm, scene) = scene(false);
    let run = run(&svm, &scene, DEPOSIT + 1);
    let failure = send_run(&mut svm, &scene, run).unwrap_err();
    tx::assert_requirement_failed(
        &failure,
        &template::examples()[NAME],
        "withdrawalMetItsFloor",
    );
    assert_eq!(marginfi::active_banks(&svm, &scene.account), [USDC_BANK]);
    assert_eq!(wallet::token_balance(&svm, &scene.treasury), 0);
}

/// Sends a run whose builder named the attacker's `attacker` account as `destination`, `treasury`
/// or both, and asserts it stops at `label` with nothing moved. Before the template checked owners
/// it landed, and the panic says what the attacker got.
fn assert_hostile_run_refused(
    mut svm: LiteSVM,
    scene: &Scene,
    (destination, treasury): (Address, Address),
    attacker: Address,
    label: &str,
) {
    let run = run_paying(&svm, scene, destination, treasury, FLOOR);
    let failure = match send_run(&mut svm, scene, run) {
        Err(failure) => failure,
        Ok(_) => panic!(
            "the run landed: the attacker's account holds {} of the {DEPOSIT} deposited, and the \
             authority's accounts {} and {}",
            wallet::token_balance(&svm, &attacker),
            wallet::token_balance(&svm, &scene.usdc),
            wallet::token_balance(&svm, &scene.treasury)
        ),
    };
    tx::assert_requirement_failed(&failure, &template::examples()[NAME], label);
    assert_eq!(marginfi::active_banks(&svm, &scene.account), [USDC_BANK]);
    assert_eq!(wallet::token_balance(&svm, &attacker), 0);
}

/// marginfi pays the withdrawal into the authority's account, and the sweep then pays whatever
/// treasury the run names: here the attacker's.
#[test]
fn an_attackers_treasury_is_refused_at_sweep_goes_to_the_authority() {
    let (mut svm, scene) = scene(false);
    let attacker = lending::attacker_account(&mut svm, &USDC_MINT, None);
    assert_hostile_run_refused(
        svm,
        &scene,
        (scene.usdc, attacker),
        attacker,
        "sweepGoesToTheAuthority",
    );
}

/// marginfi pays any USDC account it is given. The attacker names theirs as the destination and
/// the treasury, and has approved the authority, so the sweep's transfer out of it passes.
#[test]
fn an_attackers_destination_is_refused_at_withdrawal_goes_to_the_authority() {
    let (mut svm, scene) = scene(false);
    let attacker = lending::attacker_account(&mut svm, &USDC_MINT, Some(&scene.authority.pubkey()));
    assert_hostile_run_refused(
        svm,
        &scene,
        (attacker, attacker),
        attacker,
        "withdrawalGoesToTheAuthority",
    );
}
