//! Templates whose values need more than 64 registers, one each. The TypeScript compiler renumbers
//! them to reuse registers, and they must pass the verifier, on upload as well, and compute exactly
//! what a register per value would.

use super::*;
use ballista_common::template::OP_MOVE;
use mollusk_svm::result::InstructionResult;

const REQUIREMENT_FAILED: u32 = 6015;
const LOOP_COUNT_EXCEEDED: u32 = 6022;

type Context = MolluskContext<HashMap<Pubkey, Account>>;

/// One of this module's fixtures, as `pnpm fixtures` writes it.
fn reuse_fixture(name: &str) -> Vec<u8> {
    let hex = match name {
        "register-reuse-signed-quote" => {
            include_str!("../../../../fixtures/register-reuse-signed-quote.hex")
        }
        "register-reuse-loop" => include_str!("../../../../fixtures/register-reuse-loop.hex"),
        other => panic!("unknown fixture {other}"),
    };
    hex.trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

/// Requires `payload` to verify, to declare at most 64 registers, and to write more than 64
/// values, so the fixture still needs register reuse. A move into a register its loop carries
/// updates a value rather than writing a new one, so it does not count.
fn assert_reuses_registers(payload: &[u8]) {
    let program = ProgramView::parse(payload).expect("the fixture parses");
    program.verify().expect("the fixture verifies");
    let mut values = 0;
    let mut pc = 0;
    while pc < program.instructions.len() {
        let record = &program.instructions[pc];
        if matches!(record.opcode, OP_FOREACH | OP_REPEAT) {
            let carry = record.immediate();
            for body in &program.instructions[pc + 1..=pc + record.a as usize] {
                let carried = (body.dst as usize) < MAX_REGISTERS && carry & (1 << body.dst) != 0;
                if body.dst != NO_INDEX && !(body.opcode == OP_MOVE && carried) {
                    values += 1;
                }
            }
            pc += 1 + record.a as usize;
            continue;
        }
        if record.dst != NO_INDEX {
            values += 1;
        }
        pc += 1;
    }
    let registers = program.header.register_count();
    assert!(registers <= MAX_REGISTERS, "{registers} registers");
    assert!(
        values > MAX_REGISTERS,
        "{values} values fit in 64 registers without reuse, so the fixture no longer tests it"
    );
}

/// The error kind of a failed run of fixture `name`, and the label of the step it failed at.
fn failed_step(result: &InstructionResult, name: &str) -> (u32, String) {
    let code = custom_code(result).unwrap_or_else(|| panic!("no custom error: {result:#?}"));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../../fixtures/manifest.json")).expect("manifest");
    let label = manifest[name]["sourceMap"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["pc"] == code >> 16))
        .and_then(|entry| entry["label"].as_str())
        .unwrap_or_default()
        .to_owned();
    (code & 0xffff, label)
}

