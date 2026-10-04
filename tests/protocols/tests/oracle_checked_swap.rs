//! `jupiterOracleCheckedSwap` against the real programs: route `solToUsdc`, 1 SOL for USDC through
//! Meteora DLMM, valued at the snapshot's Pyth SOL/USD price.
//!
//! The feed, the two mints it prices and the tolerance are the template's constants, and it reads
//! the feed's exponent and both mints' decimals itself, so a run passes only the price account and
//! the route. The route goes in parts, its plan and the four numbers after it, so that the template
//! can require `in_amount` to leave the account it measures.
//!
//! The run takes `route`'s place in Jupiter's own transaction: Jupiter's setup wraps the SOL and
//! creates the USDC account before it, and its cleanup closes the wrapped SOL account after.

use {
    ballista_protocol_tests::{
        oracle::{copy_pyth_feed, pyth_price, set_pyth_price, PythPrice, USDC_USD_FEED_ID},
        snapshot::{
            fee_at, jupiter_ran, Leg, Routing, Snapshot, PLATFORM_FEE_ACCOUNT, ROUTE_HEAD,
            SNAPSHOT_DIR,
        },
        template::{examples, scenarios, upload, Example, Run},
        tx::{self, assert_requirement_failed, Failure, Outcome},
        wallet::{
            self, associated_token_address, fund, holding, keypair, token_account, token_balance,
            SOL, TOKEN_ACCOUNT_LEN, WSOL_MINT,
        },
    },
    ballista_sdk::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const TEMPLATE: &str = "jupiterOracleCheckedSwap";
const ROUTE: &str = "solToUsdc";
const TEMPLATE_ID: u16 = 5;
/// The template's `TOLERANCE_BPS`, 1%: how far below the oracle's valuation the fill may land.
/// `the_floor_is_exact_to_the_last_unit` holds the template to it.
const TOLERANCE_BPS: u64 = 100;
/// SPL Token `Mint`: `decimals` is the byte at 44.
const MINT_DECIMALS: usize = 44;
/// `route`'s own accounts, before the steps': the four the template passes, then the destination
/// token account, the destination mint, the platform fee account, the event authority and the
/// program.
const ROUTE_FIXED_ACCOUNTS: usize = 9;
/// 1%: the platform fee `jupiter_takes_exactly_in_amount_through_a_split_or_a_platform_fee` sends
/// `route` with directly.
const PLATFORM_FEE_BPS: u8 = 100;
/// 1.5%: a platform fee that still fits `HOSTILE_SLIPPAGE_BPS` but not `TOLERANCE_BPS`, which
/// failed at `fillBeatTheOracle` before the cap; see
/// `a_hostile_platform_fee_fails_at_platform_fee_within_cap`.
const EXCESSIVE_FEE_BPS: u8 = 150;
/// 2%: `slippageBps` loose enough that neither `PLATFORM_FEE_BPS` nor `EXCESSIVE_FEE_BPS` trips
/// Jupiter's own slippage check.
const HOSTILE_SLIPPAGE_BPS: u16 = 200;

/// A fresh SVM with the template uploaded and the trader holding SOL, which Jupiter's setup wraps.
struct Swap {
    svm: LiteSVM,
    trader: Keypair,
    template: Address,
    leg: Leg,
    jupiter: Address,
    /// The snapshot's SOL/USD price update.
    sol_usd: Address,
}

impl Swap {
    fn new(snapshot: &Snapshot, example: &Example) -> Swap {
        let mut svm = snapshot.svm();
        let trader = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [trader.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        let leg = snapshot.route(ROUTE).legs[0].clone();
        let head: Vec<Address> = leg.instructions.swap.accounts[..ROUTE_HEAD]
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(
            head,
            [
                TOKEN_PROGRAM_ID,
                trader.pubkey(),
                leg.source_token_account,
                leg.destination_token_account
            ]
        );
        Swap {
            svm,
            trader,
            template,
            leg,
            jupiter: snapshot.named("jupiter"),
            sol_usd: snapshot.named("pythSolUsd"),
        }
    }

    /// The run, valued at the price in `price_update`, with `routing`'s accounts and
    /// `slippage_bps` and `platform_fee_bps` in place of the quote's own.
    fn run_priced(
        &self,
        example: &Example,
        price_update: Address,
        (slippage_bps, platform_fee_bps): (u64, u64),
        routing: Routing,
    ) -> Instruction {
        let leg = &self.leg;
        Run::new(self.template, example)
            .account("jupiter", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("priceUpdate", price_update, false, false)
            .account("trader", self.trader.pubkey(), true, true)
            .account("sourceAta", routing.source, true, false)
            .account("destinationAta", routing.destination, true, false)
            .account("sourceMint", leg.input_mint, false, false)
            .account("destinationMint", leg.output_mint, false, false)
            .input_bytes("routePlan", &leg.route.route_plan)
            .input_u64("inAmount", leg.route.in_amount)
            .input_u64("quotedOutAmount", leg.route.quoted_out_amount)
            .input_u64("slippageBps", slippage_bps)
            .input_u64("platformFeeBps", platform_fee_bps)
            .group("routeAccounts", routing.steps)
            .build()
    }

    /// The quote's own `slippageBps` and `platformFeeBps`.
    fn quoted_fees(&self) -> (u64, u64) {
        let route = &self.leg.route;
        (
            u64::from(route.slippage_bps),
            u64::from(route.platform_fee_bps),
        )
    }

    /// The run, valued at the price in `price_update`.
    fn run(&self, example: &Example, price_update: Address) -> Instruction {
        self.run_priced(
            example,
            price_update,
            self.quoted_fees(),
            Routing::of(&self.leg),
        )
    }

    /// The run at the SOL/USD price update, as a trader selling SOL would send it.
    fn sol_usd_run(&self, example: &Example) -> Instruction {
        self.run(example, self.sol_usd)
    }

    /// [`Swap::sol_usd_run`] with `routing`'s accounts in place of the route's own.
    fn sol_usd_run_routed(&self, example: &Example, routing: Routing) -> Instruction {
        self.run_priced(example, self.sol_usd, self.quoted_fees(), routing)
    }

    /// [`Swap::sol_usd_run_routed`] with `slippage_bps` and `platform_fee_bps` in place of the
    /// quote's own.
    fn sol_usd_run_priced(
        &self,
        example: &Example,
        slippage_bps: u64,
        platform_fee_bps: u64,
        routing: Routing,
    ) -> Instruction {
        self.run_priced(
            example,
            self.sol_usd,
            (slippage_bps, platform_fee_bps),
            routing,
        )
    }

    /// Sends `run` in `route`'s place, between Jupiter's own setup and cleanup.
    fn send(&mut self, run: Instruction) -> Result<Outcome, Failure> {
        let instructions = self.leg.instructions.with_swap(run);
        tx::send(
            &mut self.svm,
            &self.trader,
            &[],
            &instructions,
            &self.leg.lookup_tables,
        )
    }

    /// The USDC the trader holds: after a run that landed, the fill.
    fn usdc(&self) -> u64 {
        token_balance(&self.svm, &self.leg.destination_token_account)
    }

    /// The power of ten the template scales `sold × price` by: the destination's decimals plus the
    /// feed's exponent less the source's decimals, each read from the SVM as the template reads
    /// them. SOL has 9, USDC 6, and SOL/USD's exponent is −8, so −11.
    fn scale(&self, price_update: &Address) -> i32 {
        let decimals =
            |mint: &Address| i32::from(self.svm.get_account(mint).unwrap().data[MINT_DECIMALS]);
        decimals(&self.leg.output_mint) + pyth_price(&self.svm, price_update).exponent
            - decimals(&self.leg.input_mint)
    }
}

/// Makes another token account of `owner`'s for `mint`, at `account`'s address rather than the
/// associated one, the way a wallet would: the System program creates it with its rent and
/// `amount` lamports more, and the Token program initializes it. A wrapped SOL account counts the
/// extra lamports as its balance, so only wrapped SOL may start with one.
fn another_token_account(
    svm: &mut LiteSVM,
    owner: &Keypair,
    account: &Keypair,
    mint: &Address,
    amount: u64,
) -> Address {
    assert!(
        amount == 0 || *mint == WSOL_MINT,
        "only wrapped SOL can be funded without the mint's authority"
    );
    let address = account.pubkey();
    let rent = svm.minimum_balance_for_rent_exemption(TOKEN_ACCOUNT_LEN);
    // SystemInstruction::CreateAccount: tag 0 as a u32, then lamports, space and owner.
    let mut create = vec![0, 0, 0, 0];
    create.extend_from_slice(&(rent + amount).to_le_bytes());
    create.extend_from_slice(&u64::try_from(TOKEN_ACCOUNT_LEN).unwrap().to_le_bytes());
    create.extend_from_slice(TOKEN_PROGRAM_ID.as_ref());
    // InitializeAccount3: tag 18, then the owner.
    let mut initialize = vec![18];
    initialize.extend_from_slice(owner.pubkey().as_ref());
    let instructions = [
        Instruction {
            program_id: SYSTEM_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(owner.pubkey(), true),
                AccountMeta::new(address, true),
            ],
            data: create,
        },
        Instruction {
            program_id: TOKEN_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(address, false),
                AccountMeta::new_readonly(*mint, false),
            ],
            data: initialize,
        },
    ];
    tx::send(svm, owner, &[account], &instructions, &[])
        .unwrap_or_else(|failure| panic!("making token account {address} failed: {failure:?}"));
    assert_eq!(token_balance(svm, &address), amount);
    address
}

/// Jupiter's `route` on its own, after the setup that wraps the trader's SOL, with `plan` and
/// `in_amount` in place of the quote's, a quoted output of 1 so that Jupiter's slippage check never
/// refuses, and `platform_fee_bps`. `edit` may change its accounts, writing wallets' token accounts
/// as it does (write rule 1). Returns the SVM after the transaction, and its result.
fn route_alone(
    snapshot: &Snapshot,
    plan: &[u8],
    in_amount: u64,
    platform_fee_bps: u8,
    edit: impl FnOnce(&mut LiteSVM, &mut Vec<AccountMeta>),
) -> (LiteSVM, Result<Outcome, Failure>) {
    let leg = &snapshot.route(ROUTE).legs[0];
    let mut svm = snapshot.svm();
    let trader = wallet::wallet();
    fund(&mut svm, &trader.pubkey(), 10 * SOL);
    let swap = &leg.instructions.swap;
    let mut accounts = swap.accounts.clone();
    edit(&mut svm, &mut accounts);
    let mut data = swap.data[..8].to_vec();
    data.extend_from_slice(plan);
    data.extend_from_slice(&in_amount.to_le_bytes());
    data.extend_from_slice(&1u64.to_le_bytes());
    data.extend_from_slice(&leg.route.slippage_bps.to_le_bytes());
    data.push(platform_fee_bps);
    let route = Instruction {
        program_id: swap.program_id,
        accounts,
        data,
    };
    let mut instructions = leg.instructions.compute_budget.clone();
    instructions.extend(leg.instructions.setup.iter().cloned());
    instructions.push(route);
    let result = tx::send(&mut svm, &trader, &[], &instructions, &leg.lookup_tables);
    (svm, result)
}

/// The template's floor: `sold × price × 10^scale`, less `tolerance_bps`, rounded down at each
/// step as `multiplyDivide` rounds.
fn oracle_floor(sold: u64, price: i64, scale: i32, tolerance_bps: u64) -> u128 {
    let value = u128::from(sold) * u128::try_from(price).expect("a positive price");
    let power = 10u128.pow(scale.unsigned_abs());
    let valued = if scale >= 0 {
        value * power
    } else {
        value / power
    };
    valued * u128::from(10_000 - tolerance_bps) / 10_000
}

/// The highest price whose floor a fill of `received` for `sold` still clears. The floor never
/// falls as the price rises, so this searches for the edge.
fn highest_price_cleared(sold: u64, received: u64, scale: i32, tolerance_bps: u64) -> i64 {
    let clears = |price| oracle_floor(sold, price, scale, tolerance_bps) <= u128::from(received);
    let (mut cleared, mut refused) = (0, i64::MAX);
    assert!(clears(cleared) && !clears(refused));
    while refused - cleared > 1 {
        let middle = cleared + (refused - cleared) / 2;
        if clears(middle) {
            cleared = middle;
        } else {
            refused = middle;
        }
    }
    cleared
}

/// Jupiter's own transaction for the leg, in a fresh SVM: the run's baseline. Returns its outcome
/// and the USDC it bought.
fn jupiter_alone(snapshot: &Snapshot, leg: &Leg) -> (Outcome, u64) {
    let mut svm = snapshot.svm();
    let trader = wallet::wallet();
    fund(&mut svm, &trader.pubkey(), 10 * SOL);
    let outcome = tx::send(
        &mut svm,
        &trader,
        &[],
        &leg.instructions.all(),
        &leg.lookup_tables,
    )
    .unwrap_or_else(|failure| panic!("Jupiter's own transaction: {failure:?}"));
    let bought = token_balance(&svm, &leg.destination_token_account);
    (outcome, bought)
}

#[test]
fn the_fill_clears_the_oracle_floor() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    let scale = swap.scale(&swap.sol_usd);
    assert_eq!(scale, -11);

    let run = swap.sol_usd_run(example);
    let outcome = swap
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    let leg = &swap.leg;
    let received = swap.usdc();
    let floor = oracle_floor(leg.in_amount, market.price, scale, TOLERANCE_BPS);
    assert!(
        u128::from(received) >= floor && received >= leg.other_amount_threshold,
        "bought {received} USDC units, under the oracle's floor of {floor} or Jupiter's {}",
        leg.other_amount_threshold
    );
    // The cleanup closed the wrapped SOL account, which the route had emptied.
    assert_eq!(swap.svm.get_account(&leg.source_token_account), None);

    // The run fills exactly as Jupiter's own transaction does: the template changes what is
    // checked, not what is traded.
    let (alone, bought_alone) = jupiter_alone(&snapshot, leg);
    assert_eq!(received, bought_alone);
    let jupiter = swap.jupiter;
    let run_units = outcome.compute_units_of(&ballista_sdk::ID).unwrap();
    let route_units = outcome.compute_units_of(&jupiter).unwrap();
    println!(
        "sold {} lamports for {received} USDC units; floor {floor} at price {} (exponent {})",
        leg.in_amount, market.price, market.exponent,
    );
    println!(
        "run: {} CU in the transaction, {run_units} in Ballista's run, {route_units} of them \
         Jupiter's; {} bytes",
        outcome.compute_units, outcome.size,
    );
    println!(
        "Jupiter alone: {} CU in the transaction, {} in route; {} bytes",
        alone.compute_units,
        alone.compute_units_of(&jupiter).unwrap(),
        alone.size,
    );
}

