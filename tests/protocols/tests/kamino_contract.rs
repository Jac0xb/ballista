//! What Kamino Lend requires of a caller, against the deployed program. Each test pins a rule one
//! of the templates once broke.

use {
    ballista_protocol_tests::{
        kamino,
        lending::{self, MARKET, SCOPE_PRICES, SOL_RESERVE, USDC_MINT, USDC_RESERVE},
        template,
        tx::{self, Failure},
        wallet::{self, SOL, WSOL_MINT},
    },
    ballista_sdk::{
        ballista_common::template::{
            ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_BYTES, VALUE_BYTES,
        },
        run_instruction, ProgramBuilder, RunInputs, Segment,
    },
    klend_interface::LendingError,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

/// What each test's user holds in USDC before the transaction under test.
const USDC: u64 = 100_000_000;
const DEPOSIT: u64 = 10_000_000;

/// The smallest Ballista caller: it forwards the `data` input to `target`, with its signer first
/// and its one account group after.
fn forward_template(target: Address) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(target.to_bytes()), None, 0);
    let signer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    builder.account_groups(1);
    let data_input = builder.input(VALUE_BYTES, 512);
    let data = builder.load_input(data_input);
    let cpi = builder.cpi_with_group(
        program,
        &[(signer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)],
        &[Segment::Register(DATA_REG_BYTES, data)],
        0,
    );
    builder.set_cpi_max_data_len(cpi, 512);
    builder.invoke(cpi, None);
    builder.build().expect("the forwarding template builds")
}

/// `call` run through the forwarding template: its first account signs through the declared
/// slot, and the rest travel as the group with their own writable flags.
fn forward(template: Address, call: &Instruction) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(call.program_id, false),
        AccountMeta::new(call.accounts[0].pubkey, true),
    ];
    accounts.extend(call.accounts[1..].iter().map(|meta| AccountMeta {
        is_signer: false,
        ..meta.clone()
    }));
    let inputs = RunInputs::new()
        .groups(&[(call.accounts.len() - 1) as u8])
        .bytes(&call.data)
        .finish();
    run_instruction(template, accounts, &inputs)
}

/// Asserts that klend's Anchor framework refused: an instruction given fewer accounts than it
/// declares.
#[track_caller]
fn refused_for_too_few_accounts(failure: &Failure) {
    assert_eq!(
        (failure.program, failure.code),
        (kamino::KLEND, Some(kamino::ACCOUNT_NOT_ENOUGH_KEYS)),
        "{failure:?}"
    );
}

/// A user (seed `ballista-protocol-tests-usdc-usr`) holding 100 USDC (rule 1), with an obligation
/// and its user state in each of `farmed`'s collateral farms, one slot after setup.
struct User {
    owner: Keypair,
    usdc: Address,
    obligation: Address,
}

fn user(svm: &mut LiteSVM, farmed: &[Address]) -> User {
    let owner = wallet::keypair(b"ballista-protocol-tests-usdc-usr");
    wallet::fund(svm, &owner.pubkey(), 10 * SOL);
    let usdc = wallet::token_account(svm, &owner.pubkey(), &USDC_MINT, USDC);
    let obligation = lending::open_obligation(svm, &owner, farmed);
    lending::next_slot(svm);
    User {
        owner,
        usdc,
        obligation,
    }
}

/// `instructions` behind the compute limit, signed by `user`.
fn send(svm: &mut LiteSVM, user: &User, instructions: Vec<Instruction>) -> Result<(), Failure> {
    let mut all = vec![lending::compute_limit()];
    all.extend(instructions);
    tx::send(svm, &user.owner, &[], &all, &[]).map(|_| ())
}

