//! `orcaHarvestManyPositions` against the real Whirlpool program: positions that earned in both
//! tokens, in one token, and not at all, and one without liquidity, harvested in one run.

use {
    ballista_protocol_tests::{
        orca::{
            self, Nft, Pool, Position, TokenWallet, COLLECT_FEES, SOL_USDC, SOL_USDC_THIN,
            UPDATE_FEES, USDC, WHIRLPOOL,
        },
        template::{examples, upload, Example, Run},
        tx::{self, ballista_error, Failure, Outcome},
        wallet::{fund, keypair, token_balance, SOL},
    },
    ballista_sdk::{decode_ballista_error, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    orca_whirlpools_client as oc,
    solana_address::Address,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
    solana_transaction::{InstructionError, TransactionError},
};

const EXAMPLE: &str = "orcaHarvestManyPositions";

/// The thin pool, an owner, a trader and the template. Rows share one tick array there.
struct Setup {
    svm: LiteSVM,
    pool: Pool,
    /// The pool's in-range liquidity as snapshotted.
    pool_liquidity: u128,
    owner: TokenWallet,
    trader: TokenWallet,
    template: Address,
}

fn setup(example: &Example) -> Setup {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC_THIN);
    let (owner, trader) = orca::owner_and_trader(&mut svm, "harvest");
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = upload(&mut svm, &creator, 1, &example.payload);
    let pool_liquidity = orca::whirlpool(&svm, &SOL_USDC_THIN).liquidity;
    Setup {
        svm,
        pool,
        pool_liquidity,
        owner,
        trader,
        template,
    }
}

impl Setup {
    /// A position of the owner's on `range`, holding `liquidity`.
    fn position(&mut self, label: &str, range: (i32, i32), nft: Nft, liquidity: u128) -> Position {
        let mint = keypair(&orca::seed(label));
        let position =
            orca::open_position(&mut self.svm, &self.pool, &self.owner, &mint, range, nft);
        if liquidity > 0 {
            orca::deposit(&mut self.svm, &self.pool, &self.owner, &position, liquidity);
        }
        position
    }

    fn balances(&self) -> (u64, u64) {
        (
            token_balance(&self.svm, &self.owner.token_a),
            token_balance(&self.svm, &self.owner.token_b),
        )
    }
}

/// The harvest over `rows`, each paired with the pool it belongs to, signed by `authority`, paying
/// fees to `fee_accounts` — ordinarily the owner's own token accounts ([`harvest`]), but a security
/// test points them at a stranger's instead.
fn harvest_paying(
    setup: &Setup,
    example: &Example,
    authority: &Keypair,
    rows: &[(&Position, &Pool)],
    dust_floor: u64,
    fee_accounts: (Address, Address),
) -> Instruction {
    let (token_owner_account_a, token_owner_account_b) = fee_accounts;
    let mut run = Run::new(setup.template, example)
        .account("whirlpoolProgram", WHIRLPOOL, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("positionAuthority", authority.pubkey(), false, true)
        .account("whirlpool", setup.pool.address, true, false)
        .account("tokenOwnerAccountA", token_owner_account_a, true, false)
        .account("tokenOwnerAccountB", token_owner_account_b, true, false)
        .account("tokenVaultA", setup.pool.vault_a, true, false)
        .account("tokenVaultB", setup.pool.vault_b, true, false)
        .input_u64("dustFloor", dust_floor);
    for (position, pool) in rows {
        run = run.row(|row| {
            row.account("position", position.address, true, false)
                .account("positionTokenAccount", position.token_account, false, false)
                .account(
                    "tickArrayLower",
                    orca::tick_array(pool, position.lower),
                    false,
                    false,
                )
                .account(
                    "tickArrayUpper",
                    orca::tick_array(pool, position.upper),
                    false,
                    false,
                )
        });
    }
    run.build()
}

/// [`harvest_paying`], paying fees to the setup's own owner.
fn harvest(
    setup: &Setup,
    example: &Example,
    authority: &Keypair,
    rows: &[(&Position, &Pool)],
    dust_floor: u64,
) -> Instruction {
    let fee_accounts = (setup.owner.token_a, setup.owner.token_b);
    harvest_paying(setup, example, authority, rows, dust_floor, fee_accounts)
}

fn send(
    setup: &mut Setup,
    signer: &Keypair,
    instructions: &[Instruction],
) -> Result<Outcome, Failure> {
    tx::send(&mut setup.svm, signer, &[], instructions, &[])
}

/// Two earning rows (one in both tokens, one only in B and held as a Token-2022 NFT), one out of
/// range, one without liquidity.
struct Rows {
    both: Position,
    only_b: Position,
    out_of_range: Position,
    empty: Position,
}

fn four_rows(setup: &mut Setup) -> Rows {
    let spacing = i32::from(setup.pool.tick_spacing);
    let around = orca::range(&setup.svm, &setup.pool, -10 * spacing, 10 * spacing);
    let above = orca::range(&setup.svm, &setup.pool, 20 * spacing, 30 * spacing);
    // Ten times the pool's own liquidity, so these rows earn most of each fee.
    let liquidity = 10 * setup.pool_liquidity;
    let both = setup.position("harvest both", around, Nft::Token, liquidity);
    let out_of_range = setup.position("harvest above", above, Nft::Token, liquidity);
    let empty = setup.position("harvest empty", around, Nft::Token, 0);
    // Sold SOL pays fees in token A to the positions in range now.
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 20 * SOL, true);
    let only_b = setup.position("harvest only b", around, Nft::Token2022, liquidity);
    // Sold USDC pays fees in token B, to `only_b` too.
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        2_000 * USDC,
        false,
    );
    Rows {
        both,
        only_b,
        out_of_range,
        empty,
    }
}