/// The floor is exact. At the highest oracle price the fill still clears, the floor equals the fill
/// and the run lands; one unit higher, a hundred-millionth of a dollar, the floor is one USDC base
/// unit more and the run fails at `fillBeatTheOracle`. So the template's on-chain decimals and
/// exponent, and both of its rounding steps, agree with [`oracle_floor`] to the last unit.
#[test]
fn the_floor_is_exact_to_the_last_unit() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    // The fill does not depend on the oracle, so a run at the snapshot's price measures it.
    let mut probe = Swap::new(&snapshot, example);
    let run = probe.sol_usd_run(example);
    probe
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let received = probe.usdc();
    let sold = probe.leg.in_amount;
    let scale = probe.scale(&probe.sol_usd);
    let highest = highest_price_cleared(sold, received, scale, TOLERANCE_BPS);
    assert_eq!(
        oracle_floor(sold, highest, scale, TOLERANCE_BPS),
        u128::from(received)
    );
    assert_eq!(
        oracle_floor(sold, highest + 1, scale, TOLERANCE_BPS),
        u128::from(received) + 1
    );

    let mut swap = Swap::new(&snapshot, example);
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    // Write rule 2. The fill is about the snapshot's price, so the edge sits about the 1% tolerance
    // above it.
    assert!((market.price..market.price * 102 / 100).contains(&highest));
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: highest + 1,
            ..market
        },
    );
    let failure = swap.send(swap.sol_usd_run(example)).unwrap_err();
    assert_requirement_failed(&failure, example, "fillBeatTheOracle");

    // A failed transaction changes nothing but the fee payer's balance, so the same fill follows.
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: highest,
            ..market
        },
    );
    swap.send(swap.sol_usd_run(example))
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(swap.usdc(), received);
}

