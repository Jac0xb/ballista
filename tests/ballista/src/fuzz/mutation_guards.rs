//! Deterministic tests that pin the exact error of each account and arithmetic check the docs
//! promise, so deleting the check makes the test fail rather than pass on a different error.
//!
//! Each test builds a template that would SUCCEED if the check were gone (the critic's point: a
//! test that asserts only `is_err()` is satisfied by any later failure), then supplies the one
//! input that should trip the check and asserts the exact `(kind, context)`.
//!
//! The mutants these catch (applied with `scripts/critic/mutate.py` on the critic branch, or by
//! hand here) are reported in the handback. Run: `cargo test -p ballista-integration-tests
//! fuzz::mutation_guards`.

use ballista_common::instruction::{IX_CREATE_TEMPLATE, IX_RUN};
use ballista_common::template::{
    ProgramBuilder, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
    SYSTEM_PROGRAM_ADDRESS, VALUE_U64,
};
use mollusk_svm::result::types::ProgramResult;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;
use solana_sdk_ids::system_program;

use super::harness::{Harness, BALLISTA_ID};

const ARITHMETIC_OVERFLOW: u32 = 6013;
const REQUIREMENT_FAILED: u32 = 6015;
const ACCOUNT_CONSTRAINT_FAILED: u32 = 6020;
const INVALID_CPI: u32 = 6115;

const CREATOR: Pubkey = Pubkey::new_from_array([0x44; 32]);

/// The custom `(kind, context)` a result failed with.
fn custom(result: &mollusk_svm::result::types::InstructionResult) -> Option<(u32, u32)> {
    match &result.program_result {
        ProgramResult::Failure(ProgramError::Custom(code)) => Some((code & 0xffff, code >> 16)),
        _ => None,
    }
}

fn program_error(result: &mollusk_svm::result::types::InstructionResult) -> Option<ProgramError> {
    match &result.program_result {
        ProgramResult::Failure(error) => Some(error.clone()),
        _ => None,
    }
}

fn template_pda(id: u16) -> Pubkey {
    Pubkey::find_program_address(&[b"template", CREATOR.as_ref(), &id.to_le_bytes()], &BALLISTA_ID).0
}

/// Uploads `payload` and returns the template address.
fn upload(harness: &Harness, id: u16, payload: &[u8]) -> Pubkey {
    harness.reset();
    let (template, _) = harness.upload(&CREATOR, id, payload).expect("template uploads");
    template
}

/// Runs `template` with the given runtime accounts (each `(address, signer, writable, account)`)
/// and empty inputs.
fn run(harness: &Harness, template: &Pubkey, runtime: &[(Pubkey, bool, bool, Account)]) -> mollusk_svm::result::types::InstructionResult {
    {
        let mut store = harness.context.account_store.borrow_mut();
        for (address, _, _, account) in runtime {
            store.insert(*address, account.clone());
        }
    }
    let mut accounts = vec![AccountMeta::new_readonly(*template, false)];
    for (address, signer, writable, _) in runtime {
        accounts.push(AccountMeta { pubkey: *address, is_signer: *signer, is_writable: *writable });
    }
    harness
        .context
        .process_instruction(&Instruction { program_id: BALLISTA_ID, accounts, data: vec![IX_RUN] })
}

fn system_account(lamports: u64) -> Account {
    Account::new(lamports, 0, &system_program::id())
}

#[test]
fn signer_check_rejects_an_unsigned_authorizer() {
    // A template that declares one signer "approver" and nothing else: the run authorizes on the
    // approver's signature alone. With the signer check gone, this run would succeed.
    let mut builder = ProgramBuilder::new();
    builder.account(ACCOUNT_SIGNER, None, None, 0);
    let flag = builder.const_bool(true);
    builder.require(flag);
    let harness = Harness::new();
    let template = upload(&harness, 1, &builder.build().unwrap());

    let approver = Pubkey::new_from_array([0x55; 32]);
    // Pass the approver without its signature.
    let result = run(&harness, &template, &[(approver, false, false, system_account(1))]);
    assert_eq!(
        program_error(&result),
        Some(ProgramError::MissingRequiredSignature),
        "an unsigned declared signer must fail with MissingRequiredSignature; deleting the check lets it pass"
    );
    // The same template with the signature present succeeds, so the failure is the missing
    // signature, not the template itself.
    let ok = run(&harness, &template, &[(approver, true, false, system_account(1))]);
    assert!(ok.program_result.is_ok(), "with the signature the run succeeds: {ok:?}");
}

#[test]
fn writable_check_rejects_a_readonly_account() {
    let mut builder = ProgramBuilder::new();
    builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let flag = builder.const_bool(true);
    builder.require(flag);
    let harness = Harness::new();
    let template = upload(&harness, 2, &builder.build().unwrap());

    let account = Pubkey::new_from_array([0x66; 32]);
    let result = run(&harness, &template, &[(account, false, false, system_account(1))]);
    assert_eq!(
        custom(&result),
        Some((ACCOUNT_CONSTRAINT_FAILED, 0)),
        "a read-only account where the template requires writable must fail AccountConstraintFailed at index 0"
    );
    let ok = run(&harness, &template, &[(account, false, true, system_account(1))]);
    assert!(ok.program_result.is_ok(), "passed writable, the run succeeds: {ok:?}");
}

