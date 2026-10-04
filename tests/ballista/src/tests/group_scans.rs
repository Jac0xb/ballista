//! What a group scan costs, per member, per match segment and per except, and its worst case; and
//! one realistic count, with what the program does about duplicate members, members shorter than
//! the floor and the template account as a member.
//!
//! The figures print with `--nocapture`. The asserts bound them loosely: the ceilings live in
//! `fixtures/cu-ceilings.json`, not here.

use super::*;
use ballista_common::template::{
    MAX_RUNTIME_ACCOUNTS, OP_AND, OP_CONST_U64, OP_GT, OP_LOAD_INPUT, VALUE_PUBKEY,
};
use mollusk_svm::result::{InstructionResult, ProgramResult};

const TOKEN_2022: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

type Context = MolluskContext<HashMap<Pubkey, Account>>;

/// The values a member's data holds at 0, 32, 64 and 96, which the match segments compare.
fn value(index: usize) -> [u8; 32] {
    [index as u8 + 1; 32]
}

fn owned(program: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: 1_000_000,
        data,
        owner: program,
        executable: false,
        rent_epoch: 0,
    }
}

/// 165 bytes holding every match value: a member every filter here matches.
fn matching_data() -> Vec<u8> {
    let mut data = vec![0u8; 165];
    for index in 0..4 {
        data[index * 32..index * 32 + 32].copy_from_slice(&value(index));
    }
    data
}