/// The oracle 5% above the market the route fills at (write rule 2): the swap runs, the fill falls
/// short of the oracle's valuation, and the whole transaction reverts at `fillBeatTheOracle`.
#[test]
fn an_oracle_above_the_market_fails_the_fill_check() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: market.price * 105 / 100,
            ..market
        },
    );
    let trader = swap.trader.pubkey();
    let before = swap.svm.get_balance(&trader).unwrap();

    let failure = swap.send(swap.sol_usd_run(example)).unwrap_err();
    assert_requirement_failed(&failure, example, "fillBeatTheOracle");
    // The check came after a swap that completed: Jupiter's `route` returned before it.
    let jupiter_returned = format!("Program {} success", swap.jupiter);
    assert!(failure.logs.contains(&jupiter_returned), "{failure:?}");
    // Everything reverted but the fee, the setup's wrapped SOL and USDC accounts included.
    let leg = &swap.leg;
    assert_eq!(swap.svm.get_account(&leg.source_token_account), None);
    assert_eq!(swap.svm.get_account(&leg.destination_token_account), None);
    assert_eq!(swap.svm.get_balance(&trader), Some(before - failure.fee));
}

/// Decoys where the template measures: the trader's own second wrapped SOL and USDC accounts,
/// made through the System and Token programs, at `sourceAta` and `destinationAta`, while the
/// route's steps still name the trader's associated accounts. Jupiter asks of those two positions
/// only that the source hold at least `in_amount` and the destination hold the destination mint,
/// and it moves the accounts its steps name. So the decoys see nothing sold and nothing bought, and
/// the fill check alone, whose floor is proportional to what was sold, passes at any price: here
/// the oracle is 5% above the market (write rule 2). The decoys are the trader's, so the owner
/// checks pass them; another wallet's fail there first
/// (`another_wallets_source_fails_at_sells_the_traders_own_tokens`). The run fails at
/// `soldTheRouteInput`, after the route ran.
#[test]
fn decoys_where_the_template_measures_fail_at_sold_the_route_input() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let leg = swap.leg.clone();
    let decoy_source = another_token_account(
        &mut swap.svm,
        &swap.trader,
        &keypair(b"ballista-protocol-tests-decoy-01"),
        &leg.input_mint,
        leg.in_amount,
    );
    let decoy_destination = another_token_account(
        &mut swap.svm,
        &swap.trader,
        &keypair(b"ballista-protocol-tests-decoy-02"),
        &leg.output_mint,
        0,
    );
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: market.price * 105 / 100,
            ..market
        },
    );

    let run = swap.sol_usd_run_routed(
        example,
        Routing {
            source: decoy_source,
            destination: decoy_destination,
            ..Routing::of(&leg)
        },
    );
    let failure = match swap.send(run) {
        Err(failure) => failure,
        // What the requirement stops: the route sells the trader's SOL at the market, and the fill
        // check passes on two balances that never moved.
        Ok(outcome) => panic!(
            "the run landed. The decoys still hold {} and {}; the trader's wrapped SOL account \
             was {}, and its USDC account holds {}. {} CU, {} bytes",
            token_balance(&swap.svm, &decoy_source),
            token_balance(&swap.svm, &decoy_destination),
            holding(&swap.svm, &leg.source_token_account),
            swap.usdc(),
            outcome.compute_units,
            outcome.size,
        ),
    };
    assert_requirement_failed(&failure, example, "soldTheRouteInput");
    // Jupiter took the decoys and ran the route: the requirement refused a completed swap.
    let jupiter_returned = format!("Program {} success", swap.jupiter);
    assert!(failure.logs.contains(&jupiter_returned), "{failure:?}");
}

