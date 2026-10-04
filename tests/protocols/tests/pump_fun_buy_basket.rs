//! `pumpFunBuyBasket` against pump.fun's real bonding-curve program: three coins bought in one run,
//! one of them in mayhem mode, with the total checked against a budget.

use {
    ballista_protocol_tests::{
        pump::{
            self, Coin, Trader, AVJ1, AVYG, BFX4, BONDING_CURVE_COMPLETE, BUYBACK_FEE_RECIPIENT,
            EVENT_AUTHORITY, FEE_CONFIG, GLOBAL, GLOBAL_VOLUME_ACCUMULATOR, HJXC, PUMP, PUMP_FEES,
            TOKEN, TOKEN_2022_PROGRAM, TOO_MUCH_SOL_REQUIRED,
        },
        template::{examples, upload, Example, Run},
        tx::{self, ballista_error, Failure, Outcome, PACKET_DATA_SIZE},
        wallet::{fund, keypair, SOL},
    },
    ballista_sdk::SYSTEM_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::Instruction,
    solana_message::AddressLookupTableAccount,
    solana_signer::Signer,
};

const EXAMPLE: &str = "pumpFunBuyBasket";

/// Anchor's own `ConstraintTokenMint`, from its framework-wide range: pump.fun raises it when the
/// account it pays holds another mint.
const ANCHOR_CONSTRAINT_TOKEN_MINT: u32 = 2014;

/// Every run here sets this compute limit: one buy costs pump.fun about 75,000 units, so the
/// default 200,000 covers only two.
const COMPUTE_LIMIT: u32 = 1_400_000;

/// The transaction-wide limit on instructions, the run's own and every call it makes
/// (`MAX_INSTRUCTION_TRACE_LENGTH`).
const MAX_INSTRUCTION_TRACE_LENGTH: usize = 64;

/// The snapshot, a buyer with an empty token account for each coin, the template, and the buyer's
/// lookup table of the accounts every basket shares.
struct Setup {
    svm: LiteSVM,
    buyer: Trader,
    template: Address,
    table: AddressLookupTableAccount,
}

fn setup(example: &Example) -> Setup {
    let mut svm = pump::svm();
    let buyer = pump::trader(
        &mut svm,
        "basket buyer",
        10 * SOL,
        &[HJXC, AVYG, AVJ1, BFX4],
    );
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = upload(&mut svm, &creator, 1, &example.payload);
    let shared = shared_accounts(template, &buyer.address());
    let table = pump::create_lookup_table(&mut svm, &buyer.keypair, &shared);
    Setup {
        svm,
        buyer,
        template,
        table,
    }
}

/// One row: what to buy, and where the coins go.
#[derive(Clone, Copy)]
struct Buy {
    coin: Coin,
    token_account: Address,
    amount: u64,
    max_sol_cost: u64,
}

impl Setup {
    /// A row buying `amount` of `mint` into the buyer's own account, for at most 1 SOL.
    fn buy(&self, mint: Address, amount: u64) -> Buy {
        Buy {
            coin: pump::coin(&self.svm, mint),
            token_account: self.buyer.token_account(&mint),
            amount,
            max_sol_cost: SOL,
        }
    }

    fn lamports(&self) -> u64 {
        pump::lamports(&self.svm, &self.buyer.address())
    }

    fn holdings(&self, rows: &[Buy]) -> Vec<u64> {
        rows.iter()
            .map(|row| pump::token_balance(&self.svm, &row.token_account))
            .collect()
    }
}

/// The run for `rows`, signed by `buyer`.
fn basket(
    template: Address,
    example: &Example,
    buyer: Address,
    rows: &[Buy],
    budget: u64,
) -> Instruction {
    let mut run = Run::new(template, example)
        .account("pumpProgram", PUMP, false, false)
        .account("global", GLOBAL, false, false)
        .account("buyer", buyer, true, true)
        .account("systemProgram", SYSTEM_PROGRAM_ID, false, false)
        .account("tokenProgram", TOKEN_2022_PROGRAM, false, false)
        .account("eventAuthority", EVENT_AUTHORITY, false, false)
        .account(
            "globalVolumeAccumulator",
            GLOBAL_VOLUME_ACCUMULATOR,
            true,
            false,
        )
        .account(
            "userVolumeAccumulator",
            pump::user_volume_accumulator(&buyer),
            true,
            false,
        )
        .account("feeConfig", FEE_CONFIG, false, false)
        .account("feeProgram", PUMP_FEES, false, false)
        .account("buybackFeeRecipient", BUYBACK_FEE_RECIPIENT, true, false)
        .input_u64("budget", budget);
    for row in rows {
        run = run.row(|bound| {
            bound
                .account("mint", row.coin.mint, false, false)
                .account("bondingCurve", row.coin.bonding_curve, true, false)
                .account(
                    "curveTokenAccount",
                    row.coin.curve_token_account,
                    true,
                    false,
                )
                .account("buyerTokenAccount", row.token_account, true, false)
                .account("creatorVault", row.coin.creator_vault, true, false)
                .account("bondingCurveV2", row.coin.bonding_curve_v2, false, false)
                .account("feeRecipient", row.coin.fee_recipient, true, false)
                .input_u64("amount", row.amount)
                .input_u64("maxSolCost", row.max_sol_cost)
        });
    }
    run.build()
}

