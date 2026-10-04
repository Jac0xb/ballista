//! The pump.fun snapshot holds what the pump.fun tests trade against: three live curves, one of
//! them in mayhem mode, and one graduated curve. When a refresh fails here, a coin in
//! `scripts/snapshot/manifests/pump-fun.json` has graduated or changed, and needs replacing.

use {
    ballista_protocol_tests::pump::{
        self, AVJ1, AVYG, BFX4, FEE_RECIPIENT, HJXC, PUMP, PUMP_FEES, RESERVED_FEE_RECIPIENT,
        TOKEN_2022_PROGRAM,
    },
    ballista_sdk::ASSOCIATED_TOKEN_PROGRAM_ID,
};

#[test]
fn three_live_curves_and_a_graduated_one_are_in_the_snapshot() {
    let svm = pump::svm();
    for (mint, fee_recipient) in [
        (HJXC, FEE_RECIPIENT),
        (AVYG, FEE_RECIPIENT),
        (AVJ1, RESERVED_FEE_RECIPIENT),
    ] {
        let coin = pump::coin(&svm, mint);
        let curve = pump::curve(&svm, &coin);
        assert!(
            !curve.complete,
            "{mint} has graduated: replace it in pump-fun.json"
        );
        assert_eq!(
            coin.fee_recipient, fee_recipient,
            "{mint}: mayhem mode changed"
        );
        // Enough left on the curve that no test's buys finish it.
        assert!(
            curve.real_token_reserves > 100_000_000 * pump::TOKEN,
            "{mint} is close to graduating: replace it in pump-fun.json"
        );
        for account in [coin.mint, coin.curve_token_account] {
            let owner = svm.get_account(&account).map(|account| account.owner);
            assert_eq!(owner, Some(TOKEN_2022_PROGRAM), "{mint}: {account}");
        }
        assert!(
            svm.get_account(&coin.creator_vault).is_some(),
            "{mint}: no creator vault"
        );
        assert!(
            svm.get_account(&coin.bonding_curve_v2).is_none(),
            "{mint}: its bonding-curve-v2 account exists now; add it to pump-fun.json"
        );
    }
    let graduated = pump::coin(&svm, BFX4);
    assert!(
        pump::curve(&svm, &graduated).complete,
        "{BFX4} has not graduated"
    );
    for program in [
        PUMP,
        PUMP_FEES,
        TOKEN_2022_PROGRAM,
        ASSOCIATED_TOKEN_PROGRAM_ID,
    ] {
        let executable = svm
            .get_account(&program)
            .is_some_and(|account| account.executable);
        assert!(executable, "{program} is not a program in the SVM");
    }
}