/// `soldTheRouteInput` requires exactly `in_amount` to leave the source, and every route in the
/// snapshot is one step at 100%. So this sends Jupiter's `route` on its own, with the route's step
/// split in two over the same pool, and with a platform fee, and measures what left the trader's
/// wrapped SOL: exactly `in_amount` each time. A split's last step takes the remainder, and a fee
/// paid in the input mint comes out of `in_amount` rather than on top of it. A plan whose shares
/// pass 100% is refused.
#[test]
fn jupiter_takes_exactly_in_amount_through_a_split_or_a_platform_fee() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let leg = snapshot.route(ROUTE).legs[0].clone();
    let plan = &leg.route.route_plan;
    // One step: a u32 count, then the step, which ends in its percent and its input and output
    // indices.
    assert_eq!(plan[..4], 1u32.to_le_bytes());
    let step = &plan[4..];
    assert_eq!(step[step.len() - 3..], [100, 0, 1]);
    let venue = &step[..step.len() - 3];
    let split = |first: u8, second: u8| {
        let mut plan = 2u32.to_le_bytes().to_vec();
        for percent in [first, second] {
            plan.extend_from_slice(venue);
            plan.extend_from_slice(&[percent, 0, 1]);
        }
        plan
    };
    // The second step takes the pool's accounts again, after the first's.
    let twice = |_: &mut LiteSVM, accounts: &mut Vec<AccountMeta>| {
        let steps = accounts[ROUTE_FIXED_ACCOUNTS..].to_vec();
        accounts.extend(steps);
    };
    // The setup wraps the quote's `in_amount`, 1 SOL.
    assert_eq!(leg.in_amount, SOL);
    let sold = |svm: &LiteSVM| SOL - token_balance(svm, &leg.source_token_account);
    let bought = |svm: &LiteSVM| token_balance(svm, &leg.destination_token_account);

    // An odd amount, so that no split divides it evenly.
    let odd = SOL - 1;
    // Two swaps through the same pool that together sell `odd` buy the same total regardless of
    // how the shares split it: the first split's fill is the reference the rest must match.
    let mut split_fill = None;
    for (first, second) in [(50, 50), (33, 67)] {
        let (svm, result) = route_alone(&snapshot, &split(first, second), odd, 0, twice);
        result.unwrap_or_else(|failure| panic!("[{first}, {second}]: {failure:?}"));
        assert_eq!(sold(&svm), odd, "[{first}, {second}]");
        let usdc = bought(&svm);
        assert_eq!(
            usdc,
            *split_fill.get_or_insert(usdc),
            "[{first}, {second}] bought a different amount than the first split"
        );
        println!("[{first}, {second}] sold {odd} for {usdc} USDC units");
    }
    let (_, result) = route_alone(&snapshot, &split(50, 100), odd, 0, twice);
    let failure = result.expect_err("a plan whose shares pass 100% must fail");
    assert_eq!(
        (failure.program, failure.code),
        (snapshot.named("jupiter"), Some(6010)),
        "{failure:?}"
    );

    // A platform fee, into the platform's account of either mint.
    let (svm, result) = route_alone(&snapshot, plan, SOL, 0, |_, _| {});
    result.unwrap_or_else(|failure| panic!("no fee: {failure:?}"));
    let unfeed = bought(&svm);
    let platform = keypair(b"ballista-protocol-tests-platform").pubkey();
    for mint in [leg.input_mint, leg.output_mint] {
        let fee_account = associated_token_address(&platform, &mint);
        let (svm, result) = route_alone(&snapshot, plan, SOL, PLATFORM_FEE_BPS, |svm, accounts| {
            token_account(svm, &platform, &mint, 0);
            accounts[PLATFORM_FEE_ACCOUNT] = AccountMeta::new(fee_account, false);
        });
        result.unwrap_or_else(|failure| panic!("a fee in {mint}: {failure:?}"));
        assert_eq!(sold(&svm), SOL, "a fee in {mint}");
        let fee = token_balance(&svm, &fee_account);
        if mint == leg.input_mint {
            // Out of `in_amount`: the pool swaps the rest.
            assert_eq!(fee, fee_at(SOL, u64::from(PLATFORM_FEE_BPS)));
        } else {
            // Out of the fill.
            assert_eq!(fee, fee_at(unfeed, u64::from(PLATFORM_FEE_BPS)));
            assert_eq!(bought(&svm) + fee, unfeed);
        }
        println!(
            "a {PLATFORM_FEE_BPS} bps fee in {mint}: sold {SOL} for {} USDC units, and the fee \
             was {fee}",
            bought(&svm)
        );
    }
}