/// Sends the basket, signed by the setup's buyer, after a compute-budget instruction, looking keys
/// up in the buyer's table of shared accounts.
fn send(
    setup: &mut Setup,
    example: &Example,
    rows: &[Buy],
    budget: u64,
) -> Result<Outcome, Failure> {
    let run = basket(setup.template, example, setup.buyer.address(), rows, budget);
    let limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_LIMIT);
    let buyer = setup.buyer.keypair.insecure_clone();
    let table = setup.table.clone();
    tx::send(&mut setup.svm, &buyer, &[], &[limit, run], &[table])
}

/// How many instructions a transaction ran, its own and every call: one `invoke` line each.
fn trace_length(logs: &[String]) -> usize {
    logs.iter()
        .filter(|line| line.contains(" invoke ["))
        .count()
}

/// pump.fun's calls into itself at depth 2, one per buy the run made.
fn buys_made(logs: &[String]) -> usize {
    logs.iter()
        .filter(|line| *line == &format!("Program {PUMP} invoke [2]"))
        .count()
}

/// One row of each live coin: two ordinary coins and one in mayhem mode.
fn three_coins(setup: &Setup) -> Vec<Buy> {
    vec![
        setup.buy(HJXC, 2_000_000 * TOKEN),
        setup.buy(AVYG, 1_000_000 * TOKEN),
        setup.buy(AVJ1, 500_000 * TOKEN),
    ]
}

/// What `rows` cost bought one by one with pump.fun's own `buy`, outside any template, from a fresh
/// copy of the same setup: each buy's lamports, transaction fees left out.
fn bought_directly(example: &Example, rows: &[Buy]) -> Vec<u64> {
    let mut twin = setup(example);
    rows.iter()
        .map(|row| pump::buy_directly(&mut twin.svm, &twin.buyer, &row.coin, row.amount))
        .collect()
}

#[test]
fn a_basket_within_budget_buys_every_coin_at_pumps_own_price() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let rows = three_coins(&setup);
    let costs = bought_directly(example, &rows);
    let total: u64 = costs.iter().sum();
    let before = setup.lamports();

    // A budget of exactly the total lands.
    let outcome =
        send(&mut setup, example, &rows, total).unwrap_or_else(|failure| panic!("{failure:?}"));

    let amounts: Vec<u64> = rows.iter().map(|row| row.amount).collect();
    assert_eq!(
        setup.holdings(&rows),
        amounts,
        "every coin reached the buyer"
    );
    // The buyer paid the transaction fee, and exactly what the three buys cost on their own.
    assert_eq!(before - setup.lamports() - outcome.fee, total);
    assert_eq!(buys_made(&outcome.logs), 3, "one buy per row");
    println!(
        "three coins: {} CU, {} of them Ballista's own, {} bytes; costs {costs:?}, {total} lamports in all",
        outcome.compute_units,
        outcome.own_compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.size
    );
}

#[test]
fn a_basket_over_budget_fails_at_within_budget_and_buys_nothing() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let rows = three_coins(&setup);
    let total: u64 = bought_directly(example, &rows).iter().sum();
    let before = setup.lamports();

    let failure = send(&mut setup, example, &rows, total - 1).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "withinBudget");
    assert_eq!(
        setup.holdings(&rows),
        [0, 0, 0],
        "the first two buys reverted too"
    );
    assert_eq!(setup.lamports(), before - failure.fee);
}

/// The budget is checked after each buy, so a basket stops at the first coin that takes it over.
#[test]
fn a_basket_stops_at_the_first_coin_over_budget() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let rows = three_coins(&setup);
    let costs = bought_directly(example, &rows);

    let failure = send(&mut setup, example, &rows, costs[0]).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "withinBudget");
    assert_eq!(
        buys_made(&failure.logs),
        2,
        "the third coin was never bought"
    );
}

/// pump.fun checks each row's `maxSolCost` itself, against the price and its fees.
#[test]
fn a_row_over_its_max_sol_cost_fails_in_pump() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let mut rows = three_coins(&setup);
    let costs = bought_directly(example, &rows);
    // One lamport short of what the second coin costs.
    rows[1].max_sol_cost = costs[1] - 1;

    let failure = send(&mut setup, example, &rows, 10 * SOL).unwrap_err();

    pump::assert_pump_error(&failure, TOO_MUCH_SOL_REQUIRED);
    assert_eq!(ballista_error(&failure), None);
    assert_eq!(setup.holdings(&rows), [0, 0, 0]);
}