#[test]
fn v1_handlers_refuse_a_ballista_caller_and_v2_handlers_do_not() {
    let mut svm = lending::svm();
    let creator = wallet::keypair(lending::CREATOR_SEED);
    wallet::fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = template::upload(&mut svm, &creator, 9, &forward_template(kamino::KLEND));
    let user = user(&mut svm, &[USDC_RESERVE]);
    let o = user.owner.pubkey();

    let v1 = kamino::deposit_v1(
        &svm,
        &o,
        &user.obligation,
        &USDC_RESERVE,
        &user.usdc,
        DEPOSIT,
    );
    let mut instructions = kamino::refreshes(&svm, &user.obligation, &[USDC_RESERVE]);
    instructions.push(forward(template, &v1));
    let failure = send(&mut svm, &user, instructions).unwrap_err();
    kamino::refused(&failure, LendingError::CpiDisabled);
    assert_eq!(wallet::token_balance(&svm, &user.usdc), USDC);

    let v2 = kamino::deposit(
        &svm,
        &o,
        &user.obligation,
        &USDC_RESERVE,
        &user.usdc,
        DEPOSIT,
    );
    let mut instructions = kamino::refreshes(&svm, &user.obligation, &[USDC_RESERVE]);
    instructions.push(forward(template, &v2));
    send(&mut svm, &user, instructions).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(wallet::token_balance(&svm, &user.usdc), USDC - DEPOSIT);
    assert!(kamino::deposited(&svm, &user.obligation, &USDC_RESERVE) > 0);
}

#[test]
fn refresh_reserve_takes_six_accounts_with_scope_last() {
    let mut svm = lending::svm();
    let user = user(&mut svm, &[USDC_RESERVE]);
    // SOL's own `refresh_reserve`, whose accounts each variant replaces.
    let refresh = kamino::refreshes(&svm, &user.obligation, &[SOL_RESERVE])[0].clone();
    assert_eq!(refresh.accounts[0].pubkey, SOL_RESERVE);
    let with = |accounts: Vec<AccountMeta>| Instruction {
        accounts,
        ..refresh.clone()
    };
    let reserve = AccountMeta::new(SOL_RESERVE, false);
    let market = AccountMeta::new_readonly(MARKET, false);
    let scope = AccountMeta::new_readonly(SCOPE_PRICES, false);
    let none = AccountMeta::new_readonly(kamino::KLEND, false);

    // The templates' old list: the switchboard slots are missing.
    let three = with(vec![reserve.clone(), market.clone(), scope.clone()]);
    refused_for_too_few_accounts(&send(&mut svm, &user, vec![three]).unwrap_err());

    // Scope in the Pyth slot: these reserves price by Scope alone.
    let scope_as_pyth = with(vec![
        reserve.clone(),
        market.clone(),
        scope.clone(),
        none.clone(),
        none.clone(),
        none.clone(),
    ]);
    let failure = send(&mut svm, &user, vec![scope_as_pyth]).unwrap_err();
    kamino::refused(&failure, LendingError::InvalidPythPriceAccount);

    // A missing trailing optional is not "none".
    let five = with(vec![
        reserve.clone(),
        market.clone(),
        none.clone(),
        none.clone(),
        none.clone(),
    ]);
    refused_for_too_few_accounts(&send(&mut svm, &user, vec![five]).unwrap_err());

    assert_eq!(
        refresh.accounts,
        [reserve, market, none.clone(), none.clone(), none, scope]
    );
    send(&mut svm, &user, vec![refresh]).unwrap_or_else(|failure| panic!("{failure:?}"));
}

#[test]
fn refresh_obligation_takes_every_reserve_refreshed_in_the_slot() {
    let mut svm = lending::svm();
    let user = user(&mut svm, &[SOL_RESERVE]);
    let o = user.owner.pubkey();
    let wsol = wallet::token_account(&mut svm, &o, &WSOL_MINT, SOL);
    lending::deposit(
        &mut svm,
        &user.owner,
        &user.obligation,
        &SOL_RESERVE,
        &wsol,
        SOL,
    );
    lending::borrow(
        &mut svm,
        &user.owner,
        &user.obligation,
        &USDC_RESERVE,
        &user.usdc,
        DEPOSIT,
    );
    lending::next_slot(&mut svm);

    let refreshes = kamino::refreshes(&svm, &user.obligation, &[]);
    let obligation_refresh = refreshes.last().unwrap().clone();
    // Deposits, then borrows, each writable.
    assert_eq!(
        obligation_refresh.accounts[2..],
        [
            AccountMeta::new(SOL_RESERVE, false),
            AccountMeta::new(USDC_RESERVE, false)
        ]
    );

    let mut without_reserves = refreshes.clone();
    without_reserves.last_mut().unwrap().accounts.truncate(2);
    let failure = send(&mut svm, &user, without_reserves).unwrap_err();
    kamino::refused(&failure, LendingError::InvalidAccountInput);
    let failure = send(&mut svm, &user, vec![obligation_refresh]).unwrap_err();
    kamino::refused(&failure, LendingError::ReserveStale);
    send(&mut svm, &user, refreshes).unwrap_or_else(|failure| panic!("{failure:?}"));
}

