//! What Orca's own instructions do, which the Orca templates' fixes rest on. These call Whirlpools
//! directly, without Ballista: if Orca changes any of it, the templates' tests fail too, and these
//! say why.

use {
    ballista_protocol_tests::{
        orca::{self, Nft, Pool, Position, TokenWallet, SOL_USDC, USDC},
        tx,
        wallet::{keypair, token_account, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    orca_whirlpools_client as oc, orca_whirlpools_core as oq,
    solana_signer::Signer,
};

/// A position 60 ticks either side of the price, holding a quarter of the pool's liquidity.
fn funded_position(label: &str) -> (LiteSVM, Pool, TokenWallet, TokenWallet, Position) {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC);
    let (owner, trader) = orca::owner_and_trader(&mut svm, label);
    let mint = keypair(&orca::seed(&format!("{label} position")));
    let range = orca::range(&svm, &pool, -60, 60);
    let position = orca::open_position(&mut svm, &pool, &owner, &mint, range, Nft::Token);
    let liquidity = orca::whirlpool(&svm, &SOL_USDC).liquidity / 4;
    orca::deposit(&mut svm, &pool, &owner, &position, liquidity);
    (svm, pool, owner, trader, position)
}

/// The root of M2: `fee_owed_*` is what the last update recorded, not what swaps have earned since.
#[test]
fn fees_owed_rise_only_when_the_position_is_updated() {
    let (mut svm, pool, _, trader, position) = funded_position("setup stale");
    orca::swap(&mut svm, &pool, &trader, 200 * SOL, true);
    orca::swap(&mut svm, &pool, &trader, 30_000 * USDC, false);

    let earned = orca::fees_owed_now(&svm, &pool, &position);
    assert!(earned.0 > 0 && earned.1 > 0, "{earned:?}");
    let stale = orca::position_state(&svm, &position);
    assert_eq!((stale.fee_owed_a, stale.fee_owed_b), (0, 0));
    // Permissionless: the trader updates the owner's position.
    orca::update_fees(&mut svm, &pool, &trader.keypair, &position).unwrap();
    let updated = orca::position_state(&svm, &position);
    assert_eq!((updated.fee_owed_a, updated.fee_owed_b), earned);
}

/// The root of M5: an empty collect is not refused, so skipping it saves compute, not a revert.
#[test]
fn collecting_with_nothing_owed_succeeds_and_moves_nothing() {
    let (mut svm, pool, owner, _, position) = funded_position("setup empty collect");
    let collect = oc::CollectFees {
        whirlpool: pool.address,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_vault_a: pool.vault_a,
        token_owner_account_b: owner.token_b,
        token_vault_b: pool.vault_b,
        token_program: TOKEN_PROGRAM_ID,
    }
    .instruction();
    let before = (
        token_balance(&svm, &owner.token_a),
        token_balance(&svm, &owner.token_b),
    );
    tx::send(&mut svm, &owner.keypair, &[], &[collect], &[]).unwrap();
    let after = (
        token_balance(&svm, &owner.token_a),
        token_balance(&svm, &owner.token_b),
    );
    assert_eq!(after, before);
}

/// Why the templates skip the update for a position without liquidity.
#[test]
fn a_position_without_liquidity_cannot_be_updated() {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC);
    let owner = orca::token_wallet(&mut svm, &orca::seed("setup empty owner"), SOL, USDC);
    let mint = keypair(&orca::seed("setup empty position"));
    let range = orca::range(&svm, &pool, -60, 60);
    let position = orca::open_position(&mut svm, &pool, &owner, &mint, range, Nft::Token);

    let failure = orca::update_fees(&mut svm, &pool, &owner.keypair, &position).unwrap_err();
    orca::assert_whirlpool_error(&failure, oc::WhirlpoolError::LiquidityZero);
}

