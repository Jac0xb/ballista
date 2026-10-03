//! `jitoProfitGuardedTip` against the real programs: a round trip through Jupiter that pays a Jito
//! tip only out of its own profit.
//!
//! The strategy is the snapshot's `solToUsdcToSol`: SOL for USDC on Meteora DLMM, then back to SOL
//! on Raydium CLMM. Jupiter's API refuses a circular quote, so the snapshot holds it as two legs,
//! and [`round_trip`] joins them into one `route` that starts and ends in the searcher's wrapped
//! SOL.
//!
//! At the snapshot's prices the round trip loses to the pools' fees: no route in the snapshot makes
//! money. The profitable runs create the opportunity the way one arises on mainnet, as a backrun.
//! A whale sells SOL into the Raydium pool with Raydium's own `swap`, which lowers that pool's SOL
//! price, and the searcher buys back more SOL there than it sold on Meteora. The only state written
//! directly is the whale's wallet (write rule 1); the profit comes out of the real pools, through
//! the real programs.

use {
    ballista_protocol_tests::{
        snapshot::{jupiter_ran, Route, Snapshot, PLATFORM_FEE_ACCOUNT, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, assert_requirement_failed, Failure, Outcome},
        wallet::{
            self, associated_token_address, fund, keypair, token_account, token_balance, SOL,
            WSOL_MINT,
        },
    },
    ballista_sdk::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    sha2::{Digest, Sha256},
    solana_address::Address,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::{AccountMeta, Instruction},
    solana_signer::Signer,
};

const EXAMPLE: &str = "jitoProfitGuardedTip";
const ROUTE: &str = "solToUsdcToSol";

/// What the whale sells into the Raydium pool: enough to move its price while staying inside the
/// tick array that holds the current tick, the one array on that side the snapshot has.
const WHALE_SALE: u64 = 500 * SOL;

/// Jito's minimum tip. The block engine enforces it; the chain does not.
const MINIMUM_TIP: u64 = 1_000;

/// What the searcher insists on keeping after the tip, in the runs that make money.
const EDGE: u64 = 100_000;

/// `route`'s own accounts, before the route's: the token program, the user, the user's source and
/// destination token accounts, the destination token account, the destination mint, the platform
/// fee account, the event authority and the program.
const ROUTE_FIXED_ACCOUNTS: usize = 9;
/// `route`'s data after its plan: `in_amount`, `quoted_out_amount` (u64 each), `slippage_bps`
/// (u16) and `platform_fee_bps` (u8).
const ROUTE_TAIL_LEN: usize = 8 + 8 + 2 + 1;

/// Of those, the ones the template passes itself: the token program, the searcher, and its wrapped
/// SOL twice, as source and as destination. The rest of the route's accounts are the group.
const TEMPLATE_ROUTE_ACCOUNTS: usize = 4;

/// Positions within [`ROUTE_FIXED_ACCOUNTS`]: the user's source and destination token accounts,
/// and the destination mint.
const SOURCE: usize = 2;
const DESTINATION: usize = 3;
const DESTINATION_MINT: usize = 5;

/// More than the whale's sale consumed at slot 451,100,151 (165,219 CU; re-measured by the
/// ignored `measure_the_findings_numbers`), leaving room for a snapshot refresh to move it.
const WHALE_SALE_COMPUTE_UNIT_LIMIT: u32 = 400_000;