#[test]
fn a_deposit_into_a_farmed_reserve_needs_the_farm_accounts() {
    let mut svm = lending::svm();
    let user = user(&mut svm, &[USDC_RESERVE]);
    let o = user.owner.pubkey();
    let mut deposit = kamino::deposit(
        &svm,
        &o,
        &user.obligation,
        &USDC_RESERVE,
        &user.usdc,
        DEPOSIT,
    );
    // The farm pair, the first two accounts after the declared ones: klend's ID means "none".
    let farm_pair = kamino::DEPOSIT_DECLARED..kamino::DEPOSIT_DECLARED + 2;
    for meta in &mut deposit.accounts[farm_pair] {
        *meta = AccountMeta::new_readonly(kamino::KLEND, false);
    }
    let mut instructions = kamino::refreshes(&svm, &user.obligation, &[USDC_RESERVE]);
    instructions.push(deposit);
    let failure = send(&mut svm, &user, instructions).unwrap_err();
    kamino::refused(&failure, LendingError::FarmAccountsMissing);
}

#[test]
fn a_deposit_needs_the_obligation_refreshed_in_its_slot() {
    let mut svm = lending::svm();
    let user = user(&mut svm, &[USDC_RESERVE]);
    let o = user.owner.pubkey();
    let deposit = kamino::deposit(
        &svm,
        &o,
        &user.obligation,
        &USDC_RESERVE,
        &user.usdc,
        DEPOSIT,
    );
    let failure = send(&mut svm, &user, vec![deposit]).unwrap_err();
    kamino::refused(&failure, LendingError::ObligationStale);
}

#[test]
fn a_liquidation_pays_liquidity_and_keeps_no_ctokens() {
    let mut svm = lending::svm();
    let unhealthy = lending::unhealthy_obligation(&mut svm);
    let (liquidator, accounts) = lending::liquidator(&mut svm, unhealthy.debt);
    lending::next_slot(&mut svm);

    let repay = unhealthy.liquidatable;
    let mut instructions = vec![lending::compute_limit()];
    instructions.extend(kamino::refreshes(&svm, &unhealthy.obligation, &[]));
    instructions.push(kamino::liquidate(
        &svm,
        &accounts,
        &unhealthy.obligation,
        &USDC_RESERVE,
        &SOL_RESERVE,
        repay,
        0,
    ));
    let outcome = tx::send(&mut svm, &liquidator, &[], &instructions, &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    let repaid = unhealthy.debt - wallet::token_balance(&svm, &accounts.source_liquidity);
    let received = wallet::token_balance(&svm, &accounts.destination_liquidity);
    let break_even = lending::break_even(repaid, unhealthy.usdc_price, unhealthy.sol_price);
    eprintln!(
        "liquidation: {} CU, {} bytes; repaid {repaid}, received {received} lamports, break-even {break_even}",
        outcome.compute_units, outcome.size
    );
    assert!(repaid > 0 && repaid <= repay, "repaid {repaid}");
    // klend burns the seized cTokens in the same instruction and pays out the SOL.
    assert_eq!(
        wallet::token_balance(&svm, &accounts.destination_collateral),
        0
    );
    assert!(received > break_even, "{received} <= {break_even}");
}
