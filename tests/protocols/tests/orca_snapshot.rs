//! The Orca snapshot holds what the Orca tests trade against, and each pool's price is still
//! inside the tick arrays it took. When a refresh fails here, the manifest's tick arrays have to
//! follow the price.

use {
    ballista_protocol_tests::orca::{self, SOL_USDC, SOL_USDC_THIN, TICKS_PER_ARRAY},
    ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID},
    orca_whirlpools_client as oc,
    solana_clock::Clock,
};

#[test]
fn both_pools_and_the_tick_arrays_around_their_prices_are_in_the_snapshot() {
    let svm = orca::svm();
    let now = u64::try_from(svm.get_sysvar::<Clock>().unix_timestamp).unwrap();
    for address in [SOL_USDC, SOL_USDC_THIN] {
        let pool = orca::pool(&svm, address);
        let state = orca::whirlpool(&svm, &address);
        // Orca refuses a clock older than the pool's last update: InvalidTimestamp, 6022.
        assert!(
            now >= state.reward_last_updated_timestamp,
            "{address}: the clock is behind the pool"
        );
        let span = TICKS_PER_ARRAY * i32::from(pool.tick_spacing);
        for offset in [-span, 0, span] {
            let tick = state.tick_current_index + offset;
            let array = orca::tick_array(&pool, tick);
            assert!(
                svm.get_account(&array).is_some(),
                "{address} is at tick {}, and the snapshot lacks tick array {array}, which holds \
                 tick {tick}: add it to scripts/snapshot/manifests/orca.json and refresh",
                state.tick_current_index
            );
        }
        // Neither pool has an adaptive-fee Oracle. Swaps name its address, which must stay empty.
        let (oracle, _) = oc::get_oracle_address(&address, None).unwrap();
        assert!(
            svm.get_account(&oracle).is_none(),
            "{address} has an Oracle now"
        );
        for token in [pool.mint_a, pool.vault_a, pool.mint_b, pool.vault_b] {
            let owner = svm.get_account(&token).map(|account| account.owner);
            assert_eq!(owner, Some(TOKEN_PROGRAM_ID), "{address}: {token}");
        }
    }
    for program in [
        orca::WHIRLPOOL,
        TOKEN_PROGRAM_ID,
        orca::TOKEN_2022_PROGRAM,
        ASSOCIATED_TOKEN_PROGRAM_ID,
        orca::MEMO_PROGRAM,
    ] {
        let executable = svm
            .get_account(&program)
            .is_some_and(|account| account.executable);
        assert!(executable, "{program} is not a program in the SVM");
    }
}