#[test]
fn only_the_rows_that_earned_collect() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows {
        both,
        only_b,
        out_of_range,
        empty,
    } = four_rows(&mut setup);
    let earned_both = orca::fees_owed_now(&setup.svm, &setup.pool, &both);
    let earned_b = orca::fees_owed_now(&setup.svm, &setup.pool, &only_b);
    assert!(earned_both.0 > 0 && earned_both.1 > 0, "{earned_both:?}");
    assert!(earned_b.0 == 0 && earned_b.1 > 0, "{earned_b:?}");
    assert_eq!(
        orca::fees_owed_now(&setup.svm, &setup.pool, &out_of_range),
        (0, 0)
    );
    let untouched =
        [out_of_range, empty].map(|position| orca::position_state(&setup.svm, &position));
    let before = setup.balances();

    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let run = harvest(
        &setup,
        example,
        &owner,
        &[
            (&both, &pool),
            (&only_b, &pool),
            (&out_of_range, &pool),
            (&empty, &pool),
        ],
        0,
    );
    let outcome = send(&mut setup, &owner, &[run]).unwrap_or_else(|failure| panic!("{failure:?}"));

    let after = setup.balances();
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        (earned_both.0 + earned_b.0, earned_both.1 + earned_b.1)
    );
    for position in [both, only_b] {
        let state = orca::position_state(&setup.svm, &position);
        assert_eq!((state.fee_owed_a, state.fee_owed_b), (0, 0));
    }
    assert_eq!(
        [out_of_range, empty].map(|position| orca::position_state(&setup.svm, &position)),
        untouched
    );
    // Row by row: `both` and `only_b` update and collect, `out_of_range` only updates, and
    // `empty`, without liquidity, does neither. Every row with liquidity updates first, so a
    // collect by an idle row would show here. It would leave no trace in any account.
    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [
            UPDATE_FEES,
            COLLECT_FEES,
            UPDATE_FEES,
            COLLECT_FEES,
            UPDATE_FEES
        ]
    );
    println!(
        "four rows: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}

/// `dustFloor` is compared with each row's own fees. `only_b` earned one fee, and at a floor of
/// exactly that fee it is dust: the row updates, which records the fee, and leaves it owed for a
/// later harvest. `both` earned more than the floor in token A, so it collects both its fees.
#[test]
fn a_row_whose_fees_are_at_the_floor_is_left_for_later() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, only_b, .. } = four_rows(&mut setup);
    let earned_both = orca::fees_owed_now(&setup.svm, &setup.pool, &both);
    let earned_b = orca::fees_owed_now(&setup.svm, &setup.pool, &only_b);
    let floor = earned_b.1;
    assert!(
        earned_b.0 == 0 && earned_both.0 > floor,
        "{earned_both:?} {earned_b:?}"
    );
    let before = setup.balances();

    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let run = harvest(
        &setup,
        example,
        &owner,
        &[(&both, &pool), (&only_b, &pool)],
        floor,
    );
    let outcome = send(&mut setup, &owner, &[run]).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES, UPDATE_FEES]
    );
    let after = setup.balances();
    assert_eq!((after.0 - before.0, after.1 - before.1), earned_both);
    let collected = orca::position_state(&setup.svm, &both);
    assert_eq!((collected.fee_owed_a, collected.fee_owed_b), (0, 0));
    let kept = orca::position_state(&setup.svm, &only_b);
    assert_eq!((kept.fee_owed_a, kept.fee_owed_b), earned_b);
}

