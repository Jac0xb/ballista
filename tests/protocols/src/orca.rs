//! Orca Whirlpools for the Orca templates' tests: the snapshot they run on, and setup through
//! Orca's own instructions (built with `orca_whirlpools_client`) and Orca's own math
//! (`orca_whirlpools_core`). Nothing here writes Orca state; wallets' balances go through
//! [`crate::wallet`] (write rule 1).
//!
//! The snapshot holds two SOL/USDC pools under the main WhirlpoolsConfig. [`SOL_USDC`] (tick
//! spacing 4) is the liquid one. [`SOL_USDC_THIN`] (tick spacing 64) is thin, and one of its tick
//! arrays spans about 75% of the price, so positions near the price share one.

use {
    crate::{
        snapshot::Snapshot,
        tx::{self, Failure, Outcome},
        wallet::{fund, keypair, token_account, SOL, WSOL_MINT},
    },
    ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    orca_whirlpools_client as oc, orca_whirlpools_core as oq,
    sha2::{Digest, Sha256},
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
    std::sync::OnceLock,
};

/// The Orca snapshot, written by `scripts/snapshot/snapshot.mjs` from
/// `scripts/snapshot/manifests/orca.json`.
pub const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot-orca");

/// The Whirlpool program.
pub const WHIRLPOOL: Address = oc::WHIRLPOOL_ID;
/// SOL/USDC at tick spacing 4 and a 0.04% fee: the liquid pool.
pub const SOL_USDC: Address =
    Address::from_str_const("Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE");
/// SOL/USDC at tick spacing 64 and a 0.30% fee: a thin pool.
pub const SOL_USDC_THIN: Address =
    Address::from_str_const("HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ");
pub const USDC_MINT: Address =
    Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
/// Base units per USDC.
pub const USDC: u64 = 1_000_000;
/// SPL Memo, which Orca's v2 instructions take.
pub const MEMO_PROGRAM: Address =
    Address::from_str_const("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const TOKEN_2022_PROGRAM: Address =
    Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
/// Ticks in one tick array.
pub const TICKS_PER_ARRAY: i32 = 88;
/// Anchor's own `ConstraintHasOne`, numbered from its framework-wide error range (100–2999), not a
/// [`WHIRLPOOL`]-specific one. Whirlpools raises it when an account's `has_one` target does not
/// match, such as a position passed against a whirlpool it does not belong to.
pub const ANCHOR_CONSTRAINT_HAS_ONE: u32 = 2001;

/// The NFT metadata authority `open_position_with_token_extensions` names.
const METADATA_UPDATE_AUTHORITY: Address =
    Address::from_str_const("3axbTs2z5GBy6usVbNVoqEgZMng3vZvMnAoX29BFfwhr");
const RENT_SYSVAR: Address = Address::from_str_const("SysvarRent111111111111111111111111111111111");

/// A LiteSVM holding the Orca snapshot, with Ballista built from source. The snapshot is read and
/// checked once per test binary.
pub fn svm() -> LiteSVM {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| Snapshot::load(SNAPSHOT_DIR)).svm()
}

/// A fixed 32-byte key seed named by `label`: fixed keys keep addresses and compute units the
/// same from run to run.
pub fn seed(label: &str) -> [u8; 32] {
    let mut seed = [0; 32];
    seed.copy_from_slice(&Sha256::digest(label.as_bytes()));
    seed
}

/// A pool's addresses.
#[derive(Clone, Copy, Debug)]
pub struct Pool {
    pub address: Address,
    pub tick_spacing: u16,
    pub mint_a: Address,
    pub mint_b: Address,
    pub vault_a: Address,
    pub vault_b: Address,
}

/// A pool as the SVM holds it now.
///
/// # Panics
///
/// If `pool` is not a Whirlpool in the SVM.
pub fn whirlpool(svm: &LiteSVM, pool: &Address) -> oc::Whirlpool {
    let account = svm
        .get_account(pool)
        .unwrap_or_else(|| panic!("whirlpool {pool} is not in the SVM"));
    oc::Whirlpool::from_bytes(&account.data)
        .unwrap_or_else(|error| panic!("{pool} is not a Whirlpool: {error}"))
}