/// The round trip as one `route`: the legs' plan steps in order, the second's input and output
/// indices moved up by one so that it spends exactly what the first produced, and the second leg's
/// accounts after the first's. It sells the first leg's input for at least the second leg's quote
/// less its slippage: the two quotes' own terms.
///
/// # Panics
///
/// If the route is not two legs of one step each, from wrapped SOL back to it, or if either leg's
/// swap instruction is not a Jupiter `route`. A step ends in its input and output index, but the
/// `Swap` enum before them has variants of different lengths, so a longer plan cannot be
/// renumbered without decoding it.
fn round_trip(route: &Route) -> Instruction {
    let [first, second] = route.legs.as_slice() else {
        panic!("{ROUTE} has {} legs, not two", route.legs.len());
    };
    assert!(
        first.input_mint == WSOL_MINT && second.output_mint == WSOL_MINT,
        "{ROUTE} does not start and end in SOL"
    );
    // Neither the snapshot loader nor anything else checks that a leg's swap instruction is a
    // Jupiter `route`: it checks neither the program nor the discriminator. Only this does.
    let route_discriminator = anchor_discriminator("route");
    for leg in [first, second] {
        assert_eq!(
            &leg.instructions.swap.data[..8],
            &route_discriminator[..],
            "{ROUTE}'s legs must both be a Jupiter `route`"
        );
    }
    // What ends up spliced in as the round trip's source (below) must be what is actually left as
    // its destination, not just what the snapshot's wallet-accounts metadata calls them.
    assert_eq!(
        first.instructions.swap.accounts[SOURCE].pubkey,
        second.instructions.swap.accounts[DESTINATION].pubkey,
        "{ROUTE}'s legs start and end in different accounts"
    );

    // The `route` discriminator, then a two-step plan.
    let mut data = first.instructions.swap.data[..8].to_vec();
    data.extend_from_slice(&2u32.to_le_bytes());
    for (position, leg) in [(0u8, first), (1, second)] {
        let (count, step) = leg.route.route_plan.split_at(4);
        assert_eq!(count, 1u32.to_le_bytes(), "each leg must be a single step");
        let (swap_and_percent, indices) = step.split_at(step.len() - 2);
        assert_eq!(
            indices,
            [0, 1],
            "a single step reads index 0 and writes index 1"
        );
        data.extend_from_slice(swap_and_percent);
        data.extend_from_slice(&[position, position + 1]);
    }
    data.extend_from_slice(&first.route.in_amount.to_le_bytes());
    data.extend_from_slice(&second.route.quoted_out_amount.to_le_bytes());
    data.extend_from_slice(&second.route.slippage_bps.to_le_bytes());
    data.push(second.route.platform_fee_bps);

    // The second leg's fixed accounts end in SOL, as the round trip does; its source is the first
    // leg's, the same wrapped SOL account.
    let mut accounts = second.instructions.swap.accounts[..ROUTE_FIXED_ACCOUNTS].to_vec();
    accounts[SOURCE] = first.instructions.swap.accounts[SOURCE].clone();
    for leg in [first, second] {
        accounts.extend_from_slice(&leg.instructions.swap.accounts[ROUTE_FIXED_ACCOUNTS..]);
    }
    Instruction {
        program_id: first.instructions.swap.program_id,
        accounts,
        data,
    }
}