#[test]
fn address_pin_rejects_a_substitute_account() {
    let pinned = Pubkey::new_from_array([0x77; 32]);
    let mut builder = ProgramBuilder::new();
    builder.account(0, Some(pinned.to_bytes()), None, 0);
    let flag = builder.const_bool(true);
    builder.require(flag);
    let harness = Harness::new();
    let template = upload(&harness, 3, &builder.build().unwrap());

    let other = Pubkey::new_from_array([0x78; 32]);
    let result = run(&harness, &template, &[(other, false, false, system_account(1))]);
    assert_eq!(
        custom(&result),
        Some((ACCOUNT_CONSTRAINT_FAILED, 0)),
        "a substitute for a pinned address must fail AccountConstraintFailed at index 0"
    );
    let ok = run(&harness, &template, &[(pinned, false, false, system_account(1))]);
    assert!(ok.program_result.is_ok(), "the pinned account passes: {ok:?}");
}

#[test]
fn owner_pin_rejects_a_wrong_owner() {
    let owner = Pubkey::new_from_array([0x79; 32]);
    let mut builder = ProgramBuilder::new();
    builder.account(0, None, Some(owner.to_bytes()), 0);
    let flag = builder.const_bool(true);
    builder.require(flag);
    let harness = Harness::new();
    let template = upload(&harness, 4, &builder.build().unwrap());

    let account = Pubkey::new_from_array([0x7a; 32]);
    let wrong = Account::new(1, 0, &system_program::id());
    let result = run(&harness, &template, &[(account, false, false, wrong)]);
    assert_eq!(
        custom(&result),
        Some((ACCOUNT_CONSTRAINT_FAILED, 0)),
        "an account owned by the wrong program must fail AccountConstraintFailed at index 0"
    );
    let right = Account::new(1, 0, &owner);
    let ok = run(&harness, &template, &[(account, false, false, right)]);
    assert!(ok.program_result.is_ok(), "the correctly owned account passes: {ok:?}");
}

#[test]
fn add_overflow_fails_and_a_sum_is_exact() {
    // r0 = input; r1 = u64::MAX; r2 = r0 + r1; emit r2. A non-zero input overflows.
    let mut builder = ProgramBuilder::new();
    let input = builder.input(VALUE_U64, 0);
    let value = builder.load_input(input);
    let max = builder.const_u64(u64::MAX);
    let sum = builder.binary(ballista_common::template::OP_ADD, value, max);
    builder.set_return_data(&[Segment::Register(DATA_REG_U64, sum)]);
    let harness = Harness::new();
    let template = upload(&harness, 5, &builder.build().unwrap());

    // input = 1 overflows u64::MAX + 1.
    let overflow = run_with_input(&harness, &template, 1);
    assert_eq!(
        custom(&overflow),
        Some((ARITHMETIC_OVERFLOW, 2)),
        "u64::MAX + 1 must fail ArithmeticOverflow at the ADD's pc; a wrapping add would return 0 and succeed"
    );

    // input = 0: the sum is exactly u64::MAX, returned as the run's data.
    let exact = run_with_input(&harness, &template, 0);
    assert!(exact.program_result.is_ok(), "u64::MAX + 0 succeeds: {exact:?}");
    assert_eq!(exact.return_data, u64::MAX.to_le_bytes(), "the sum must be exactly u64::MAX");
}

fn run_with_input(harness: &Harness, template: &Pubkey, input: u64) -> mollusk_svm::result::types::InstructionResult {
    let mut data = vec![IX_RUN];
    data.extend_from_slice(&input.to_le_bytes());
    harness.context.process_instruction(&Instruction {
        program_id: BALLISTA_ID,
        accounts: vec![AccountMeta::new_readonly(*template, false)],
        data,
    })
}

#[test]
fn require_false_stops_the_run() {
    // r0 = input; require(r0 == 0). With `require` never stopping the run, input=1 would succeed.
    let mut builder = ProgramBuilder::new();
    let input = builder.input(VALUE_U64, 0);
    let value = builder.load_input(input);
    let zero = builder.const_u64(0);
    let is_zero = builder.binary(ballista_common::template::OP_EQ, value, zero);
    builder.require(is_zero);
    let harness = Harness::new();
    let template = upload(&harness, 6, &builder.build().unwrap());

    let fails = run_with_input(&harness, &template, 1);
    assert_eq!(
        custom(&fails),
        Some((REQUIREMENT_FAILED, 3)),
        "require(false) must fail RequirementFailed at the require's pc"
    );
    let ok = run_with_input(&harness, &template, 0);
    assert!(ok.program_result.is_ok(), "require(true) continues: {ok:?}");
}

#[test]
fn verifier_rejects_a_cpi_exceeding_the_account_ceiling() {
    // A CPI that passes a read-only account as writable. The verifier must reject the template at
    // create; with the privilege check gone, create would succeed.
    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let account = builder.account(0, None, None, 0); // declared read-only
    let literal = builder.blob(&[0]);
    // Ask the CPI to pass `account` as writable, which its declaration does not allow.
    let cpi = builder.cpi(program, &[(account, ACCOUNT_WRITABLE)], &[Segment::Literal(literal)]);
    builder.invoke(cpi, None);
    let payload = builder.build().unwrap();

    let harness = Harness::new();
    harness.reset();
    let template = template_pda(7);
    let mut data = vec![IX_CREATE_TEMPLATE];
    data.extend_from_slice(&7u16.to_le_bytes());
    data.extend_from_slice(&solana_sha256_hasher::hash(&payload).to_bytes());
    data.extend_from_slice(&payload);
    harness.context.account_store.borrow_mut().insert(CREATOR, system_account(10_000_000_000));
    harness.context.account_store.borrow_mut().insert(template, system_account(0));
    let result = harness.context.process_instruction(&Instruction {
        program_id: BALLISTA_ID,
        accounts: vec![
            AccountMeta::new(CREATOR, true),
            AccountMeta::new(template, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    });
    assert_eq!(
        custom(&result).map(|(kind, _)| kind),
        Some(INVALID_CPI),
        "a CPI exceeding the account's declared privilege must be rejected with InvalidCpi; deleting the check lets create succeed"
    );
}
