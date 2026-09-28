//! What marginfi's withdrawal requires of a caller, against the deployed program.

use {
    ballista_protocol_tests::{
        lending::{self, MARGINFI_GROUP, SOL_BANK, USDC_BANK, USDC_MINT},
        marginfi,
        tx::{self, Failure, Outcome},
        wallet::{self, SOL, WSOL_MINT},
    },
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::AccountMeta,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const DEPOSIT: u64 = 100_000_000;

struct Scene {
    authority: Keypair,
    account: Address,
    usdc: Address,
}

/// An account (seed `ballista-protocol-tests-mfi-acct`) of `ballista-protocol-tests-mfi-auth`
/// holding 100 USDC and, with `with_sol`, 1 SOL, one slot after setup.
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
    lending::next_slot(&mut svm);
    (
        svm,
        Scene {
            authority,
            account,
            usdc,
        },
    )
}

/// `withdraw_all` of the USDC balance into the authority's USDC account, with `remaining`.
fn withdraw_usdc(
    svm: &mut LiteSVM,
    scene: &Scene,
    remaining: Vec<AccountMeta>,
) -> Result<Outcome, Failure> {
    let withdraw = marginfi::withdraw_all(
        svm,
        &MARGINFI_GROUP,
        &scene.account,
        &scene.authority.pubkey(),
        &USDC_BANK,
        &scene.usdc,
        remaining,
    );
    tx::send(
        svm,
        &scene.authority,
        &[],
        &[lending::compute_limit(), withdraw],
        &[],
    )
}

/// What `withdraw_all` returned: marginfi's share rounding can keep one base unit.
fn assert_withdrawn(svm: &LiteSVM, scene: &Scene) {
    let withdrawn = wallet::token_balance(svm, &scene.usdc);
    assert!(
        (DEPOSIT - 1..=DEPOSIT).contains(&withdrawn),
        "withdrew {withdrawn}"
    );
}

#[test]
fn withdrawing_a_sole_balance_needs_no_remaining_accounts() {
    let (mut svm, scene) = scene(false);
    withdraw_usdc(&mut svm, &scene, vec![]).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_withdrawn(&svm, &scene);
    assert_eq!(marginfi::active_banks(&svm, &scene.account), []);
}

#[test]
fn a_withdrawal_that_leaves_a_balance_needs_that_balances_bank() {
    let (mut svm, scene) = scene(true);
    let failure = withdraw_usdc(&mut svm, &scene, vec![]).unwrap_err();
    assert_eq!(
        (failure.program, failure.code),
        (marginfi::MARGINFI, Some(marginfi::INVALID_BANK_ACCOUNT)),
        "{failure:?}"
    );
    assert_eq!(wallet::token_balance(&svm, &scene.usdc), 0);

    let health = marginfi::health_accounts(&svm, &scene.account, &USDC_BANK);
    withdraw_usdc(&mut svm, &scene, health).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_withdrawn(&svm, &scene);
    assert_eq!(marginfi::active_banks(&svm, &scene.account), [SOL_BANK]);
}