/// The little-endian u64 at `offset` in `data`.
fn u64_at(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

/// An Anchor instruction discriminator: the first eight bytes of `sha256("global:<name>")`.
fn anchor_discriminator(name: &str) -> [u8; 8] {
    Sha256::digest(format!("global:{name}"))[..8]
        .try_into()
        .unwrap()
}

/// The mint of the token account at `address`.
fn mint_of(svm: &LiteSVM, address: &Address) -> Address {
    let account = svm
        .get_account(address)
        .unwrap_or_else(|| panic!("token account {address} does not exist"));
    Address::new_from_array(account.data[..32].try_into().unwrap())
}

fn balance(svm: &LiteSVM, address: &Address) -> u64 {
    svm.get_balance(address).unwrap_or(0)
}

/// The whale sells `lamports` of wrapped SOL for USDC into the pool the round trip buys its SOL
/// back from, with Raydium CLMM's own `swap(amount, other_amount_threshold, sqrt_price_limit_x64,
/// is_base_input)`, which lowers the pool's SOL price. The accounts are the ones Jupiter's step
/// passes Raydium in the second leg, with the input and output sides exchanged.
///
/// The whale's wallet and token accounts are written directly, under write rule 1.
///
/// # Panics
///
/// If a snapshot refresh has moved the second leg off Raydium CLMM: the sale is built for
/// Raydium's `swap`.
fn whale_sells_sol(svm: &mut LiteSVM, snapshot: &Snapshot, lamports: u64) -> Outcome {
    let route = snapshot.route(ROUTE);
    // Jupiter's Raydium step: the Raydium program, then `swap`'s accounts in `swap`'s order.
    let step = &route.legs[1].instructions.swap.accounts[ROUTE_FIXED_ACCOUNTS..];
    assert_eq!(
        step[0].pubkey,
        snapshot.named("raydiumClmm"),
        "a snapshot refresh moved {ROUTE}'s second leg off Raydium CLMM; the whale's sale is built \
         for Raydium's `swap` and must be rebuilt for the AMM the leg goes through now"
    );

    let whale = keypair(b"ballista-protocol-tests-whale-01");
    let usdc_mint = route.legs[0].output_mint;
    fund(svm, &whale.pubkey(), SOL);
    let wsol = token_account(svm, &whale.pubkey(), &WSOL_MINT, lamports);
    let usdc = token_account(svm, &whale.pubkey(), &usdc_mint, 0);

    // The second leg sells USDC, so its input vault is the pool's USDC and its output vault the
    // SOL.
    let (usdc_vault, sol_vault) = (step[6].pubkey, step[7].pubkey);
    assert_eq!(
        mint_of(svm, &sol_vault),
        WSOL_MINT,
        "step[7] should be Raydium's SOL vault"
    );
    assert_eq!(
        mint_of(svm, &usdc_vault),
        usdc_mint,
        "step[6] should be Raydium's USDC vault"
    );
    let mut data = anchor_discriminator("swap").to_vec();
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // other_amount_threshold: any output
    data.extend_from_slice(&0u128.to_le_bytes()); // sqrt_price_limit_x64: none
    data.push(1); // is_base_input: `lamports` is the amount sold
    let sale = Instruction {
        program_id: step[0].pubkey,
        accounts: vec![
            AccountMeta::new_readonly(whale.pubkey(), true), // payer
            AccountMeta::new_readonly(step[2].pubkey, false), // amm_config
            AccountMeta::new(step[3].pubkey, false),         // pool_state
            AccountMeta::new(wsol, false),                   // input_token_account
            AccountMeta::new(usdc, false),                   // output_token_account
            AccountMeta::new(sol_vault, false),              // input_vault
            AccountMeta::new(usdc_vault, false),             // output_vault
            AccountMeta::new(step[8].pubkey, false),         // observation_state
            AccountMeta::new_readonly(step[9].pubkey, false), // token_program
            // The tick array holding the current tick, which the sale stays inside, then the tick
            // array bitmap extension.
            AccountMeta::new(step[10].pubkey, false),
            AccountMeta::new_readonly(step[11].pubkey, false),
        ],
        data,
    };
    let limit = ComputeBudgetInstruction::set_compute_unit_limit(WHALE_SALE_COMPUTE_UNIT_LIMIT);
    let outcome = tx::send(svm, &whale, &[], &[limit, sale], &[])
        .unwrap_or_else(|failure| panic!("the whale's sale failed: {failure:?}"));
    assert_eq!(token_balance(svm, &wsol), 0, "the whale sold it all");
    assert!(
        token_balance(svm, &usdc) > 0,
        "the whale's sale must have produced USDC"
    );
    outcome
}

/// A fresh SVM from the snapshot, after the whale's sale when there is one. The searcher holds
/// 10 SOL and an empty USDC account (write rule 1), so the round trip's setup creates only the
/// wrapped SOL account, whose rent the cleanup returns: the searcher's lamports then account for
/// the fee, the tip and the profit alone.
fn market(snapshot: &Snapshot, whale_sale: Option<u64>) -> LiteSVM {
    let route = snapshot.route(ROUTE);
    let mut svm = snapshot.svm();
    let searcher = wallet::wallet().pubkey();
    fund(&mut svm, &searcher, 10 * SOL);
    token_account(&mut svm, &searcher, &route.legs[0].output_mint, 0);
    if let Some(lamports) = whale_sale {
        whale_sells_sol(&mut svm, snapshot, lamports);
    }
    svm
}

/// What the round trip did when sent to Jupiter directly, without Ballista.
struct Realised {
    /// The change in the searcher's wrapped SOL: the round trip's profit, or its loss.
    wrapped_sol: i128,
    /// The change in the searcher's lamports while the route ran, its fee aside.
    lamports: i128,
    /// The round trip's own transaction: its compute units and wire size.
    outcome: Outcome,
}

/// Sends the round trip to Jupiter directly. The SOL is wrapped in a transaction of its own first,
/// so the balances can be read on both sides of the route.
fn realised(svm: &mut LiteSVM, route: &Route) -> Realised {
    let first = &route.legs[0];
    let searcher = wallet::wallet();
    tx::send(svm, &searcher, &[], &first.instructions.setup, &[])
        .unwrap_or_else(|failure| panic!("wrapping the SOL failed: {failure:?}"));
    let wsol = first.source_token_account;
    let wsol_before = token_balance(svm, &wsol);
    let lamports_before = balance(svm, &searcher.pubkey());

    let mut instructions = first.instructions.compute_budget.clone();
    instructions.push(round_trip(route));
    let outcome = tx::send(svm, &searcher, &[], &instructions, &route.lookup_tables())
        .unwrap_or_else(|failure| panic!("the round trip failed: {failure:?}"));
    Realised {
        wrapped_sol: i128::from(token_balance(svm, &wsol)) - i128::from(wsol_before),
        lamports: i128::from(balance(svm, &searcher.pubkey())) + i128::from(outcome.fee)
            - i128::from(lamports_before),
        outcome,
    }
}

/// `profit - EDGE`, panicking with a message naming `WHALE_SALE` rather than underflowing if a
/// snapshot refresh has shrunk the profit below it.
fn profit_less_edge(profit: u64) -> u64 {
    profit.checked_sub(EDGE).unwrap_or_else(|| {
        panic!(
            "a profit of {profit} cannot cover EDGE ({EDGE}); WHALE_SALE may need resizing after \
             a snapshot refresh"
        )
    })
}

/// The profit the backrun realises after a whale sells `whale_sale` lamports of SOL, measured in
/// an SVM of its own.
fn backrun_profit(snapshot: &Snapshot, whale_sale: u64) -> u64 {
    let mut svm = market(snapshot, Some(whale_sale));
    let profit = realised(&mut svm, snapshot.route(ROUTE)).wrapped_sol;
    println!("{ROUTE} after the whale's {whale_sale}-lamport sale realises {profit} lamports");
    u64::try_from(profit).unwrap_or_else(|_| panic!("the backrun lost {profit} lamports"))
}

/// The template, uploaded by a creator of its own.
fn upload_template(svm: &mut LiteSVM, example: &Example) -> Address {
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(svm, &creator.pubkey(), SOL);
    upload(svm, &creator, 1, &example.payload)
}

/// The accounts a run binds, other than the programs.
struct Accounts {
    searcher: Address,
    wsol: Address,
    tip: Address,
}

impl Accounts {
    /// The searcher's wrapped SOL, which the round trip starts and ends in, and the snapshot's
    /// Jito tip account.
    fn of(snapshot: &Snapshot) -> Self {
        Accounts {
            searcher: wallet::wallet().pubkey(),
            wsol: snapshot.route(ROUTE).legs[0].source_token_account,
            tip: snapshot.named("jitoTip"),
        }
    }
}

/// A run of the template with the round trip as its strategy.
fn tip_run(
    snapshot: &Snapshot,
    template: Address,
    example: &Example,
    accounts: &Accounts,
    tip: u64,
    edge: u64,
) -> Instruction {
    tip_run_of(
        snapshot,
        template,
        example,
        accounts,
        tip,
        edge,
        &round_trip(snapshot.route(ROUTE)),
    )
}

/// [`tip_run`] with `round_trip` as the strategy in place of the snapshot's own.
fn tip_run_of(
    snapshot: &Snapshot,
    template: Address,
    example: &Example,
    accounts: &Accounts,
    tip: u64,
    edge: u64,
    round_trip: &Instruction,
) -> Instruction {
    // The joined data is `route`'s: the plan, then `in_amount`, `quoted_out_amount`,
    // `slippage_bps` and `platform_fee_bps`, which the template takes in parts.
    let tail = round_trip.data.len() - ROUTE_TAIL_LEN;
    Run::new(template, example)
        .account("systemProgram", SYSTEM_PROGRAM_ID, false, false)
        .account("strategyProgram", snapshot.named("jupiter"), false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("searcher", accounts.searcher, true, true)
        .account("wsolAccount", accounts.wsol, true, false)
        .account("jitoTip", accounts.tip, true, false)
        .input_bytes("routePlan", &round_trip.data[8..tail])
        .input_u64("inAmount", u64_at(&round_trip.data, tail))
        .input_u64("quotedOutAmount", u64_at(&round_trip.data, tail + 8))
        .input_u64(
            "slippageBps",
            u16::from_le_bytes([round_trip.data[tail + 16], round_trip.data[tail + 17]]).into(),
        )
        .input_u64("platformFeeBps", round_trip.data[tail + 18].into())
        .input_u64("tipLamports", tip)
        .input_u64("minimumEdge", edge)
        .group(
            "strategyAccounts",
            round_trip.accounts[TEMPLATE_ROUTE_ACCOUNTS..].to_vec(),
        )
        .build()
}

/// The searcher's transaction: Jupiter's own for the first leg with `route` replaced by `run`.
/// That is its compute budget, the setup that creates the wrapped SOL account and wraps the round
/// trip's SOL, the run, and the cleanup that closes the account, profit and all, into lamports.
fn send_backrun(
    svm: &mut LiteSVM,
    snapshot: &Snapshot,
    run: Instruction,
) -> Result<Outcome, Failure> {
    let route = snapshot.route(ROUTE);
    tx::send(
        svm,
        &wallet::wallet(),
        &[],
        &route.legs[0].instructions.with_swap(run),
        &route.lookup_tables(),
    )
}

/// The compute units Ballista's run consumed, CPIs included, from its `consumed` log line.
fn run_units(logs: &[String]) -> u64 {
    let prefix = format!("Program {} consumed ", ballista_sdk::ID);
    logs.iter()
        .find_map(|line| line.strip_prefix(&prefix)?.split(' ').next()?.parse().ok())
        .expect("Ballista logs the units it consumed")
}

/// What [`run_tip`] returns: the SVM after the backrun, the tip account's and the searcher's
/// balances just before it, and the backrun's own result.
struct TipRun {
    svm: LiteSVM,
    tip_before: u64,
    searcher_before: u64,
    result: Result<Outcome, Failure>,
}

/// The market, upload, `tip_run` and `send_backrun` sequence most of this file's tests repeat:
/// builds the market (with a whale's sale first when `whale_sale` is some), uploads the template,
/// runs `setup` on the SVM (for a test that needs another account in place before the run), then
/// builds and sends the backrun.
fn run_tip(
    snapshot: &Snapshot,
    example: &Example,
    whale_sale: Option<u64>,
    setup: impl FnOnce(&mut LiteSVM),
    accounts: &Accounts,
    tip: u64,
    edge: u64,
) -> TipRun {
    let mut svm = market(snapshot, whale_sale);
    let template = upload_template(&mut svm, example);
    setup(&mut svm);
    let tip_before = balance(&svm, &accounts.tip);
    let searcher_before = balance(&svm, &accounts.searcher);
    let run = tip_run(snapshot, template, example, accounts, tip, edge);
    let result = send_backrun(&mut svm, snapshot, run);
    TipRun {
        svm,
        tip_before,
        searcher_before,
        result,
    }
}

/// Why the template measures wrapped SOL: `route` moves token accounts only. Sent to Jupiter on
/// its own, a profitable round trip grows the searcher's wrapped SOL and leaves its lamports alone
/// but for the fee, so a template that read the lamports around the route would see no profit.
#[test]
fn a_route_moves_wrapped_sol_and_no_lamports() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let mut svm = market(&snapshot, Some(WHALE_SALE));
    let realised = realised(&mut svm, snapshot.route(ROUTE));
    assert!(realised.wrapped_sol > 0, "{}", realised.wrapped_sol);
    assert_eq!(
        realised.lamports, 0,
        "route must not move the searcher's lamports"
    );
}

/// The round trip with `edit` applied, sent to Jupiter directly after the setup that wraps its SOL,
/// in a market with no whale. Returns the SVM, the searcher's wrapped SOL before the route, and
/// what the route did.
fn send_edited_round_trip(
    snapshot: &Snapshot,
    edit: impl FnOnce(&mut LiteSVM, &mut Instruction),
) -> (LiteSVM, u64, Result<Outcome, Failure>) {
    let route = snapshot.route(ROUTE);
    let first = &route.legs[0];
    let searcher = wallet::wallet();
    let mut svm = market(snapshot, None);
    tx::send(&mut svm, &searcher, &[], &first.instructions.setup, &[])
        .unwrap_or_else(|failure| panic!("wrapping the SOL failed: {failure:?}"));
    let wsol_before = token_balance(&svm, &first.source_token_account);
    let mut instruction = round_trip(route);
    edit(&mut svm, &mut instruction);
    let result = tx::send(
        &mut svm,
        &searcher,
        &[],
        &[instruction],
        &route.lookup_tables(),
    );
    (svm, wsol_before, result)
}

/// Jupiter does not check that `route`'s source and destination are the accounts its steps move:
/// only that the source holds at least `in_amount`, of any mint, and the destination holds the
/// destination mint. Another wallet's wrapped SOL, holding exactly `in_amount`, at both positions:
/// the route runs and moves the searcher's own wrapped SOL, not the decoy, which is left
/// untouched. So the two positions prove nothing about what moved; the template measures the
/// account itself.
#[test]
fn a_decoy_at_the_source_and_destination_is_left_untouched() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let route = snapshot.route(ROUTE);
    let first = &route.legs[0];
    let in_amount = first.route.in_amount;
    let searcher_wsol = first.source_token_account;
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();
    // What the round trip does to the searcher's own wsol with no decoy in place, so this can
    // assert the exact change rather than merely that some change happened.
    let undecoyed_change = realised(&mut market(&snapshot, None), route).wrapped_sol;

    let decoy = associated_token_address(&other, &WSOL_MINT);
    let (svm, wsol_before, result) = send_edited_round_trip(&snapshot, |svm, round_trip| {
        token_account(svm, &other, &WSOL_MINT, in_amount);
        round_trip.accounts[SOURCE].pubkey = decoy;
        round_trip.accounts[DESTINATION].pubkey = decoy;
    });
    result.unwrap_or_else(|failure| panic!("the decoyed round trip failed: {failure:?}"));
    assert_eq!(
        token_balance(&svm, &decoy),
        in_amount,
        "the decoy must be left untouched"
    );
    assert_eq!(
        i128::from(token_balance(&svm, &searcher_wsol)) - i128::from(wsol_before),
        undecoyed_change,
        "the decoy must not change what the round trip does to the searcher's own wsol"
    );
}