pub fn pool(svm: &LiteSVM, address: Address) -> Pool {
    let state = whirlpool(svm, &address);
    Pool {
        address,
        tick_spacing: state.tick_spacing,
        mint_a: state.token_mint_a,
        mint_b: state.token_mint_b,
        vault_a: state.token_vault_a,
        vault_b: state.token_vault_b,
    }
}

/// The tick array holding `tick`: `["tick_array", pool, start]`, where `start` is its first tick.
pub fn tick_array(pool: &Pool, tick: i32) -> Address {
    let start = oq::get_tick_array_start_tick_index(tick, pool.tick_spacing);
    oc::get_tick_array_address(&pool.address, start, None)
        .expect("a tick array address")
        .0
}

/// Creates the tick array holding `tick` with Orca's `initialize_tick_array`, unless it exists,
/// and returns its address. `payer` funds it. The snapshot holds only the arrays around each
/// pool's price.
pub fn initialize_tick_array(
    svm: &mut LiteSVM,
    pool: &Pool,
    payer: &Keypair,
    tick: i32,
) -> Address {
    let start = oq::get_tick_array_start_tick_index(tick, pool.tick_spacing);
    let address = tick_array(pool, tick);
    if svm.get_account(&address).is_none() {
        let instruction = oc::InitializeTickArray {
            whirlpool: pool.address,
            funder: payer.pubkey(),
            tick_array: address,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .instruction(oc::InitializeTickArrayInstructionArgs {
            start_tick_index: start,
        });
        tx::send(svm, payer, &[], &[instruction], &[])
            .unwrap_or_else(|failure| panic!("initialize_tick_array failed: {failure:?}"));
    }
    address
}

/// The widest range a position in `pool` can take.
pub fn full_range(pool: &Pool) -> (i32, i32) {
    let range = oq::get_full_range_tick_indexes(pool.tick_spacing);
    (range.tick_lower_index, range.tick_upper_index)
}

/// Initializable ticks `from` and `to` ticks from the current one, rounded outward.
pub fn range(svm: &LiteSVM, pool: &Pool, from: i32, to: i32) -> (i32, i32) {
    let tick = whirlpool(svm, &pool.address).tick_current_index;
    (
        oq::get_initializable_tick_index(tick + from, pool.tick_spacing, Some(false)),
        oq::get_initializable_tick_index(tick + to, pool.tick_spacing, Some(true)),
    )
}

/// A keypair with a wSOL and a USDC account: the two tokens both pools trade.
pub struct TokenWallet {
    pub keypair: Keypair,
    /// wSOL, the pools' token A.
    pub token_a: Address,
    /// USDC, the pools' token B.
    pub token_b: Address,
}

/// A wallet whose associated token accounts hold `sol` lamports of wSOL and `usdc` base units of
/// USDC (write rule 1), with 100 SOL besides for fees and rent.
pub fn token_wallet(svm: &mut LiteSVM, seed: &[u8; 32], sol: u64, usdc: u64) -> TokenWallet {
    let keypair = keypair(seed);
    fund(svm, &keypair.pubkey(), 100 * SOL);
    let token_a = token_account(svm, &keypair.pubkey(), &WSOL_MINT, sol);
    let token_b = token_account(svm, &keypair.pubkey(), &USDC_MINT, usdc);
    TokenWallet {
        keypair,
        token_a,
        token_b,
    }
}

/// An owner, funded to open and fund positions, and a trader with ten times as much, funded to
/// move the price without running dry. Seeded from `"{label} owner"` and `"{label} trader"`, so
/// callers share a label across a test's other fixed seeds.
pub fn owner_and_trader(svm: &mut LiteSVM, label: &str) -> (TokenWallet, TokenWallet) {
    let owner = token_wallet(
        svm,
        &seed(&format!("{label} owner")),
        10_000 * SOL,
        2_000_000 * USDC,
    );
    let trader = token_wallet(
        svm,
        &seed(&format!("{label} trader")),
        100_000 * SOL,
        20_000_000 * USDC,
    );
    (owner, trader)
}

/// Approves `delegate` for `amount` of whatever `token_account` holds, with Token's own
/// `approve`, signed by `owner`. For a position's NFT token account, `amount` must be exactly 1 β€”
/// Whirlpools reads a different `delegated_amount` as no approval at all (`InvalidPositionTokenAmount`,
/// 6020) β€” and this is what then lets a keeper sign as `positionAuthority` without holding the NFT
/// itself; Whirlpools refuses one that is neither the owner nor such a delegate with
/// `MissingOrInvalidDelegate` (6019).
pub fn approve_delegate(
    svm: &mut LiteSVM,
    token_account: Address,
    delegate: Address,
    amount: u64,
    owner: &Keypair,
) {
    let mut data = vec![4u8]; // SPL Token's `Approve` instruction.
    data.extend_from_slice(&amount.to_le_bytes());
    let instruction = Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(token_account, false),
            AccountMeta::new_readonly(delegate, false),
            AccountMeta::new_readonly(owner.pubkey(), true),
        ],
        data,
    };
    tx::send(svm, owner, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("approve failed: {failure:?}"));
}