/// A fresh run at the market, `platform_fee_bps` and `HOSTILE_SLIPPAGE_BPS` in place of the
/// quote's own, its platform fee paid to a fresh attacker's USDC account: nothing ties `route`'s
/// platform fee account to the trader. Returns the swap, the attacker's account, and the result.
fn hostile_fee_run(
    snapshot: &Snapshot,
    example: &Example,
    platform_fee_bps: u64,
) -> (Swap, Address, Result<Outcome, Failure>) {
    let mut swap = Swap::new(snapshot, example);
    let leg = swap.leg.clone();
    let attacker = keypair(b"ballista-protocol-tests-attacker").pubkey();
    let attacker_usdc = token_account(&mut swap.svm, &attacker, &leg.output_mint, 0);
    let run = swap.sol_usd_run_priced(
        example,
        u64::from(HOSTILE_SLIPPAGE_BPS),
        platform_fee_bps,
        Routing::platform_fee_to(&leg, attacker_usdc),
    );
    let result = swap.send(run);
    (swap, attacker_usdc, result)
}

/// A platform fee the trader never agreed to, paid to an attacker's account: the template caps
/// `platformFeeBps` at `MAX_PLATFORM_FEE_BPS`, 0, so any fee fails at `platformFeeWithinCap` before
/// Jupiter runs. Before the cap, `PLATFORM_FEE_BPS` landed inside `TOLERANCE_BPS` once
/// `slippageBps` was relaxed to `HOSTILE_SLIPPAGE_BPS`, and only `EXCESSIVE_FEE_BPS` failed, at
/// `fillBeatTheOracle`. `findings/platform-fee.md`.
#[test]
fn a_hostile_platform_fee_fails_at_platform_fee_within_cap() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];

    for fee_bps in [1, PLATFORM_FEE_BPS, EXCESSIVE_FEE_BPS, u8::MAX] {
        let (swap, attacker_usdc, result) = hostile_fee_run(&snapshot, example, u64::from(fee_bps));
        let failure = result.expect_err("a platform fee above the cap should fail");
        assert_requirement_failed(&failure, example, "platformFeeWithinCap");
        assert!(!jupiter_ran(&swap.jupiter, &failure), "{fee_bps} bps");
        assert_eq!(token_balance(&swap.svm, &attacker_usdc), 0);
        println!(
            "{fee_bps} bps fee: refused at platformFeeWithinCap after {} CU in Ballista's run",
            failure.compute_units_of(&ballista_sdk::ID).unwrap()
        );
    }
}