/// The source's mint is not checked either: another wallet's USDC account, holding `in_amount`,
/// does as well as wrapped SOL, if there is enough.
#[test]
fn the_source_s_mint_is_not_checked() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let route = snapshot.route(ROUTE);
    let first = &route.legs[0];
    let (in_amount, usdc_mint) = (first.route.in_amount, first.output_mint);
    let searcher_wsol = first.source_token_account;
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();
    let undecoyed_change = realised(&mut market(&snapshot, None), route).wrapped_sol;

    let usdc_decoy = associated_token_address(&other, &usdc_mint);
    let (svm, wsol_before, result) = send_edited_round_trip(&snapshot, |svm, round_trip| {
        token_account(svm, &other, &usdc_mint, in_amount);
        round_trip.accounts[SOURCE].pubkey = usdc_decoy;
    });
    result.unwrap_or_else(|failure| panic!("the USDC-sourced round trip failed: {failure:?}"));
    assert_eq!(
        token_balance(&svm, &usdc_decoy),
        in_amount,
        "the decoy must be left untouched"
    );
    assert_eq!(
        i128::from(token_balance(&svm, &searcher_wsol)) - i128::from(wsol_before),
        undecoyed_change,
        "the decoy must not change what the round trip does to the searcher's own wsol"
    );
}