/// The root of M4: inside its range, a position takes liquidity only in both tokens. With one cap
/// at zero, `increase_liquidity` fails whatever liquidity it is asked for, and
/// `increase_liquidity_by_token_amounts_v2` works out zero liquidity and fails.
#[test]
fn in_range_liquidity_needs_both_tokens() {
    let (mut svm, pool, owner, _, position) = funded_position("setup one-sided");
    let by_liquidity = oc::IncreaseLiquidity {
        whirlpool: pool.address,
        token_program: TOKEN_PROGRAM_ID,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: orca::tick_array(&pool, position.lower),
        tick_array_upper: orca::tick_array(&pool, position.upper),
    }
    .instruction(oc::IncreaseLiquidityInstructionArgs {
        liquidity_amount: 1_000_000,
        token_max_a: SOL,
        token_max_b: 0,
    });
    let failure = tx::send(&mut svm, &owner.keypair, &[], &[by_liquidity], &[]).unwrap_err();
    orca::assert_whirlpool_error(&failure, oc::WhirlpoolError::TokenMaxExceeded);

    let by_amounts = oc::IncreaseLiquidityByTokenAmountsV2 {
        whirlpool: pool.address,
        token_program_a: TOKEN_PROGRAM_ID,
        token_program_b: TOKEN_PROGRAM_ID,
        memo_program: orca::MEMO_PROGRAM,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_mint_a: pool.mint_a,
        token_mint_b: pool.mint_b,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: orca::tick_array(&pool, position.lower),
        tick_array_upper: orca::tick_array(&pool, position.upper),
    }
    .instruction(oc::IncreaseLiquidityByTokenAmountsV2InstructionArgs {
        method: oc::IncreaseLiquidityMethod::ByTokenAmounts {
            token_max_a: SOL,
            token_max_b: 0,
            min_sqrt_price: oq::MIN_SQRT_PRICE,
            max_sqrt_price: oq::MAX_SQRT_PRICE,
        },
        remaining_accounts_info: None,
    });
    let failure = tx::send(&mut svm, &owner.keypair, &[], &[by_amounts], &[]).unwrap_err();
    orca::assert_whirlpool_error(&failure, oc::WhirlpoolError::LiquidityZero);
}

/// The root of M7's trust in the templates' unpinned read: both read `positionTokenAccount`'s
/// owner field without pinning its own mint or amount, on the strength of Orca's own `collect_fees`
/// refusing one that is not genuinely the position's NFT account. An empty account for the right
/// mint and an account for the wrong mint both fail the same way: Anchor's generic
/// `constraint = ...` failure, `ConstraintRaw`, not a named check.
///
/// Whirlpools also refuses one held by another program outright (`AccountOwnedByWrongProgram`,
/// 3007) even when its mint, owner and amount are exactly right, but building that account needs a
/// direct write to fake its owning program, which this suite forbids (see `findings/orca.md`); only
/// the two legitimately-built cases above are pinned here.
#[test]
fn collect_fees_refuses_a_position_token_account_that_does_not_hold_the_nft() {
    let (mut svm, pool, owner, _, position) = funded_position("setup fake token account");
    let attacker = orca::token_wallet(
        &mut svm,
        &orca::seed("setup fake token account attacker"),
        0,
        0,
    );
    // A real ATA the attacker holds, for the position's own mint, but empty.
    let mint = orca::position_mint(&svm, &position);
    let empty_ata = token_account(&mut svm, &attacker.keypair.pubkey(), &mint, 0);

    for (label, fake) in [
        ("an empty ATA for the position's own mint", empty_ata),
        ("the attacker's own USDC account", attacker.token_b),
    ] {
        let collect = oc::CollectFees {
            whirlpool: pool.address,
            position_authority: owner.keypair.pubkey(),
            position: position.address,
            position_token_account: fake,
            token_owner_account_a: owner.token_a,
            token_vault_a: pool.vault_a,
            token_owner_account_b: owner.token_b,
            token_vault_b: pool.vault_b,
            token_program: TOKEN_PROGRAM_ID,
        }
        .instruction();
        let failure = tx::send(&mut svm, &owner.keypair, &[], &[collect], &[]).unwrap_err();
        assert_eq!(
            (failure.program, failure.code),
            (orca::WHIRLPOOL, Some(orca::ANCHOR_CONSTRAINT_RAW)),
            "{label}: {failure:?}"
        );
    }
}