/// The cap is a template constant an author running their own frontend can raise. The test-only
/// `jupiterOracleCheckedSwapFeeCap100` is this template with `MAX_PLATFORM_FEE_BPS` at 100: a 100
/// bps fee lands and pays the fee account exactly its share, and 101 bps fails at
/// `platformFeeWithinCap` before Jupiter runs.
#[test]
fn a_raised_cap_takes_a_fee_within_it_and_refuses_one_above() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let scenarios = scenarios();
    let raised = &scenarios["jupiterOracleCheckedSwapFeeCap100"];

    // No fee, at the market: the on-chain fill a fee comes out of.
    let mut baseline = Swap::new(&snapshot, &examples[TEMPLATE]);
    let run = baseline.sol_usd_run(&examples[TEMPLATE]);
    baseline
        .send(run)
        .unwrap_or_else(|failure| panic!("no fee: {failure:?}"));
    let unfeed = baseline.usdc();

    let (swap, fee_account, result) = hostile_fee_run(&snapshot, raised, 100);
    let outcome =
        result.unwrap_or_else(|failure| panic!("100 bps under a 100 bps cap: {failure:?}"));
    let fee = token_balance(&swap.svm, &fee_account);
    assert_eq!(fee, fee_at(unfeed, 100));
    assert_eq!(swap.usdc(), unfeed - fee);
    println!(
        "100 bps under a 100 bps cap: the fee account took {fee}, the trader {}; {} CU, {} bytes",
        swap.usdc(),
        outcome.compute_units,
        outcome.size
    );

    let (swap, fee_account, result) = hostile_fee_run(&snapshot, raised, 101);
    let failure = result.expect_err("101 bps should fail under a 100 bps cap");
    assert_requirement_failed(&failure, raised, "platformFeeWithinCap");
    assert!(!jupiter_ran(&swap.jupiter, &failure));
    assert_eq!(token_balance(&swap.svm, &fee_account), 0);
}