#[test]
fn a_graduated_coin_fails_at_curve_not_graduated() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let graduated = setup.buy(BFX4, 1_000 * TOKEN);
    assert!(pump::curve(&setup.svm, &graduated.coin).complete);
    let rows = [setup.buy(HJXC, 1_000_000 * TOKEN), graduated];

    let failure = send(&mut setup, example, &rows, 10 * SOL).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "curveNotGraduated");
    assert_eq!(
        setup.holdings(&rows),
        [0, 0],
        "the first coin's buy reverted too"
    );

    // pump.fun alone refuses the same buy with its own error.
    let alone = pump::buy(
        &setup.buyer.address(),
        &graduated.token_account,
        &graduated.coin,
        graduated.amount,
        SOL,
    );
    let buyer = setup.buyer.keypair.insecure_clone();
    let failure = tx::send(&mut setup.svm, &buyer, &[], &[alone], &[]).unwrap_err();
    pump::assert_pump_error(&failure, BONDING_CURVE_COMPLETE);
}

/// Security: pump.fun pays whichever token account of the mint a buy names. Alone, it pays a
/// stranger's; in the template, a row naming one fails before anything is bought.
#[test]
fn coins_must_go_to_the_buyer_not_a_strangers_account() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let stranger = pump::trader(&mut setup.svm, "basket stranger", SOL, &[AVYG]);
    let mut rows = three_coins(&setup);
    rows[1].token_account = stranger.token_account(&AVYG);

    let failure = send(&mut setup, example, &rows, 10 * SOL).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "tokensGoToTheBuyer");
    assert_eq!(pump::token_balance(&setup.svm, &rows[1].token_account), 0);

    // The same buy, sent to pump.fun alone, lands and pays the stranger.
    let alone = pump::buy(
        &setup.buyer.address(),
        &rows[1].token_account,
        &rows[1].coin,
        rows[1].amount,
        SOL,
    );
    let buyer = setup.buyer.keypair.insecure_clone();
    tx::send(&mut setup.svm, &buyer, &[], &[alone], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        pump::token_balance(&setup.svm, &rows[1].token_account),
        rows[1].amount
    );
}

/// The buyer's own account of another coin: the template's owner check passes, and pump.fun refuses
/// the mint.
#[test]
fn a_token_account_of_another_coin_fails_in_pump() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let mut rows = three_coins(&setup);
    rows[0].token_account = setup.buyer.token_account(&AVYG);

    let failure = send(&mut setup, example, &rows, 10 * SOL).unwrap_err();

    pump::assert_pump_error(&failure, ANCHOR_CONSTRAINT_TOKEN_MINT);
    assert_eq!(ballista_error(&failure), None);
}

/// The accounts every basket names, whatever its coins: pump.fun's own, the fee recipients, the
/// template and the buyer's volume record.
fn shared_accounts(template: Address, buyer: &Address) -> Vec<Address> {
    vec![
        template,
        PUMP,
        GLOBAL,
        SYSTEM_PROGRAM_ID,
        TOKEN_2022_PROGRAM,
        EVENT_AUTHORITY,
        GLOBAL_VOLUME_ACCUMULATOR,
        FEE_CONFIG,
        PUMP_FEES,
        BUYBACK_FEE_RECIPIENT,
        pump::FEE_RECIPIENT,
        pump::RESERVED_FEE_RECIPIENT,
        pump::user_volume_accumulator(buyer),
    ]
}

/// A row's own accounts, all but its fee recipient, which [`shared_accounts`] holds.
fn row_accounts(row: &Buy) -> [Address; 6] {
    [
        row.coin.mint,
        row.coin.bonding_curve,
        row.coin.curve_token_account,
        row.token_account,
        row.coin.creator_vault,
        row.coin.bonding_curve_v2,
    ]
}

/// A row of a made-up coin, `index`: distinct addresses, for measuring size only.
fn made_up_row(setup: &Setup, index: u8) -> Buy {
    let mint = Address::new_from_array([index; 32]);
    let pda = |seeds: &[&[u8]]| Address::find_program_address(seeds, &PUMP).0;
    let bonding_curve = pda(&[b"bonding-curve", mint.as_ref()]);
    Buy {
        coin: Coin {
            mint,
            bonding_curve,
            curve_token_account: pump::associated_token_address(&bonding_curve, &mint),
            creator_vault: pda(&[b"creator-vault", &[index; 32]]),
            bonding_curve_v2: pda(&[b"bonding-curve-v2", mint.as_ref()]),
            fee_recipient: pump::FEE_RECIPIENT,
        },
        token_account: setup.buyer.token_account(&mint),
        amount: 1_000_000 * TOKEN,
        max_sol_cost: SOL,
    }
}