/// M6: Whirlpools numbers its errors from 6000, as Ballista does, so only the logs say who refused.
#[test]
fn a_stranger_cannot_collect_and_whirlpools_says_so() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, .. } = four_rows(&mut setup);
    let stranger = keypair(&orca::seed("harvest stranger"));
    fund(&mut setup.svm, &stranger.pubkey(), SOL);

    let pool = setup.pool;
    // The fees still go to the real holder's own accounts: `positionBelongsToTheFeeOwner` passes
    // (the position belongs to its holder, not to whoever signs), so Whirlpools' own delegate
    // check on `positionAuthority` is what's on trial.
    let run = harvest(&setup, example, &stranger, &[(&both, &pool)], 0);
    let failure = send(&mut setup, &stranger, &[run]).unwrap_err();

    orca::assert_whirlpool_error(&failure, oc::WhirlpoolError::MissingOrInvalidDelegate);
    assert_eq!(ballista_error(&failure), None);
    // The same number, read as Ballista's, names an unrelated failure.
    assert_eq!(
        decode_ballista_error(6019).map(|decoded| decoded.name),
        Some("ReturnDataMismatch")
    );
}

/// M5: skipping rows does not keep a harvest from reverting; a row Whirlpools refuses reverts all.
#[test]
fn a_row_from_another_pool_reverts_the_whole_harvest() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, .. } = four_rows(&mut setup);
    let other_pool = orca::pool(&setup.svm, SOL_USDC);
    let mint = keypair(&orca::seed("harvest foreign"));
    let range = orca::range(&setup.svm, &other_pool, -40, 40);
    let foreign = orca::open_position(
        &mut setup.svm,
        &other_pool,
        &setup.owner,
        &mint,
        range,
        Nft::Token,
    );
    orca::deposit(
        &mut setup.svm,
        &other_pool,
        &setup.owner,
        &foreign,
        1_000_000_000,
    );
    let before = setup.balances();

    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let run = harvest(
        &setup,
        example,
        &owner,
        &[(&both, &pool), (&foreign, &other_pool)],
        0,
    );
    let failure = send(&mut setup, &owner, &[run]).unwrap_err();

    // Anchor's ConstraintHasOne: the position's whirlpool is not the run's.
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(orca::ANCHOR_CONSTRAINT_HAS_ONE)),
        "{failure:?}"
    );
    assert_eq!(
        setup.balances(),
        before,
        "the first row's collect reverted too"
    );
}

/// Ten earning rows fit one legacy transaction, with a compute budget; eleven do not.
#[test]
fn ten_rows_fit_one_legacy_transaction() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let spacing = i32::from(setup.pool.tick_spacing);
    let around = orca::range(&setup.svm, &setup.pool, -10 * spacing, 10 * spacing);
    let liquidity = setup.pool_liquidity;
    let positions: Vec<Position> = (0..11)
        .map(|index| {
            setup.position(
                &format!("harvest many {index}"),
                around,
                Nft::Token,
                liquidity,
            )
        })
        .collect();
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 20 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        2_000 * USDC,
        false,
    );
    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let rows: Vec<(&Position, &Pool)> =
        positions.iter().map(|position| (position, &pool)).collect();
    let budget = ComputeBudgetInstruction::set_compute_unit_limit(400_000);

    let eleven = harvest(&setup, example, &owner, &rows, 0);
    let transaction = tx::transaction(&setup.svm, &owner, &[], &[budget.clone(), eleven], &[]);
    assert!(
        tx::wire_size(&transaction) > tx::PACKET_DATA_SIZE,
        "eleven rows now fit: update the header"
    );

    let ten = harvest(&setup, example, &owner, &rows[..10], 0);
    let outcome =
        send(&mut setup, &owner, &[budget, ten]).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES].repeat(10)
    );
    println!(
        "ten rows: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}

/// Harvests `both`'s one row with a stranger's own accounts in the slots `stranger_in` marks (A,
/// B), signed by the real owner, and asserts the run fails at `positionBelongsToTheFeeOwner`.
fn assert_fees_must_go_to_the_position_holder(stranger_in: (bool, bool)) {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, .. } = four_rows(&mut setup);
    let stranger = orca::token_wallet(
        &mut setup.svm,
        &orca::seed("harvest stranger accounts"),
        0,
        0,
    );
    let (stranger_in_a, stranger_in_b) = stranger_in;
    let fee_accounts = (
        if stranger_in_a {
            stranger.token_a
        } else {
            setup.owner.token_a
        },
        if stranger_in_b {
            stranger.token_b
        } else {
            setup.owner.token_b
        },
    );

    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let run = harvest_paying(&setup, example, &owner, &[(&both, &pool)], 0, fee_accounts);
    let failure = send(&mut setup, &owner, &[run]).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "positionBelongsToTheFeeOwner");
}