/// Another wallet's wrapped SOL, holding `in_amount` (write rule 1), at `sourceAta`, while the
/// route's steps still sell the trader's own. The template holds both ends of the swap to the
/// trader: it fails at `sellsTheTradersOwnTokens`, before the route runs.
#[test]
fn another_wallets_source_fails_at_sells_the_traders_own_tokens() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let leg = swap.leg.clone();
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();
    let theirs = token_account(&mut swap.svm, &other, &leg.input_mint, leg.in_amount);

    let run = swap.sol_usd_run_routed(
        example,
        Routing {
            source: theirs,
            ..Routing::of(&leg)
        },
    );
    let failure = swap.send(run).unwrap_err();
    assert_requirement_failed(&failure, example, "sellsTheTradersOwnTokens");
    assert!(!jupiter_ran(&swap.jupiter, &failure), "{failure:?}");
    assert_eq!(token_balance(&swap.svm, &theirs), leg.in_amount);
}

/// A hostile route's proceeds: an attacker's USDC account (write rule 1) at `destinationAta`, and
/// in place of the trader's as the Meteora step's output. Jupiter checks `route`'s destination by
/// its mint alone, and the step pays the account it names, so the fill reaches the attacker, where
/// the template measures it, and clears the oracle's floor: the oracle is at the market. The
/// template requires the trader to own the destination, and fails at `proceedsGoToTheTrader`,
/// before the route runs. With the trader's own account at `destinationAta` instead, the step
/// still pays the attacker, and the fill check sees nothing arrive.
#[test]
fn an_attackers_destination_fails_at_proceeds_go_to_the_trader() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let leg = swap.leg.clone();
    let attacker = keypair(b"ballista-protocol-tests-attacker").pubkey();
    let attacker_usdc = token_account(&mut swap.svm, &attacker, &leg.output_mint, 0);
    let trader = swap.trader.pubkey();
    let lamports_before = swap.svm.get_balance(&trader).unwrap();

    let run = swap.sol_usd_run_routed(
        example,
        Routing {
            destination: attacker_usdc,
            ..Routing::paying(&leg, attacker_usdc)
        },
    );
    let failure = match swap.send(run) {
        Err(failure) => failure,
        // What the requirement stops: the trader's SOL buys USDC for the attacker.
        Ok(outcome) => panic!(
            "the run landed. The attacker's USDC account holds {}; the trader's wrapped SOL \
             account was {}, its USDC account holds {}, and its lamports fell by {} (the fee \
             {}). {} CU, {} bytes",
            token_balance(&swap.svm, &attacker_usdc),
            holding(&swap.svm, &leg.source_token_account),
            swap.usdc(),
            lamports_before - swap.svm.get_balance(&trader).unwrap(),
            outcome.fee,
            outcome.compute_units,
            outcome.size,
        ),
    };
    assert_requirement_failed(&failure, example, "proceedsGoToTheTrader");
    assert!(!jupiter_ran(&swap.jupiter, &failure), "{failure:?}");
    assert_eq!(token_balance(&swap.svm, &attacker_usdc), 0);
    assert_eq!(
        swap.svm.get_balance(&trader),
        Some(lamports_before - failure.fee)
    );

    // The trader's own USDC account where the template measures, the attacker's still in the step.
    let run = swap.sol_usd_run_routed(example, Routing::paying(&leg, attacker_usdc));
    let failure = swap.send(run).unwrap_err();
    assert_requirement_failed(&failure, example, "fillBeatTheOracle");
    assert_eq!(token_balance(&swap.svm, &attacker_usdc), 0);
}

