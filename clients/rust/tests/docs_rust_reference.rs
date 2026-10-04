//! Runs the examples on the Rust SDK reference page, `examples/docs_rust_reference.rs`: the
//! templates compile and verify, the instructions carry what the page says, and every upload
//! transaction fits.

#[path = "../examples/docs_rust_reference.rs"]
mod reference;

use ballista_sdk::{
    anchor_discriminator, ballista_common::instruction::IX_RUN, ballista_common::template::*,
    find_registry_entry_address, find_template_pda, find_template_pda_for_program, template_hash,
    write_template_chunk_instruction, ID, SYSTEM_PROGRAM_ID,
};
use solana_program::{instruction::Instruction, pubkey::Pubkey};

#[test]
fn the_templates_verify() {
    let templates = [
        ("author", reference::payroll()),
        ("output", reference::log_and_return(reference::payroll())),
        ("introspection", reference::after_an_ed25519_instruction()),
        ("registry", reference::capped_total()),
        ("own-program", reference::deposit_into(Pubkey::new_unique())),
    ];
    for (name, template) in templates {
        let compiled = template
            .compile()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let program = ProgramView::parse(&compiled.bytes).unwrap();
        program
            .verify()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        if name == "output" {
            assert_eq!(program.header.flags(), PROGRAM_FLAG_EMIT_EVENT);
        }
    }
}

#[test]
fn the_call_to_your_own_program_starts_with_its_discriminator() {
    let payload = reference::deposit_into(Pubkey::new_unique())
        .compile()
        .unwrap()
        .bytes;
    let discriminator = anchor_discriminator("deposit");
    assert!(
        payload.windows(8).any(|window| window == discriminator),
        "the CPI data starts with the literal discriminator"
    );
}

#[test]
fn addresses_and_runs_are_the_sdk_s() {
    let (creator, caller, other) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let template = find_template_pda(&creator, 7).0;
    assert_eq!(
        reference::addresses(&creator, 7, &other, &caller).unwrap(),
        [
            template,
            find_template_pda_for_program(&creator, 7, &other).0,
            find_registry_entry_address(&template, 0, &caller.to_bytes()).0,
        ]
    );

    let (treasury, recipients) = (
        Pubkey::new_unique(),
        [Pubkey::new_unique(), Pubkey::new_unique()],
    );
    let run = reference::run_payroll(template, treasury, &recipients, 5, 10).unwrap();
    let keys: Vec<(Pubkey, bool, bool)> = run
        .accounts
        .iter()
        .map(|meta| (meta.pubkey, meta.is_signer, meta.is_writable))
        .collect();
    assert_eq!(
        keys,
        [
            (template, false, false),
            (SYSTEM_PROGRAM_ID, false, false),
            (treasury, true, true),
            (recipients[0], false, true),
            (recipients[1], false, true),
        ]
    );
    let mut data = vec![IX_RUN];
    data.extend_from_slice(&5u64.to_le_bytes());
    data.extend_from_slice(&10u64.to_le_bytes());
    assert_eq!(run.data, data);
}

#[test]
fn run_output_is_read_back_from_the_logs_and_the_return_data() {
    let logs = vec![
        format!("Program {ID} invoke [1]"),
        "Program data: UEFJRAUAAAAAAAAA".to_owned(), // PAID, then 5 as a u64
        format!("Program {ID} success"),
    ];
    assert_eq!(reference::print_run_output(&logs), Ok(()));
    assert_eq!(reference::returned_total(&ID, &5u64.to_le_bytes()), Some(5));
    let token = ballista_sdk::TOKEN_PROGRAM_ID;
    assert_eq!(reference::returned_total(&token, &5u64.to_le_bytes()), None);
    assert_eq!(reference::returned_total(&ID, &[5]), None, "too short");
}

