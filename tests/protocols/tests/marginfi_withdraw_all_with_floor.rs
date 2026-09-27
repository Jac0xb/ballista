//! `marginfiWithdrawAllWithFloor` against marginfi: empty a 100 USDC balance, require enough came
//! out, and sweep it to a treasury.

use {
    ballista_protocol_tests::{
        lending::{self, MARGINFI_GROUP, SOL_BANK, USDC_BANK, USDC_MINT},
        marginfi,
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

const NAME: &str = "marginfiWithdrawAllWithFloor";
/// USDC base units deposited.
const DEPOSIT: u64 = 100_000_000;

struct Scene {
    authority: Keypair,
    account: Address,
    /// The authority's USDC account: the deposit came from it and the withdrawal lands in it.
    usdc: Address,
    treasury: Address,
    template: Address,
}

/// A marginfi account holding 100 USDC and, with `with_sol`, 1 SOL, deposited from its
/// authority's token accounts (rule 1); an empty USDC treasury; the template, uploaded.
fn scene(with_sol: bool) -> (LiteSVM, Scene) {
    let mut svm = lending::svm();
    let authority = wallet::keypair(b"ballista-protocol-tests-mfi-auth");
    let account = wallet::keypair(b"ballista-protocol-tests-mfi-acct");
    let a = authority.pubkey();
    wallet::fund(&mut svm, &a, 10 * SOL);
    let usdc = wallet::token_account(&mut svm, &a, &USDC_MINT, DEPOSIT);
    let mut deposits = vec![(USDC_BANK, usdc, DEPOSIT)];
    if with_sol {
        let wsol = wallet::token_account(&mut svm, &a, &WSOL_MINT, SOL);
        deposits.push((SOL_BANK, wsol, SOL));
    }
    let account = lending::marginfi_account(&mut svm, &authority, &account, &deposits);
    let treasury_owner = wallet::keypair(b"ballista-protocol-tests-treasury").pubkey();
    let treasury = wallet::token_account(&mut svm, &treasury_owner, &USDC_MINT, 0);
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
        .account("destinationAta", scene.usdc, true, false)
        .account("treasuryAta", scene.treasury, true, false)
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
    let run = run(&svm, &scene, 99_000_000);
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
    let run = run(&svm, &scene, 99_000_000);
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