/// Another feed's price account is refused before anything else reads it, and before Jupiter runs.
///
/// The account is mainnet's USDC/USD feed, written by copying SOL/USD's with that feed's id and
/// address and giving it USDC's price of $0.9999 (write rule 2). The receiver owns both and both
/// are fully verified, so only the feed id tells them apart. Valued at USDC's price, the SOL sold
/// would have a floor of 989,901 USDC units, under a dollar, which any fill clears. When the feed id
/// was a run input, a builder who named USDC/USD's got exactly that.
#[test]
fn a_price_from_another_feed_fails_at_the_feed_pin() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let usdc_usd = copy_pyth_feed(&mut swap.svm, &swap.sol_usd, &USDC_USD_FEED_ID);
    let copied = pyth_price(&swap.svm, &usdc_usd);
    set_pyth_price(
        &mut swap.svm,
        &usdc_usd,
        PythPrice {
            price: 99_990_000,
            conf: 70_000,
            ..copied
        },
    );
    assert_eq!(
        oracle_floor(
            swap.leg.in_amount,
            99_990_000,
            swap.scale(&usdc_usd),
            TOLERANCE_BPS
        ),
        989_901
    );

    let run = swap.run(example, usdc_usd);
    let failure = swap.send(run).unwrap_err();
    assert_requirement_failed(&failure, example, "priceIsTheExpectedFeed");
    assert!(!jupiter_ran(&swap.jupiter, &failure), "{failure:?}");
}

/// The pair is pinned with the feed, since SOL/USD prices SOL in dollars and nothing else. Were the
/// mints the builder's choice, a route could sell another token at SOL's price, or buy one worth
/// less than a dollar. A run that passes another mint where the template reads a mint's decimals
/// fails that account's address constraint, before any step: here USDC's mint as `sourceMint`.
#[test]
fn another_mint_fails_the_pinned_pair() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let source_mint = example
        .fixed_accounts
        .iter()
        .position(|name| name == "sourceMint")
        .unwrap();
    let mut run = swap.sol_usd_run(example);
    // The harness refuses to bind a pinned account to another address, so the run is edited after
    // it is built. Account 0 is the template.
    run.accounts[1 + source_mint].pubkey = swap.leg.output_mint;

    let failure = swap.send(run).unwrap_err();
    assert_eq!(
        tx::ballista_error(&failure),
        Some((
            "AccountConstraintFailed",
            u16::try_from(source_mint).unwrap()
        )),
        "{failure:?}"
    );
    assert!(!jupiter_ran(&swap.jupiter, &failure), "{failure:?}");
}