/// The token program that holds a position's NFT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Nft {
    Token,
    Token2022,
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub address: Address,
    /// The owner's token account holding the position's NFT.
    pub token_account: Address,
    pub lower: i32,
    pub upper: i32,
}

/// Opens a position on `[lower, upper]` for `owner`, who pays. `mint` becomes its NFT's mint.
pub fn open_position(
    svm: &mut LiteSVM,
    pool: &Pool,
    owner: &TokenWallet,
    mint: &Keypair,
    (lower, upper): (i32, i32),
    nft: Nft,
) -> Position {
    let owner_address = owner.keypair.pubkey();
    let (position, bump) =
        oc::get_position_address(&mint.pubkey(), None).expect("a position address");
    let nft_program = match nft {
        Nft::Token => TOKEN_PROGRAM_ID,
        Nft::Token2022 => TOKEN_2022_PROGRAM,
    };
    let token_account = Address::find_program_address(
        &[
            owner_address.as_ref(),
            nft_program.as_ref(),
            mint.pubkey().as_ref(),
        ],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0;
    let instruction = match nft {
        Nft::Token => oc::OpenPosition {
            funder: owner_address,
            owner: owner_address,
            position,
            position_mint: mint.pubkey(),
            position_token_account: token_account,
            whirlpool: pool.address,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            rent: RENT_SYSVAR,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
        }
        .instruction(oc::OpenPositionInstructionArgs {
            position_bump: bump,
            tick_lower_index: lower,
            tick_upper_index: upper,
        }),
        Nft::Token2022 => oc::OpenPositionWithTokenExtensions {
            funder: owner_address,
            owner: owner_address,
            position,
            position_mint: mint.pubkey(),
            position_token_account: token_account,
            whirlpool: pool.address,
            token2022_program: TOKEN_2022_PROGRAM,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            metadata_update_auth: METADATA_UPDATE_AUTHORITY,
        }
        .instruction(oc::OpenPositionWithTokenExtensionsInstructionArgs {
            tick_lower_index: lower,
            tick_upper_index: upper,
            with_token_metadata_extension: false,
        }),
    };
    tx::send(svm, &owner.keypair, &[mint], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("open_position failed: {failure:?}"));
    Position {
        address: position,
        token_account,
        lower,
        upper,
    }
}

/// Adds `liquidity` to `position` with Orca's `increase_liquidity`, capped 1% above Orca's quote.
pub fn deposit(
    svm: &mut LiteSVM,
    pool: &Pool,
    owner: &TokenWallet,
    position: &Position,
    liquidity: u128,
) {
    let sqrt_price = whirlpool(svm, &pool.address).sqrt_price;
    let quote = oq::increase_liquidity_quote(
        liquidity,
        100,
        sqrt_price,
        position.lower,
        position.upper,
        None,
        None,
    )
    .expect("an increase_liquidity quote");
    let instruction = oc::IncreaseLiquidity {
        whirlpool: pool.address,
        token_program: TOKEN_PROGRAM_ID,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: tick_array(pool, position.lower),
        tick_array_upper: tick_array(pool, position.upper),
    }
    .instruction(oc::IncreaseLiquidityInstructionArgs {
        liquidity_amount: liquidity,
        token_max_a: quote.token_max_a,
        token_max_b: quote.token_max_b,
    });
    tx::send(svm, &owner.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("increase_liquidity failed: {failure:?}"));
}

/// Removes all of `position`'s liquidity with Orca's `decrease_liquidity`, which also records the
/// fees it had earned.
pub fn withdraw_all(svm: &mut LiteSVM, pool: &Pool, owner: &TokenWallet, position: &Position) {
    let liquidity = position_state(svm, position).liquidity;
    let instruction = oc::DecreaseLiquidity {
        whirlpool: pool.address,
        token_program: TOKEN_PROGRAM_ID,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: tick_array(pool, position.lower),
        tick_array_upper: tick_array(pool, position.upper),
    }
    .instruction(oc::DecreaseLiquidityInstructionArgs {
        liquidity_amount: liquidity,
        token_min_a: 0,
        token_min_b: 0,
    });
    tx::send(svm, &owner.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("decrease_liquidity failed: {failure:?}"));
}

/// Swaps `amount` of the input token through `pool`: token A for B when `a_to_b`. No price limit
/// and no minimum out; these swaps exist to pay fees.
pub fn swap(svm: &mut LiteSVM, pool: &Pool, trader: &TokenWallet, amount: u64, a_to_b: bool) {
    let tick = whirlpool(svm, &pool.address).tick_current_index;
    let [tick_array0, tick_array1, tick_array2] = swap_tick_arrays(pool, tick, a_to_b);
    let instruction = oc::Swap {
        token_program: TOKEN_PROGRAM_ID,
        token_authority: trader.keypair.pubkey(),
        whirlpool: pool.address,
        token_owner_account_a: trader.token_a,
        token_vault_a: pool.vault_a,
        token_owner_account_b: trader.token_b,
        token_vault_b: pool.vault_b,
        tick_array0,
        tick_array1,
        tick_array2,
        oracle: oc::get_oracle_address(&pool.address, None)
            .expect("an oracle address")
            .0,
    }
    .instruction(oc::SwapInstructionArgs {
        amount,
        other_amount_threshold: 0,
        sqrt_price_limit: 0,
        amount_specified_is_input: true,
        a_to_b,
    });
    tx::send(svm, &trader.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("swap failed: {failure:?}"));
}

/// The three tick arrays a swap from `tick` walks, laid out as Orca's `sparse_swap.rs` does: from
/// the current array downward for A to B, and upward from one tick spacing higher for B to A. An
/// array that does not exist reads as one without initialized ticks.
fn swap_tick_arrays(pool: &Pool, tick: i32, a_to_b: bool) -> [Address; 3] {
    let spacing = i32::from(pool.tick_spacing);
    let span = TICKS_PER_ARRAY * spacing;
    let (from, step) = if a_to_b {
        (tick, -span)
    } else {
        (tick + spacing, span)
    };
    let start = oq::get_tick_array_start_tick_index(from, pool.tick_spacing);
    [0, 1, 2].map(|index| {
        oc::get_tick_array_address(&pool.address, start + index * step, None)
            .expect("a tick array address")
            .0
    })
}

/// Orca's `update_fees_and_rewards` for `position`. It needs no signature; `payer` pays.
pub fn update_fees(
    svm: &mut LiteSVM,
    pool: &Pool,
    payer: &Keypair,
    position: &Position,
) -> Result<Outcome, Failure> {
    let instruction = oc::UpdateFeesAndRewards {
        whirlpool: pool.address,
        position: position.address,
        tick_array_lower: tick_array(pool, position.lower),
        tick_array_upper: tick_array(pool, position.upper),
    }
    .instruction();
    tx::send(svm, payer, &[], &[instruction], &[])
}

/// A position's liquidity and the fees it records as owed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PositionState {
    pub liquidity: u128,
    pub fee_owed_a: u64,
    pub fee_owed_b: u64,
}