/// The source's balance is checked: one unit short of `in_amount` is refused. The code is not in
/// Jupiter's published IDL.
#[test]
fn a_source_one_unit_short_of_in_amount_fails() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let jupiter = snapshot.named("jupiter");
    let in_amount = snapshot.route(ROUTE).legs[0].route.in_amount;
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();

    let (_, _, result) = send_edited_round_trip(&snapshot, |svm, round_trip| {
        round_trip.accounts[SOURCE].pubkey = token_account(svm, &other, &WSOL_MINT, in_amount - 1);
    });
    let failure = result.expect_err("a source one unit short of in_amount must fail");
    assert_eq!((failure.program, failure.code), (jupiter, Some(6024)));
}

/// A destination that does not hold the destination mint is refused: the searcher's own USDC
/// account there instead.
#[test]
fn a_destination_not_holding_the_destination_mint_fails() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let jupiter = snapshot.named("jupiter");
    let first = &snapshot.route(ROUTE).legs[0];

    let (_, _, result) = send_edited_round_trip(&snapshot, |_, round_trip| {
        round_trip.accounts[DESTINATION].pubkey = first.destination_token_account;
    });
    let failure = result.expect_err("a destination not holding the destination mint must fail");
    assert_eq!((failure.program, failure.code), (jupiter, Some(6019)));
}

