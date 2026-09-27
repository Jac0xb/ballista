//! The harness can load Ballista built from source, then upload a template and run it, each as a
//! real signed transaction.

use {
    ballista_sdk::{
        create_template_instruction, find_template_pda, run_instruction, RunInputs,
        SYSTEM_PROGRAM_ID,
    },
    litesvm::{types::TransactionMetadata, LiteSVM},
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
    solana_transaction::Transaction,
};

/// Written by `cargo build-sbf --manifest-path programs/ballista/Cargo.toml`.
const BALLISTA_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/ballista.so"
);
/// Input `amount: u64`. Accounts: `systemProgram` (pinned), `source` (signer, writable),
/// `destination` (writable).
const SYSTEM_TRANSFER_HEX: &str = include_str!("../../../fixtures/system-transfer.hex");
const SOL: u64 = 1_000_000_000;
const TEMPLATE_ID: u16 = 1;
const AMOUNT: u64 = 1_000_000;

fn decode_hex(text: &str) -> Vec<u8> {
    let text = text.trim();
    assert_eq!(text.len() % 2, 0, "hex has an odd number of digits");
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex digit"))
        .collect()
}

/// A fixed key keeps compute units deterministic: the template address's bump search depends on
/// the creator.
fn keypair(seed: &[u8; 32]) -> Keypair {
    Keypair::new_from_array(*seed)
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction, what: &str) -> TransactionMetadata {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap_or_else(|failure| {
        panic!(
            "{what} failed: {:?}\n{}",
            failure.err,
            failure.meta.pretty_logs()
        )
    })
}

#[test]
fn uploads_and_runs_the_system_transfer_template() {
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(ballista_sdk::ID, BALLISTA_SO)
        .unwrap_or_else(|error| {
            panic!("loading {BALLISTA_SO} failed (build it with cargo build-sbf): {error:?}")
        });

    let creator = keypair(b"ballista-protocol-tests-creator1");
    let sender = keypair(b"ballista-protocol-tests-sender-1");
    let recipient = keypair(b"ballista-protocol-tests-receiver").pubkey();
    for key in [creator.pubkey(), sender.pubkey(), recipient] {
        svm.airdrop(&key, 10 * SOL).unwrap();
    }

    let payload = decode_hex(SYSTEM_TRANSFER_HEX);
    let create = create_template_instruction(creator.pubkey(), TEMPLATE_ID, &payload);
    send(&mut svm, &creator, create, "create_template");
    let (template, _) = find_template_pda(&creator.pubkey(), TEMPLATE_ID);
    let stored = svm
        .get_account(&template)
        .expect("the template account exists");
    assert_eq!(stored.owner, ballista_sdk::ID);

    let sender_before = svm.get_balance(&sender.pubkey()).unwrap();
    let recipient_before = svm.get_balance(&recipient).unwrap();
    let run = run_instruction(
        template,
        vec![
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new(sender.pubkey(), true),
            AccountMeta::new(recipient, false),
        ],
        &RunInputs::new().u64(AMOUNT).finish(),
    );
    let meta = send(&mut svm, &sender, run, "run");

    assert_eq!(
        svm.get_balance(&recipient).unwrap(),
        recipient_before + AMOUNT
    );
    // The sender is also the fee payer.
    assert_eq!(
        svm.get_balance(&sender.pubkey()).unwrap(),
        sender_before - AMOUNT - meta.fee
    );
}