/// Decodes `position`'s account as a Whirlpool position.
///
/// # Panics
///
/// If `position` does not exist in the SVM, or is not a Whirlpool position.
fn decode_position(svm: &LiteSVM, position: &Position) -> oc::Position {
    let account = svm
        .get_account(&position.address)
        .unwrap_or_else(|| panic!("position {} does not exist", position.address));
    oc::Position::from_bytes(&account.data).expect("a Whirlpool position")
}

pub fn position_state(svm: &LiteSVM, position: &Position) -> PositionState {
    let decoded = decode_position(svm, position);
    PositionState {
        liquidity: decoded.liquidity,
        fee_owed_a: decoded.fee_owed_a,
        fee_owed_b: decoded.fee_owed_b,
    }
}

/// The fees `update_fees_and_rewards` would record for `position` now, from Orca's
/// `collect_fees_quote`: `(fee_owed_a, fee_owed_b)`.
pub fn fees_owed_now(svm: &LiteSVM, pool: &Pool, position: &Position) -> (u64, u64) {
    let tick = |index: i32| -> oq::TickFacade {
        let account = svm
            .get_account(&tick_array(pool, index))
            .unwrap_or_else(|| panic!("the tick array holding tick {index} is not in the SVM"));
        let array: oq::TickArrayFacade = oc::FixedTickArray::from_bytes(&account.data)
            .expect("a fixed tick array")
            .into();
        let offset = oq::get_tick_index_in_array(index, array.start_tick_index, pool.tick_spacing)
            .expect("the tick is in its array");
        array.ticks[offset as usize]
    };
    let state = decode_position(svm, position);
    let quote = oq::collect_fees_quote(
        whirlpool(svm, &pool.address).into(),
        state.into(),
        tick(position.lower),
        tick(position.upper),
        None,
        None,
    )
    .expect("a collect_fees quote");
    (quote.fee_owed_a, quote.fee_owed_b)
}