/// Security: the fixed `tokenOwnerAccountA`/`B` must belong to each row's real holder (its
/// position NFT's owner), not to `positionAuthority`, which Whirlpools lets be a delegate instead.
/// Whirlpools' `collect_fees` checks only the fee accounts' mint, never who owns them, so nothing
/// else stops a run an untrusted builder assembled from paying a stranger every row's fees while
/// the true holder just signs.
#[test]
fn fees_must_go_to_the_position_holder_not_a_strangers_accounts() {
    assert_fees_must_go_to_the_position_holder((true, true));
}

/// The same check on token B alone: with the stranger only in `tokenOwnerAccountB`, token A's own
/// comparison passes and it is the B comparison that must still fail the run. Deleting the B check
/// (or comparing it against the wrong account) would otherwise let this land.
#[test]
fn fees_must_go_to_the_position_holder_in_token_b_alone() {
    assert_fees_must_go_to_the_position_holder((false, true));
}

/// A delegate keeper can sign as `positionAuthority` for a row's position it does not hold β€”
/// Whirlpools accepts a delegate approved on that row's `positionTokenAccount` in the holder's
/// place, refusing one that is neither with `MissingOrInvalidDelegate` (6019) β€” and the run still
/// lands with the fees going to the real holder's own accounts, not the delegate's: proof that
/// `positionBelongsToTheFeeOwner` binds to the holder, not to whoever signs. Harvest never debits
/// the fee accounts (only `collect_fees`' destinations, which need no authority from their own
/// owner), so unlike compound's version of this test, no further delegation is needed for the run
/// to land. The approval is Token's own `approve`, sent as a real signed transaction; nothing here
/// is a direct write beyond write rule 1's wallet balances.
#[test]
fn a_delegates_signature_still_pays_the_position_holder() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, .. } = four_rows(&mut setup);
    let earned = orca::fees_owed_now(&setup.svm, &setup.pool, &both);
    assert!(earned.0 > 0 && earned.1 > 0, "{earned:?}");
    let before = setup.balances();

    let delegate = keypair(&orca::seed("harvest delegate"));
    fund(&mut setup.svm, &delegate.pubkey(), SOL);
    orca::approve_delegate(
        &mut setup.svm,
        both.token_account,
        delegate.pubkey(),
        1,
        &setup.owner.keypair,
    );

    let pool = setup.pool;
    let run = harvest(&setup, example, &delegate, &[(&both, &pool)], 0);
    let outcome =
        send(&mut setup, &delegate, &[run]).unwrap_or_else(|failure| panic!("{failure:?}"));

    let after = setup.balances();
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        earned,
        "the holder's own accounts grew"
    );
    println!(
        "delegate-signed harvest: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}

/// Eight earning rows land under the default 200,000-CU limit with no compute-budget instruction;
/// a ninth exhausts it mid-CPI. That is not a custom program error, so it carries no code.
#[test]
fn eight_rows_fit_the_default_compute_limit() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let spacing = i32::from(setup.pool.tick_spacing);
    let around = orca::range(&setup.svm, &setup.pool, -10 * spacing, 10 * spacing);
    let liquidity = setup.pool_liquidity;
    let positions: Vec<Position> = (0..9)
        .map(|index| {
            setup.position(
                &format!("harvest limit {index}"),
                around,
                Nft::Token,
                liquidity,
            )
        })
        .collect();
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 20 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        2_000 * USDC,
        false,
    );
    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let rows: Vec<(&Position, &Pool)> =
        positions.iter().map(|position| (position, &pool)).collect();

    // Nine rows, unbudgeted: a failed transaction changes no account, so the same positions are
    // still fresh for the eight-row attempt below.
    let nine = harvest(&setup, example, &owner, &rows, 0);
    let failure = send(&mut setup, &owner, &[nine]).unwrap_err();
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, None),
        "compute exhaustion mid-CPI carries no custom code: {failure:?}"
    );
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(0, InstructionError::ProgramFailedToComplete),
        "expected the ninth row to exhaust the default compute limit: {failure:?}"
    );
    // `ProgramFailedToComplete` also covers other VM faults (a panic, an illegal instruction); the
    // log line is what actually says the compute meter, not something else, is what stopped it.
    assert!(
        failure
            .logs
            .iter()
            .any(|line| line.contains("exceeded CUs meter")),
        "expected the log to blame the compute meter: {failure:?}"
    );

    let eight = harvest(&setup, example, &owner, &rows[..8], 0);
    let outcome =
        send(&mut setup, &owner, &[eight]).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES].repeat(8)
    );
    println!(
        "eight rows, no budget: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}