/// So is a destination mint that is not the destination's: the USDC mint, with wrapped SOL still
/// at `destinationAta`.
#[test]
fn a_destination_mint_that_does_not_match_the_destination_fails() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let jupiter = snapshot.named("jupiter");
    let usdc_mint = snapshot.route(ROUTE).legs[0].output_mint;

    let (_, _, result) = send_edited_round_trip(&snapshot, |_, round_trip| {
        round_trip.accounts[DESTINATION_MINT].pubkey = usdc_mint;
    });
    let failure = result.expect_err("a mismatched destination mint must fail");
    assert_eq!((failure.program, failure.code), (jupiter, Some(6019)));
}

/// The required pass: a whale's sale leaves the Raydium pool cheap, the searcher's round trip
/// takes the difference, and the tip is paid out of it. Tip and edge add up to exactly the profit,
/// the most the requirement lets through.
#[test]
fn a_backrun_pays_the_tip_out_of_its_profit() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let accounts = Accounts::of(&snapshot);
    let profit = backrun_profit(&snapshot, WHALE_SALE);
    let tip = profit_less_edge(profit);
    assert!(tip >= MINIMUM_TIP, "a profit of {profit} cannot pay a tip");

    let backrun = run_tip(
        &snapshot,
        example,
        Some(WHALE_SALE),
        |_| {},
        &accounts,
        tip,
        EDGE,
    );
    let outcome = backrun
        .result
        .unwrap_or_else(|failure| panic!("the backrun failed: {failure:?}"));
    println!(
        "paid a {tip}-lamport tip: {} CU in all, {} in the run; {} bytes",
        outcome.compute_units,
        run_units(&outcome.logs),
        outcome.size
    );

    assert_eq!(
        balance(&backrun.svm, &accounts.tip),
        backrun.tip_before + tip,
        "the tip account must receive exactly the tip"
    );
    // The cleanup closed the wrapped SOL account into the searcher's lamports, profit included:
    // the searcher kept the edge, less the fee.
    assert_eq!(
        backrun.svm.get_account(&accounts.wsol),
        None,
        "the cleanup must close the wrapped SOL account"
    );
    assert_eq!(
        balance(&backrun.svm, &accounts.searcher) + outcome.fee,
        backrun.searcher_before + EDGE,
        "the searcher must keep exactly the edge, less the fee"
    );
    // The route spent all the USDC it bought.
    let usdc = snapshot.route(ROUTE).legs[0].destination_token_account;
    assert_eq!(
        token_balance(&backrun.svm, &usdc),
        0,
        "the route must spend all the USDC it bought"
    );
}