/// The name `update_fees_and_rewards` logs on entry.
pub const UPDATE_FEES: &str = "UpdateFeesAndRewards";
/// The name `collect_fees` logs on entry.
pub const COLLECT_FEES: &str = "CollectFees";
/// How [`whirlpool_calls`] shows a call that logs no instruction name, as
/// `increase_liquidity_by_token_amounts_v2` does: it logs only its event.
pub const UNNAMED: &str = "(no name logged)";

/// The Whirlpool instructions a run invoked, in order, each by the name it logs on entry:
/// [`UPDATE_FEES`], [`COLLECT_FEES`], or [`UNNAMED`].
///
/// A run's calls follow its steps and rows in order, so the sequence says which calls each row
/// made; a count cannot. A collect of nothing moves nothing and leaves no trace in any account, so
/// this is the only place an idle row's collect would show.
pub fn whirlpool_calls(logs: &[String]) -> Vec<&str> {
    let invoked = format!("Program {WHIRLPOOL} invoke [2]");
    logs.iter()
        .enumerate()
        .filter(|(_, line)| **line == invoked)
        .map(|(at, _)| {
            logs.get(at + 1)
                .and_then(|line| line.strip_prefix("Program log: Instruction: "))
                .unwrap_or(UNNAMED)
        })
        .collect()
}

/// Asserts that Whirlpools itself refused with `error`, naming it instead of its bare code.
/// Whirlpools numbers its errors from 6000, like Ballista's own (M6), so `failure.program` is what
/// tells them apart; see [`tx::assert_ballista_failure`] for Ballista's own errors.
#[track_caller]
pub fn assert_whirlpool_error(failure: &Failure, error: oc::WhirlpoolError) {
    let code = error.clone() as u32;
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(code)),
        "expected Whirlpools' {error:?}, but {failure:?}"
    );
}
