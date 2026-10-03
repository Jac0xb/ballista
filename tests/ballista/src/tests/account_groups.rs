//! `GROUP_LENGTH`, `GROUP_ANY` and `GROUP_COUNT` against the program: what a template learns about
//! the members a caller supplies in an account group.

use super::*;
use ballista_common::template::{MAX_RUNTIME_ACCOUNTS, OP_CONST_U64, OP_GTE};
use mollusk_svm::result::InstructionResult;

const TOKEN_2022: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

type Context = MolluskContext<HashMap<Pubkey, Account>>;

/// `fixtures.test.ts`'s `group-filter`. It returns, in order: the group's length; how many members
/// are token accounts (Token or Token-2022) of the input mint owned by `user`; whether any is; how
/// many are, `destination` aside; and how many are Token accounts of at least 165 bytes holding
/// exactly 1,000.
fn group_filter_fixture() -> Vec<u8> {
    let hex = include_str!("../../../../fixtures/group-filter.hex").trim();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    ProgramView::parse(&bytes)
        .and_then(|program| program.verify())
        .expect("the fixture verifies");
    bytes
}

/// `len` bytes of token-account data: the mint at 0, the owner at 32 and the amount at 64, each
/// written only as far as `len` reaches.
fn token_data(mint: Pubkey, owner: Pubkey, amount: u64, len: usize) -> Vec<u8> {
    let mut data = vec![0u8; len.max(72)];
    data[..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data.truncate(len);
    data
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

/// Uploads `payload` as template `id` of a fresh creator and returns its address.
fn upload(context: &Context, id: u16, payload: &[u8]) -> Pubkey {
    let creator = Pubkey::new_unique();
    context.account_store.borrow_mut().insert(
        creator,
        Account::new(10_000_000_000, 0, &system_program::id()),
    );
    let created = context.process_instruction(&create_template_instruction(creator, id, payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    find_template_pda(&creator, id).0
}

/// The fixture's five results, read back from the return data.
fn results(result: &InstructionResult) -> (u64, u64, bool, u64, u64) {
    assert!(result.program_result.is_ok(), "{result:#?}");
    let data = &result.return_data;
    assert_eq!(data.len(), 33);
    let word = |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
    (word(0), word(8), data[16] == 1, word(17), word(25))
}

#[test]
fn the_typescript_fixture_counts_the_members_that_match() {
    let user = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let other = Pubkey::new_unique();
    let destination = Pubkey::new_unique();
    let member = |data| (Pubkey::new_unique(), data);

    // Each member, and whether it is the user's token account of `mint`.
    let token_of_user = member(owned(token::ID, token_data(mint, user, 1_000, 165)));
    let token_2022_of_user = member(owned(TOKEN_2022, token_data(mint, user, 5, 170)));
    let destination_account = (
        destination,
        owned(token::ID, token_data(mint, user, 7, 165)),
    );
    // The same bytes, owned by a program the filter does not name.
    let system_owned = member(owned(
        system_program::id(),
        token_data(mint, user, 1_000, 165),
    ));
    // A Token account too short to hold the owner the filter compares.
    let short = member(owned(token::ID, token_data(mint, user, 1_000, 40)));
    let other_owner = member(owned(token::ID, token_data(mint, other, 1, 165)));
    // Holds 1,000, but in 100 bytes, under the last filter's 165-byte floor.
    let other_mint = member(owned(token::ID, token_data(other, user, 1_000, 100)));

    let mut accounts: HashMap<Pubkey, Account> = HashMap::new();
    accounts.insert(user, Account::new(1_000_000_000, 0, &system_program::id()));
    let members = [
        token_of_user,
        token_2022_of_user,
        destination_account,
        system_owned,
        short,
        other_owner,
        other_mint,
    ];
    for (address, account) in &members {
        accounts.insert(*address, account.clone());
    }
    let context = context(accounts);
    let template = upload(&context, 1, &group_filter_fixture());

    let run = |route: &[Pubkey]| {
        let mut metas = vec![
            AccountMeta::new_readonly(user, true),
            AccountMeta::new_readonly(destination, false),
        ];
        // A member's privileges do not matter to a filter: these are writable.
        metas.extend(
            route
                .iter()
                .map(|address| AccountMeta::new(*address, false)),
        );
        let mut inputs = vec![route.len() as u8];
        inputs.extend_from_slice(mint.as_ref());
        context.process_instruction(&run_instruction(template, metas, &inputs))
    };

    let everyone: Vec<Pubkey> = members.iter().map(|(address, _)| *address).collect();
    // Three token accounts of the user's: Token, Token-2022, and the destination, which the fourth
    // result leaves out. One Token account of 165 bytes or more holds exactly 1,000.
    assert_eq!(results(&run(&everyone)), (7, 3, true, 2, 1));
    // Order does not change a count.
    let reversed: Vec<Pubkey> = everyone.iter().rev().copied().collect();
    assert_eq!(results(&run(&reversed)), (7, 3, true, 2, 1));
    // An empty group.
    assert_eq!(results(&run(&[])), (0, 0, false, 0, 0));
    // Members that fail on the owner program, the data length, or the bytes.
    assert_eq!(
        results(&run(&[everyone[3], everyone[4], everyone[5], everyone[6]])),
        (4, 0, false, 0, 0)
    );
    // The destination alone matches, except where it is excepted.
    assert_eq!(results(&run(&[destination])), (1, 1, true, 0, 0));
    // A member passed twice counts twice: the filter counts members, not distinct accounts.
    assert_eq!(
        results(&run(&[everyone[0], everyone[0]])),
        (2, 2, true, 2, 2)
    );
}

/// A template that reads one value about the group and requires it be at least zero: with `filter`,
/// how many members are Token accounts that `user` owns holding exactly 1,000, two matches over
/// 165 bytes; without, the group's length. The difference between the two on the same members is
/// the filter's work, apart from what every account in an instruction costs.
fn measuring_template(filter: bool) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let user = builder.account(ACCOUNT_SIGNER, None, None, 0);
    builder.account_groups(1);
    let key = builder.op(OP_ACCOUNT_KEY, user, NO_INDEX, NO_INDEX, 0);
    let amount = builder.op(OP_CONST_U64, NO_INDEX, NO_INDEX, NO_INDEX, 1_000);
    let value = if filter {
        builder.group_filter(
            true,
            0,
            &[token::ID.to_bytes()],
            &[(32, DATA_REG_PUBKEY, key), (64, DATA_REG_U64, amount)],
            &[],
            165,
        )
    } else {
        builder.group_length(0)
    };
    let zero = builder.const_u64(0);
    let at_least_zero = builder.binary(OP_GTE, value, zero);
    builder.require(at_least_zero);
    let bytes = builder.build().unwrap();
    ProgramView::parse(&bytes)
        .unwrap()
        .verify()
        .expect("verifies");
    bytes
}

/// Prints what a filter costs a member that matches, and one that fails at each test, against the
/// same members read only for the group's length; `--nocapture` shows the figures.
#[test]
fn a_filter_costs_the_same_for_every_member() {
    let user = Pubkey::new_unique();
    let other = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let count = MAX_RUNTIME_ACCOUNTS - 1;
    let kinds: [(&str, Account); 4] = [
        (
            "matching",
            owned(token::ID, token_data(mint, user, 1_000, 165)),
        ),
        (
            "wrong owner program",
            owned(system_program::id(), token_data(mint, user, 1_000, 165)),
        ),
        (
            "short data",
            owned(token::ID, token_data(mint, user, 1_000, 100)),
        ),
        (
            "wrong bytes",
            owned(token::ID, token_data(mint, other, 1_000, 165)),
        ),
    ];
    let mut accounts: HashMap<Pubkey, Account> = HashMap::new();
    accounts.insert(user, Account::new(1_000_000_000, 0, &system_program::id()));
    let mut members: Vec<Vec<Pubkey>> = Vec::new();
    for (_, account) in &kinds {
        let addresses: Vec<Pubkey> = (0..count).map(|_| Pubkey::new_unique()).collect();
        for address in &addresses {
            accounts.insert(*address, account.clone());
        }
        members.push(addresses);
    }
    let context = context(accounts);
    let filtering = upload(&context, 1, &measuring_template(true));
    let length_only = upload(&context, 2, &measuring_template(false));

    let units = |template: Pubkey, route: &[Pubkey]| {
        let mut metas = vec![AccountMeta::new_readonly(user, true)];
        metas.extend(
            route
                .iter()
                .map(|address| AccountMeta::new_readonly(*address, false)),
        );
        let result =
            context.process_instruction(&run_instruction(template, metas, &[route.len() as u8]));
        assert!(result.program_result.is_ok(), "{result:#?}");
        result.compute_units_consumed
    };
    // The filter's own cost on `route`: the same members, read only for their length, cost the
    // rest.
    let filter_units = |route: &[Pubkey]| units(filtering, route) - units(length_only, route);
    let fixed = filter_units(&[]);
    eprintln!("group filter, empty group: {fixed} CU over GROUP_LENGTH");
    for ((name, _), addresses) in kinds.iter().zip(&members) {
        let per = |n: usize| (filter_units(&addresses[..n]) - fixed) as f64 / n as f64;
        let (one, ten, all) = (per(1), per(10), per(count));
        eprintln!(
            "group filter, {name}: {one:.1} CU a member over 1, {ten:.1} over 10, {all:.1} over {count}"
        );
        // Every member costs the same, wherever it sits: over 10 members the run's fixed costs
        // still weigh a little.
        assert!(
            (ten - all).abs() <= 5.0,
            "{name}: {ten} against {all} CU a member"
        );
        // Measured at 91 for a member that passes both matches, 55 for one that fails the first,
        // 25 for one too short and 16 for one of another program.
        assert!(all <= 100.0, "{name}: {all} CU a member");
    }
}
