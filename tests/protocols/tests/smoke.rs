//! The harness can load Ballista built from source, then upload a template and run it, each as a
//! real signed transaction.

use {
    ballista_protocol_tests::{
        decode_hex,
        snapshot::add_ballista,
        template::upload,
        tx,
        wallet::{fund, keypair, SOL},
    },
    ballista_sdk::{find_template_pda, run_instruction, RunInputs, SYSTEM_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_instruction::AccountMeta,
    solana_signer::Signer,
};

/// `system-transfer` in `clients/js/src/fixtures.test.ts`. Accounts: `systemProgram` (pinned),
/// `source` (signer, writable), `destination` (writable). Input: `amount: u64`.
const SYSTEM_TRANSFER_HEX: &str = include_str!("../../../fixtures/system-transfer.hex");
const TEMPLATE_ID: u16 = 1;
const AMOUNT: u64 = 1_000_000;

#[test]
fn uploads_and_runs_the_system_transfer_template() {
    let mut svm = LiteSVM::new();
    add_ballista(&mut svm);

    let creator = keypair(b"ballista-protocol-tests-creator1");
    let source = keypair(b"ballista-protocol-tests-sender-1");
    let destination = keypair(b"ballista-protocol-tests-receiver").pubkey();
    for key in [creator.pubkey(), source.pubkey(), destination] {
        fund(&mut svm, &key, 10 * SOL);
    }

    let payload = decode_hex(SYSTEM_TRANSFER_HEX);
    let template = upload(&mut svm, &creator, TEMPLATE_ID, &payload);
    assert_eq!(
        template,
        find_template_pda(&creator.pubkey(), TEMPLATE_ID).0
    );
    let stored = svm
        .get_account(&template)
        .expect("the template account exists");
    assert_eq!(stored.owner, ballista_sdk::ID);

    let run = run_instruction(
        template,
        vec![
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new(source.pubkey(), true),
            AccountMeta::new(destination, false),
        ],
        &RunInputs::new().u64(AMOUNT).finish(),
    );
    let outcome = tx::send(&mut svm, &source, &[], &[run], &[]).unwrap();

    assert_eq!(svm.get_balance(&destination), Some(10 * SOL + AMOUNT));
    // The source is also the fee payer.
    assert_eq!(
        svm.get_balance(&source.pubkey()),
        Some(10 * SOL - AMOUNT - outcome.fee)
    );
}