#[test]
fn failures_and_template_accounts_decode() {
    assert_eq!(
        reference::describe((7 << 16) | 6015).as_deref(),
        Some("RequirementFailed (program counter 7)")
    );
    assert_eq!(
        reference::describe((2 << 16) | 6020).as_deref(),
        Some("AccountConstraintFailed (runtime account index 2)")
    );
    assert_eq!(reference::describe(1), None);
    // The payroll is the guide's budgeted payroll: the budget check is at program counter 8, as
    // the TypeScript test of `explainRunError` finds.
    let compiled = reference::payroll().compile().unwrap();
    assert_eq!(
        compiled.explain_error((8 << 16) | 6015).as_deref(),
        Some("RequirementFailed at steps[2] (withinBudget)")
    );
    assert_eq!(
        compiled.explain_error((3 << 16) | 6020).as_deref(),
        Some("AccountConstraintFailed: account recipient in row 1 does not satisfy its constraint")
    );
    assert_eq!(compiled.explain_error(1), None);

    let payload = reference::payroll().compile().unwrap().bytes;
    let mut header = TemplateAccountHeader::new_uploading(
        [1; 32],
        7,
        255,
        payload.len(),
        template_hash(&payload),
    )
    .unwrap();
    header.set_written_len(payload.len()).unwrap();
    header.finalize().unwrap();
    let mut data = header.as_bytes().to_vec();
    data.extend_from_slice(&payload);
    let stats = reference::inspect(&data).unwrap();
    assert_eq!(
        stats.max_expanded_cpis, 30,
        "one transfer per row, at most 30 rows"
    );
}

/// The bytes a short-vec length prefix takes.
fn short_vec_len(len: usize) -> usize {
    match len {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        _ => 3,
    }
}

/// The size of a legacy transaction that `payer` signs and pays for, carrying `instructions`: the
/// signatures, then the message's header, account keys, blockhash, and instructions.
fn legacy_transaction_len(payer: &Pubkey, instructions: &[Instruction]) -> usize {
    let mut keys: Vec<(Pubkey, bool)> = vec![(*payer, true)];
    for instruction in instructions {
        let metas = instruction
            .accounts
            .iter()
            .map(|meta| (meta.pubkey, meta.is_signer));
        for (key, signer) in metas.chain([(instruction.program_id, false)]) {
            match keys.iter_mut().find(|(known, _)| *known == key) {
                Some(known) => known.1 |= signer,
                None => keys.push((key, signer)),
            }
        }
    }
    let signers = keys.iter().filter(|(_, signer)| *signer).count();
    let instructions_len: usize = instructions
        .iter()
        .map(|instruction| {
            let (accounts, data) = (instruction.accounts.len(), instruction.data.len());
            1 + short_vec_len(accounts) + accounts + short_vec_len(data) + data
        })
        .sum();
    short_vec_len(signers)
        + 64 * signers
        + 3
        + short_vec_len(keys.len())
        + 32 * keys.len()
        + 32
        + short_vec_len(instructions.len())
        + instructions_len
}

/// A legacy transaction holds at most 1,232 bytes. Each instruction `upload` returns fits in one
/// that the creator signs and pays for, and a full chunk or the largest one-shot payload fills it.
#[test]
fn every_upload_transaction_fits_and_a_full_one_fills_it() {
    const LIMIT: usize = 1_232;
    let creator = Pubkey::new_unique();
    let size = |instruction: &Instruction| {
        legacy_transaction_len(&creator, std::slice::from_ref(instruction))
    };

    let one_shot = reference::upload(creator, 1, &[7; 960]);
    assert_eq!(one_shot.len(), 1);
    assert_eq!(
        size(&one_shot[0]),
        LIMIT,
        "the largest one-shot payload fills the transaction"
    );
    assert_eq!(
        reference::upload(creator, 1, &[7; 961]).len(),
        3,
        "begin, a write, finalize"
    );

    let chunked = reference::upload(creator, 1, &[7; 1_023 * 2 + 1]);
    assert_eq!(chunked.len(), 5, "begin, three writes, finalize");
    assert_eq!(
        size(&chunked[1]),
        LIMIT,
        "a full chunk fills the transaction"
    );
    assert!(chunked.iter().all(|instruction| size(instruction) <= LIMIT));
    let largest = reference::upload(creator, 1, &[7; MAX_TEMPLATE_PAYLOAD_LEN]);
    assert!(largest.iter().all(|instruction| size(instruction) <= LIMIT));

    // Sizes measured by serializing real transactions with solana-transaction 4.3: one byte past
    // each limit no longer fits, and a 3,000-byte chunk makes a 3,209-byte transaction.
    let template = find_template_pda(&creator, 1).0;
    let write = |len: usize| write_template_chunk_instruction(creator, template, 0, &vec![0; len]);
    assert_eq!(size(&write(1_024)), LIMIT + 1);
    assert_eq!(size(&write(3_000)), 3_209);
    let create = |len: usize| ballista_sdk::create_template_instruction(creator, 1, &vec![0; len]);
    assert_eq!(size(&create(961)), LIMIT + 1);
    assert_eq!(size(&chunked[0]), 275, "begin");
    assert_eq!(size(&chunked[4]), 204, "finalize");
}