/// The required failure: one lamport more than the profit covers, and the run stops at the
/// requirement with nothing paid.
#[test]
fn a_tip_above_the_profit_fails_at_the_requirement() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let accounts = Accounts::of(&snapshot);
    let profit = backrun_profit(&snapshot, WHALE_SALE);
    let tip = profit_less_edge(profit) + 1;

    let backrun = run_tip(
        &snapshot,
        example,
        Some(WHALE_SALE),
        |_| {},
        &accounts,
        tip,
        EDGE,
    );
    let failure = backrun
        .result
        .expect_err("a tip one lamport above the profit must fail at profitCoversTheTip");
    println!(
        "refused a {tip}-lamport tip after {} CU in the run",
        run_units(&failure.logs)
    );

    tx::assert_requirement_failed(&failure, example, "profitCoversTheTip");
    assert_eq!(balance(&backrun.svm, &accounts.tip), backrun.tip_before);
    assert_eq!(
        balance(&backrun.svm, &accounts.searcher),
        backrun.searcher_before - failure.fee
    );
}

/// With no whale the round trip loses to the pools' fees. Measured by subtracting, the loss would
/// underflow and fail with ArithmeticOverflow before the requirement; compared by adding, it fails
/// at the requirement like any profit too thin for the tip.
#[test]
fn a_losing_round_trip_fails_at_the_requirement_rather_than_underflowing() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let accounts = Accounts::of(&snapshot);
    let loss = realised(&mut market(&snapshot, None), snapshot.route(ROUTE)).wrapped_sol;
    println!("{ROUTE} at the snapshot's prices realises {loss} lamports");
    assert!(loss < 0, "{loss}");

    let backrun = run_tip(&snapshot, example, None, |_| {}, &accounts, MINIMUM_TIP, 0);
    let failure = backrun
        .result
        .expect_err("a losing round trip must fail at profitCoversTheTip, not underflow");
    println!(
        "refused a {MINIMUM_TIP}-lamport tip on a loss after {} CU in the run",
        run_units(&failure.logs)
    );

    tx::assert_requirement_failed(&failure, example, "profitCoversTheTip");
    assert_eq!(balance(&backrun.svm, &accounts.tip), backrun.tip_before);
}

/// The measured account must be the searcher's wrapped SOL: the searcher's own USDC account there
/// instead fails before the route runs, so the profit would never have been in lamports.
#[test]
fn the_wsol_account_must_hold_wrapped_sol() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let route = snapshot.route(ROUTE);
    let usdc = Accounts {
        wsol: route.legs[0].destination_token_account,
        ..Accounts::of(&snapshot)
    };

    let backrun = run_tip(&snapshot, example, None, |_| {}, &usdc, MINIMUM_TIP, 0);
    let failure = backrun
        .result
        .expect_err("a USDC account as wsolAccount must fail wsolAccountHoldsWrappedSol");
    tx::assert_requirement_failed(&failure, example, "wsolAccountHoldsWrappedSol");
}

/// The measured account must be the searcher's own: another wallet's wrapped SOL fails before the
/// route runs, so its profit would never reach the searcher, who pays the tip.
#[test]
fn the_wsol_account_must_be_the_searcher_s() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();
    let theirs = Accounts {
        wsol: associated_token_address(&other, &WSOL_MINT),
        ..Accounts::of(&snapshot)
    };

    let backrun = run_tip(
        &snapshot,
        example,
        None,
        |svm| {
            token_account(svm, &other, &WSOL_MINT, SOL);
        },
        &theirs,
        MINIMUM_TIP,
        0,
    );
    let failure = backrun
        .result
        .expect_err("another wallet's wrapped SOL must fail searcherOwnsTheWsolAccount");
    tx::assert_requirement_failed(&failure, example, "searcherOwnsTheWsolAccount");
}

/// The tip must go to an account of Jito's Tip Payment program: a wallet in its place fails its
/// owner constraint before the route runs.
#[test]
fn the_tip_account_must_belong_to_the_tip_payment_program() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();
    let wallet_tip = Accounts {
        tip: other,
        ..Accounts::of(&snapshot)
    };

    let backrun = run_tip(
        &snapshot,
        example,
        None,
        |svm| fund(svm, &other, SOL),
        &wallet_tip,
        MINIMUM_TIP,
        0,
    );
    let failure = backrun
        .result
        .expect_err("a wallet as the tip account must fail its owner constraint");
    let tip_index = example
        .fixed_accounts
        .iter()
        .position(|name| name == "jitoTip")
        .unwrap();
    assert_eq!(
        tx::ballista_error(&failure),
        Some(("AccountConstraintFailed", u16::try_from(tip_index).unwrap())),
        "{failure:?}"
    );
}