/// The wire size of a basket of `rows` distinct coins after a compute-budget instruction, looking
/// keys up in `tables`.
fn size_of(
    setup: &Setup,
    example: &Example,
    rows: usize,
    tables: &[AddressLookupTableAccount],
) -> usize {
    let rows: Vec<Buy> = (1..=rows as u8)
        .map(|index| made_up_row(setup, index))
        .collect();
    let run = basket(setup.template, example, setup.buyer.address(), &rows, SOL);
    let limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_LIMIT);
    tx::wire_size(&tx::transaction(
        &setup.svm,
        &setup.buyer.keypair,
        &[],
        &[limit, run],
        tables,
    ))
}

/// Two coins need neither a lookup table nor a compute-budget instruction: a plain legacy
/// transaction under the default 200,000-unit limit.
#[test]
fn two_coins_fit_a_plain_transaction() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let rows = [
        setup.buy(HJXC, 2_000_000 * TOKEN),
        setup.buy(AVYG, 1_000_000 * TOKEN),
    ];
    let run = basket(setup.template, example, setup.buyer.address(), &rows, SOL);
    let buyer = setup.buyer.keypair.insecure_clone();

    let outcome = tx::send(&mut setup.svm, &buyer, &[], &[run], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(
        setup.holdings(&rows),
        [2_000_000 * TOKEN, 1_000_000 * TOKEN]
    );
    assert!(outcome.compute_units < 200_000);
    println!(
        "two coins, no table, no budget: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}

/// Distinct coins per transaction: two without a lookup table, four with the shared accounts in
/// one, and all seven rows the template allows with every account in one. Each coin adds 215 bytes
/// when its accounts are not in a table, and 29 when they are.
#[test]
fn how_many_coins_fit_one_transaction() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let setup = setup(example);
    let shared = [setup.table.clone()];
    let mut everything = setup.table.clone();
    for index in 1..=7 {
        everything
            .addresses
            .extend(row_accounts(&made_up_row(&setup, index)));
    }
    let everything = [everything];
    let fits = |rows: usize, tables: &[AddressLookupTableAccount]| {
        size_of(&setup, example, rows, tables) <= PACKET_DATA_SIZE
    };
    assert!(fits(2, &[]) && !fits(3, &[]));
    assert!(fits(4, &shared) && !fits(5, &shared));
    assert!(fits(7, &everything));
    println!(
        "bytes: 2 rows without a table {}; 4 rows with the shared table {}; 7 rows with every \
         account in a table {}",
        size_of(&setup, example, 2, &[]),
        size_of(&setup, example, 4, &shared),
        size_of(&setup, example, 7, &everything),
    );
}

/// Seven rows, the template's most, land in one transaction with every account in lookup tables.
/// Their calls fill the transaction's instruction trace to within an eighth buy of its limit. The
/// three live coins are bought over and over.
#[test]
fn seven_rows_land_and_fill_the_instruction_trace() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let three = three_coins(&setup);
    let rows: Vec<Buy> = three
        .iter()
        .cycle()
        .take(7)
        .map(|row| Buy {
            amount: 100_000 * TOKEN,
            ..*row
        })
        .collect();
    // The coins' accounts, in a second table: a table's address comes from its authority and a
    // slot, and LiteSVM knows one slot.
    let keeper = keypair(&pump::seed("basket table keeper"));
    fund(&mut setup.svm, &keeper.pubkey(), SOL);
    let coins: Vec<Address> = three.iter().flat_map(row_accounts).collect();
    let coin_table = pump::create_lookup_table(&mut setup.svm, &keeper, &coins);

    let run = basket(
        setup.template,
        example,
        setup.buyer.address(),
        &rows,
        10 * SOL,
    );
    let limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_LIMIT);
    let buyer = setup.buyer.keypair.insecure_clone();
    let tables = [setup.table.clone(), coin_table];
    let outcome = tx::send(&mut setup.svm, &buyer, &[], &[limit, run], &tables)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(buys_made(&outcome.logs), 7);
    assert_eq!(
        setup.holdings(&three),
        [300_000 * TOKEN, 200_000 * TOKEN, 200_000 * TOKEN]
    );
    let trace = trace_length(&outcome.logs);
    let per_buy = (trace - 2) / 7;
    assert!(
        trace + per_buy > MAX_INSTRUCTION_TRACE_LENGTH,
        "{trace} instructions: an eighth buy would fit, so the template could allow one"
    );
    println!(
        "seven rows: {} CU, {} of them Ballista's own, {} bytes, {trace} instructions in the trace",
        outcome.compute_units,
        outcome.own_compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.size
    );
}