/// The signed-quote settlement with a cap on what each taker takes. Each fill pays and delivers as
/// the quote says, the taker's entry records it, and the cap holds and refills.
#[test]
fn a_signed_quote_with_a_rate_limit_settles_and_caps_each_taker() {
    const NAME: &str = "register-reuse-signed-quote";
    let payload = reuse_fixture(NAME);
    assert_reuses_registers(&payload);

    let creator = Pubkey::new_unique();
    let (maker_key, maker) = ed25519_keypair(9);
    let taker = Pubkey::new_unique();
    let (base_mint, quote_mint) = (Pubkey::new_unique(), Pubkey::new_unique());
    let [taker_quote, maker_quote, maker_base, taker_base] = [(); 4].map(|_| Pubkey::new_unique());
    let mut accounts = funded_accounts([creator, maker, taker], 10_000_000_000);
    for mint in [base_mint, quote_mint] {
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::None,
                supply: 1_000_000_000_000,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
    }
    for (address, mint, owner, amount) in [
        (taker_quote, quote_mint, taker, 100_000_000),
        (maker_quote, quote_mint, maker, 0),
        (maker_base, base_mint, maker, 10_000_000),
        (taker_base, base_mint, taker, 0),
    ] {
        accounts.insert(
            address,
            token::create_account_for_token_account(token_account_state(mint, owner, amount)),
        );
    }
    let mut context = context(accounts);
    context.mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
    let created = context.process_instruction(&create_template_instruction(creator, 93, &payload));
    assert!(created.program_result.is_ok(), "the upload verifies it: {created:#?}");
    let (template, _) = find_template_pda(&creator, 93);
    let entry =
        Pubkey::find_program_address(&[b"registry", template.as_ref(), &[0], taker.as_ref()], &ID).0;

    // 2.5 quote units per base unit, at most 4 base units a settlement, for a minute.
    let message =
        quote_message(2_500_000, 4_000_000, 1_800_000_060, &taker, &base_mint, &quote_mint);
    let quote = ed25519_instruction(&maker_key, &message, 1, u16::MAX);
    let settle = |context: &Context, amount: u64| {
        // The fixture's account order: the signed quote's, then the entry and the System program.
        let run = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(sysvar::instructions::id(), false),
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new(taker, true),
                AccountMeta::new_readonly(maker, true),
                AccountMeta::new(taker_quote, false),
                AccountMeta::new(maker_quote, false),
                AccountMeta::new(maker_base, false),
                AccountMeta::new(taker_base, false),
                AccountMeta::new(entry, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            &amount.to_le_bytes(),
        );
        context.process_transaction_instructions(&[quote.clone(), run])
    };
    // The entry's two fields: base units spent, and when they were last spent.
    let fills = |context: &Context| {
        let store = context.account_store.borrow();
        let data = store[&entry].data();
        (
            u64::from_le_bytes(data[72..80].try_into().unwrap()),
            i64::from_le_bytes(data[80..88].try_into().unwrap()),
        )
    };
    let capped = |result: &TransactionResult| fixture_failure(result, NAME);
    let over_the_cap = Some((1, REQUIREMENT_FAILED, "withinTakerCap".to_owned()));

    // 3.000001 base units at 2.5 cost 7.5000025 quote units, rounded up in the maker's favour. The
    // first fill opens the taker's entry.
    let settled = settle(&context, 3_000_001);
    assert!(settled.program_result.is_ok(), "{settled:#?}");
    eprintln!("signed quote with a rate limit, first fill: {} CU", settled.compute_units_consumed);
    assert_eq!(token_amount(&context, taker_quote), 100_000_000 - 7_500_003);
    assert_eq!(token_amount(&context, maker_quote), 7_500_003);
    assert_eq!(token_amount(&context, maker_base), 10_000_000 - 3_000_001);
    assert_eq!(token_amount(&context, taker_base), 3_000_001);
    assert_eq!(fills(&context), (3_000_001, 1_800_000_000));

    // The cap is 5 base units: 2 more is over it, 1.999999 more reaches it exactly.
    assert_eq!(capped(&settle(&context, 2_000_000)), over_the_cap);
    assert!(settle(&context, 1_999_999).program_result.is_ok());
    assert_eq!(fills(&context), (5_000_000, 1_800_000_000));

    // Ten seconds refill 1,000 base units: 1,001 is still over, 1,000 lands.
    context.mollusk.sysvars.clock.unix_timestamp += 10;
    assert_eq!(capped(&settle(&context, 1_001)), over_the_cap);
    assert!(settle(&context, 1_000).program_result.is_ok());
    assert_eq!(fills(&context), (5_000_000, 1_800_000_010));
    // 1.999999 cost 4.9999975, rounded up; 0.001 cost 0.0025. With the refill, the taker has
    // taken 5.001 base units in all.
    assert_eq!(token_amount(&context, maker_quote), 7_500_003 + 4_999_998 + 2_500);
    assert_eq!(token_amount(&context, taker_base), 5_001_000);
    assert_eq!(token_amount(&context, maker_base), 10_000_000 - 5_001_000);

    // The quote's own checks still name their steps: a minute later it has expired.
    context.mollusk.sysvars.clock.unix_timestamp += 60;
    assert_eq!(
        capped(&settle(&context, 1)),
        Some((1, REQUIREMENT_FAILED, "quoteHasNotExpired".to_owned()))
    );
}

/// The loop fixture. Each even pass pays the term the pass computed, past the values its guard
/// computes; the carried total and largest term flow from pass to pass; and the run returns both.
#[test]
fn a_loop_past_64_registers_pays_each_term_and_returns_what_it_carried() {
    const NAME: &str = "register-reuse-loop";
    let payload = reuse_fixture(NAME);
    assert_reuses_registers(&payload);

    let creator = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let recipient = Pubkey::new_unique();
    let mut accounts = funded_accounts([creator, payer], 10_000_000_000);
    accounts.insert(recipient, Account::new(1_000_000_000, 0, &system_program::id()));
    let context = context(accounts);
    let created = context.process_instruction(&create_template_instruction(creator, 94, &payload));
    assert!(created.program_result.is_ok(), "the upload verifies it: {created:#?}");
    let (template, _) = find_template_pda(&creator, 94);
    let run = |passes: u64, stride: u64| {
        // The fixture's account order: systemProgram, payer, recipient.
        let metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(payer, true),
            AccountMeta::new(recipient, false),
        ];
        let mut inputs = passes.to_le_bytes().to_vec();
        inputs.extend_from_slice(&stride.to_le_bytes());
        context.process_instruction(&run_instruction(template, metas, &inputs))
    };

    for (passes, stride) in [(0u64, 1_000u64), (1, 1_000), (4, 1_000), (10, 7)] {
        let before = lamports(&context, recipient);
        let result = run(passes, stride);
        assert!(result.program_result.is_ok(), "{passes} passes: {result:#?}");
        let terms: Vec<u64> = (0..passes).map(|index| index * stride + 1).collect();
        let total: u64 = terms.iter().sum();
        let largest = terms.iter().copied().max().unwrap_or(0);
        let paid: u64 = terms.iter().step_by(2).sum();
        let mut returned = total.to_le_bytes().to_vec();
        returned.extend_from_slice(&largest.to_le_bytes());
        assert_eq!(result.return_data, returned, "{passes} passes of {stride}");
        assert_eq!(lamports(&context, recipient), before + paid, "{passes} passes of {stride}");
    }

    // Eleven passes is over the loop's maximum of ten: the run fails at the loop and pays nothing.
    let before = lamports(&context, recipient);
    let over = run(11, 1);
    assert_eq!(failed_step(&over, NAME), (LOOP_COUNT_EXCEEDED, "payTerms".to_owned()));
    assert_eq!(lamports(&context, recipient), before);
}