/// Setting the round trip's own `quoted_out_amount` to `in_amount` at zero slippage bps makes
/// Jupiter's slippage check require the last step's gross output to reach `in_amount`, which for
/// this round trip, whose source is its destination, comes down to requiring no loss. The round
/// trip loses to the pools' fees, so it fails there with Jupiter's own code, not the template's
/// label (`findings/jito-tip.md`).
#[test]
fn jupiters_own_slippage_check_can_reject_a_loss() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let jupiter = snapshot.named("jupiter");
    let route = snapshot.route(ROUTE);
    let in_amount = route.legs[0].route.in_amount;
    let (_, _, result) = send_edited_round_trip(&snapshot, |_, instruction| {
        // The data's tail, built by `round_trip`: `in_amount` (8 bytes), `quoted_out_amount` (8
        // bytes), `slippage_bps` (2 bytes), `platform_fee_bps` (1 byte).
        let len = instruction.data.len();
        instruction.data[len - 11..len - 3].copy_from_slice(&in_amount.to_le_bytes());
        instruction.data[len - 3..len - 1].copy_from_slice(&0u16.to_le_bytes());
    });
    let failure = result.expect_err("quoting no slippage on a losing round trip must fail");
    assert_eq!((failure.program, failure.code), (jupiter, Some(6001)));
}

/// Reprints the measurements `findings/jito-tip.md` quotes for slot 451,100,151: the round trip
/// alone, the whale's sale, and the round trip's profit after whale sales of sizes other than the
/// one the required tests use. Not a correctness check; re-run after a snapshot refresh and update
/// the numbers the doc quotes.
#[test]
#[ignore = "prints findings/jito-tip.md's measurements rather than asserting one"]
fn measure_the_findings_numbers() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let route = snapshot.route(ROUTE);

    // "Alone (compute budget and `route`), it costs 88,457 CU and 710 bytes."
    let realised = realised(&mut market(&snapshot, None), route);
    println!(
        "the round trip alone costs {} CU and {} bytes",
        realised.outcome.compute_units, realised.outcome.size
    );

    // "A 500 SOL sale costs 165,219 CU."
    let mut svm = snapshot.svm();
    let sale = whale_sells_sol(&mut svm, &snapshot, WHALE_SALE);
    println!(
        "the whale's {WHALE_SALE}-lamport sale costs {} CU",
        sale.compute_units
    );

    // The round trip's profit after whale sales of sizes other than the one the required tests use.
    for whale_sale in [100 * SOL, 300 * SOL] {
        let profit = backrun_profit(&snapshot, whale_sale);
        println!("a {whale_sale}-lamport whale sale leaves the round trip a profit of {profit}");
    }
}

/// The joined round trip's platform fee rate and account are the run's builder's choice, as they
/// are any route's: nothing ties either to the searcher. The template caps the round trip's
/// `platformFeeBps` at `MAX_PLATFORM_FEE_BPS`, 0, so after the whale's sale a 0.1% fee paid to an
/// attacker's wrapped SOL account, at 2% slippage, fails at `platformFeeWithinCap` before Jupiter
/// runs, and no tip is paid. `findings/platform-fee.md`.
#[test]
fn a_hostile_platform_fee_fails_at_platform_fee_within_cap() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[EXAMPLE];
    let accounts = Accounts::of(&snapshot);
    let mut svm = market(&snapshot, Some(WHALE_SALE));
    let template = upload_template(&mut svm, example);
    let attacker = keypair(b"ballista-protocol-tests-attacker").pubkey();
    let attacker_wsol = token_account(&mut svm, &attacker, &WSOL_MINT, 0);
    let tip_before = balance(&svm, &accounts.tip);

    // `route`'s data ends in `slippage_bps` (u16) and `platform_fee_bps` (u8).
    let mut hostile = round_trip(snapshot.route(ROUTE));
    let at = hostile.data.len() - 3;
    hostile.data[at..at + 2].copy_from_slice(&200u16.to_le_bytes());
    hostile.data[at + 2] = 10;
    hostile.accounts[PLATFORM_FEE_ACCOUNT] = AccountMeta::new(attacker_wsol, false);

    let run = tip_run_of(
        &snapshot,
        template,
        example,
        &accounts,
        MINIMUM_TIP,
        0,
        &hostile,
    );
    let failure = send_backrun(&mut svm, &snapshot, run)
        .expect_err("a platform fee above the cap should fail");
    assert_requirement_failed(&failure, example, "platformFeeWithinCap");
    assert!(
        !jupiter_ran(&snapshot.named("jupiter"), &failure),
        "{failure:?}"
    );
    assert_eq!(token_balance(&svm, &attacker_wsol), 0);
    assert_eq!(balance(&svm, &accounts.tip), tip_before);
    println!(
        "refused a 10 bps fee at platformFeeWithinCap after {} CU in the run",
        run_units(&failure.logs)
    );
}