fn upload(context: &Context, id: u16, payload: &[u8]) -> Pubkey {
    ProgramView::parse(payload)
        .and_then(|program| program.verify())
        .expect("the template verifies");
    let creator = Pubkey::new_unique();
    context.account_store.borrow_mut().insert(
        creator,
        Account::new(10_000_000_000, 0, &system_program::id()),
    );
    let created = context.process_instruction(&create_template_instruction(creator, id, payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    find_template_pda(&creator, id).0
}

/// A template that counts the members owned by one of `programs` whose data holds the first
/// `matches` values (32-byte segments at 0, 32, 64 and 96: the widest compare, made in full when
/// it matches) and whose key is none of `excepts` keys that never match, then requires the count
/// to equal its `u64` input. With `matches` zero it reads the group's length instead, and loads
/// the same constants, so the two differ only in the scan.
fn counting_template(matches: usize, excepts: usize, programs: &[Pubkey]) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    builder.account(ACCOUNT_SIGNER, None, None, 0);
    builder.account_groups(1);
    let expected_input = builder.input(VALUE_U64, 0);
    let segments: Vec<(u16, u8, u8)> = (0..matches.max(1))
        .map(|index| {
            let register = builder.const_pubkey(value(index));
            ((index * 32) as u16, DATA_REG_PUBKEY, register)
        })
        .collect();
    let except_registers: Vec<u8> = (0..excepts)
        .map(|index| builder.const_pubkey([0xe0 + index as u8; 32]))
        .collect();
    let programs: Vec<[u8; 32]> = programs.iter().map(Pubkey::to_bytes).collect();
    let result = if matches == 0 {
        builder.group_length(0)
    } else {
        builder.group_filter(true, 0, &programs, &segments, &except_registers, 165)
    };
    if matches == 0 {
        // Keep the constants read, as the scan reads them.
        let mut keep = builder.binary(OP_EQ, segments[0].2, segments[0].2);
        for register in &except_registers {
            let same = builder.binary(OP_EQ, *register, *register);
            keep = builder.binary(OP_AND, keep, same);
        }
        builder.require(keep);
    }
    let expected = builder.op(OP_LOAD_INPUT, expected_input, NO_INDEX, NO_INDEX, 0);
    let equal = builder.binary(OP_EQ, result, expected);
    builder.require(equal);
    builder.build().unwrap()
}

fn run(
    context: &Context,
    template: Pubkey,
    user: Pubkey,
    fixed: &[AccountMeta],
    members: &[Pubkey],
    inputs: &[u8],
) -> InstructionResult {
    let mut metas = vec![AccountMeta::new_readonly(user, true)];
    metas.extend_from_slice(fixed);
    metas.extend(
        members
            .iter()
            .map(|address| AccountMeta::new_readonly(*address, false)),
    );
    let mut data = vec![members.len() as u8];
    data.extend_from_slice(inputs);
    context.process_instruction(&run_instruction(template, metas, &data))
}

/// Per member, per match and per except, on members that all match (each compare made in full),
/// over the same members read only for the group's length; then the worst case: four 32-byte
/// matches, four excepts, two programs with every member owned by the second, every member
/// matching.
#[test]
fn a_scan_costs_a_fixed_amount_per_member_match_and_except() {
    let user = Pubkey::new_unique();
    let other_program = Pubkey::new_unique();
    let most = MAX_RUNTIME_ACCOUNTS - 1;
    let mut accounts: HashMap<Pubkey, Account> = HashMap::new();
    accounts.insert(user, Account::new(1_000_000_000, 0, &system_program::id()));
    let members: Vec<Pubkey> = (0..most).map(|_| Pubkey::new_unique()).collect();
    for address in &members {
        accounts.insert(*address, owned(token::ID, matching_data()));
    }
    let context = context(accounts);

    let id = std::cell::Cell::new(0u16);
    let template = |matches: usize, excepts: usize, programs: &[Pubkey]| {
        id.set(id.get() + 1);
        upload(
            &context,
            id.get(),
            &counting_template(matches, excepts, programs),
        )
    };
    let units = |template: Pubkey, n: usize, expected: usize| {
        let result = run(
            &context,
            template,
            user,
            &[],
            &members[..n],
            &(expected as u64).to_le_bytes(),
        );
        assert!(result.program_result.is_ok(), "{result:#?}");
        result.compute_units_consumed
    };
    let token_only = [token::ID];
    let either = [other_program, token::ID];
    let length_only = |excepts: usize| template(0, excepts, &token_only);
    // The scan's cost a member: the scan over `n` members, less the same run reading the length,
    // less the same difference over no members.
    let per_member = |scan: Pubkey, length: Pubkey, n: usize| {
        let over = |n: usize| units(scan, n, n) as i64 - units(length, n, n) as i64;
        (over(n) - over(0)) as f64 / n as f64
    };

    let baseline = length_only(0);
    let mut by_matches = Vec::new();
    for matches in 1..=4 {
        let scan = template(matches, 0, &token_only);
        let (ten, all) = (
            per_member(scan, baseline, 10),
            per_member(scan, baseline, most),
        );
        eprintln!(
            "group scan, {matches} match(es): {ten:.1} CU a member over 10, {all:.1} over {most}"
        );
        assert!(
            (ten - all).abs() <= 5.0,
            "{matches} matches: {ten} against {all} CU a member"
        );
        by_matches.push(all);
    }
    let per_match: Vec<f64> = by_matches
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    eprintln!("group scan, each further 32-byte match: {per_match:.1?} CU a member");

    let mut by_excepts = vec![by_matches[3]];
    for excepts in 1..=4 {
        let scan = template(4, excepts, &token_only);
        by_excepts.push(per_member(scan, length_only(excepts), most));
    }
    let per_except: Vec<f64> = by_excepts
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    eprintln!("group scan, each except that misses: {per_except:.1?} CU a member");

    let second_owner = per_member(template(4, 0, &either), baseline, most) - by_matches[3];
    eprintln!(
        "group scan, a member owned by the second program: {second_owner:.1} CU more a member"
    );

    let worst = template(4, 4, &either);
    let worst_length = length_only(4);
    for n in [61, most] {
        let total = units(worst, n, n);
        let scan = total as i64 - units(worst_length, n, n) as i64;
        eprintln!("group scan, worst case over {n} members: {total} CU the run, {scan} CU the scan over GROUP_LENGTH");
        assert!(total < 200_000, "{n} members: {total} CU");
    }
    // Every figure here is far under a transaction's budget; these bounds catch a scan that grows
    // faster than linearly, or a compare that stops being a few instructions a byte.
    assert!(
        by_excepts[4] <= 400.0,
        "worst case: {} CU a member",
        by_excepts[4]
    );
}

/// A realistic count: the token accounts among a group's members that the Token program owns,
/// for one mint (offset 0) and one holder (offset 32), the holder's own declared account aside.
/// The members include the same account twice, accounts under the 165-byte floor, the excepted
/// account, a Token-2022 account, and the template account itself.
#[test]
fn a_group_counts_a_holders_token_accounts_for_a_mint() {
    let user = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let other = Pubkey::new_unique();
    let token_account = |mint: Pubkey, owner: Pubkey, len: usize| {
        let mut data = vec![0u8; len.max(64)];
        data[..32].copy_from_slice(mint.as_ref());
        data[32..64].copy_from_slice(owner.as_ref());
        data.truncate(len);
        data
    };
    let named = [
        ("held", owned(token::ID, token_account(mint, user, 165))),
        (
            "held, longer",
            owned(token::ID, token_account(mint, user, 200)),
        ),
        ("excepted", owned(token::ID, token_account(mint, user, 165))),
        (
            "one byte short",
            owned(token::ID, token_account(mint, user, 164)),
        ),
        (
            "only the mint",
            owned(token::ID, token_account(mint, user, 32)),
        ),
        ("empty", owned(token::ID, Vec::new())),
        (
            "another holder",
            owned(token::ID, token_account(mint, other, 165)),
        ),
        (
            "another mint",
            owned(token::ID, token_account(other, user, 165)),
        ),
        (
            "Token-2022",
            owned(TOKEN_2022, token_account(mint, user, 165)),
        ),
    ];
    let mut accounts: HashMap<Pubkey, Account> = HashMap::new();
    accounts.insert(user, Account::new(1_000_000_000, 0, &system_program::id()));
    let addresses: Vec<Pubkey> = named.iter().map(|_| Pubkey::new_unique()).collect();
    for (address, (_, account)) in addresses.iter().zip(&named) {
        accounts.insert(*address, account.clone());
    }
    let address = |name: &str| {
        addresses[named
            .iter()
            .position(|(current, _)| *current == name)
            .unwrap()]
    };
    let context = context(accounts);

    // count = GROUP_COUNT(token accounts of `mint` held by `user`, except `excepted`); require
    // count == input and GROUP_LENGTH == input, and GROUP_ANY == (count > 0).
    let mut builder = ProgramBuilder::new();
    let signer = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let excepted = builder.account(0, None, None, 0);
    builder.account_groups(1);
    let mint_input = builder.input(VALUE_PUBKEY, 0);
    let count_input = builder.input(VALUE_U64, 0);
    let length_input = builder.input(VALUE_U64, 0);
    let mint_register = builder.op(OP_LOAD_INPUT, mint_input, NO_INDEX, NO_INDEX, 0);
    let holder = builder.op(OP_ACCOUNT_KEY, signer, NO_INDEX, NO_INDEX, 0);
    let except = builder.op(OP_ACCOUNT_KEY, excepted, NO_INDEX, NO_INDEX, 0);
    let filter = |builder: &mut ProgramBuilder, count: bool| {
        builder.group_filter(
            count,
            0,
            &[token::ID.to_bytes()],
            &[
                (0, DATA_REG_PUBKEY, mint_register),
                (32, DATA_REG_PUBKEY, holder),
            ],
            &[except],
            165,
        )
    };
    let count = filter(&mut builder, true);
    let any = filter(&mut builder, false);
    let length = builder.group_length(0);
    let expected_count = builder.op(OP_LOAD_INPUT, count_input, NO_INDEX, NO_INDEX, 0);
    let expected_length = builder.op(OP_LOAD_INPUT, length_input, NO_INDEX, NO_INDEX, 0);
    let zero = builder.op(OP_CONST_U64, NO_INDEX, NO_INDEX, NO_INDEX, 0);
    let counted = builder.binary(OP_EQ, count, expected_count);
    let measured = builder.binary(OP_EQ, length, expected_length);
    let some = builder.binary(OP_GT, count, zero);
    let agrees = builder.binary(OP_EQ, any, some);
    for condition in [counted, measured, agrees] {
        builder.require(condition);
    }
    let template = upload(&context, 1, &builder.build().unwrap());

    let fixed = [AccountMeta::new_readonly(address("excepted"), false)];
    let check = |members: &[Pubkey], count: u64| -> InstructionResult {
        let mut inputs = mint.to_bytes().to_vec();
        inputs.extend_from_slice(&count.to_le_bytes());
        inputs.extend_from_slice(&(members.len() as u64).to_le_bytes());
        run(&context, template, user, &fixed, members, &inputs)
    };
    // The count is exact: it passes at `count` and fails one either side.
    let exactly = |members: &[Pubkey], count: u64| {
        let result = check(members, count);
        assert!(
            result.program_result.is_ok(),
            "{count} over {members:?}: {result:#?}"
        );
        for wrong in [count.wrapping_sub(1), count + 1] {
            assert!(
                matches!(
                    check(members, wrong).program_result,
                    ProgramResult::Failure(_)
                ),
                "{wrong} over {members:?} passed"
            );
        }
    };

    let everyone: Vec<Pubkey> = addresses.clone();
    // "held" and "held, longer"; nothing else is the user's Token account of `mint` with 165
    // bytes or more that the template does not except.
    exactly(&everyone, 2);
    // Observed: a member supplied twice counts twice (the scan counts members, not accounts).
    exactly(&[address("held"), address("held"), address("held")], 3);
    // Observed: members under the floor are skipped, not an error, down to no data at all.
    exactly(
        &[
            address("one byte short"),
            address("only the mint"),
            address("empty"),
        ],
        0,
    );
    // An except removes the account however many times it appears.
    exactly(
        &[address("excepted"), address("excepted"), address("held")],
        1,
    );
    // Observed: the template account, owned by Ballista, is a member like any other; it does not
    // match a Token filter and does not fail the run.
    exactly(&[template, address("held")], 1);
    // The signer, a declared account, as a member: a system account, so it does not match.
    exactly(&[user, address("held, longer")], 1);
    exactly(&[], 0);
}
