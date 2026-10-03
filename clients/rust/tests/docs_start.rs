//! Holds the Rust code on the Getting started and Template lifecycle pages to what the pages say.
//!
//! The pages send each upload instruction in its own legacy transaction, which Solana caps at
//! 1,232 bytes. These tests build every transaction the upload code sends, for each protocol
//! template in `fixtures/protocol-examples.json` (several of them over 1 KB), and measure it. They
//! also hold Getting started's template to the bytes the TypeScript compiler produces.

#[path = "../examples/docs_start.rs"]
mod start;

use std::cell::RefCell;

use ballista_sdk::{
    ballista_common::instruction::{
        IX_BEGIN_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_WRITE_TEMPLATE_CHUNK,
    },
    create_template_instruction, find_template_pda, write_template_chunk_instruction,
};
use solana_program::{instruction::Instruction, pubkey::Pubkey};
use start::{solana_keypair::Keypair, solana_signature::Signature, solana_signer::Signer};

/// The most a legacy or version 0 transaction can hold.
const TRANSACTION_SIZE_LIMIT: usize = 1_232;

const PROTOCOL_EXAMPLES: &str = include_str!("../../../fixtures/protocol-examples.json");
const BENCHMARKS: &str = include_str!("../../../fixtures/benchmarks.json");

/// The wire size of a legacy transaction that carries `instructions` and that `fee_payer` pays
/// for: one 64-byte signature per signer, then the message's header, account keys, blockhash and
/// compiled instructions, each list after its compact-u16 length.
fn legacy_transaction_size(fee_payer: &Pubkey, instructions: &[Instruction]) -> usize {
    let mut keys = vec![(*fee_payer, true)];
    let mut add = |key: Pubkey, signer: bool| match keys.iter_mut().find(|(seen, _)| *seen == key) {
        Some(entry) => entry.1 |= signer,
        None => keys.push((key, signer)),
    };
    for instruction in instructions {
        for meta in &instruction.accounts {
            add(meta.pubkey, meta.is_signer);
        }
        add(instruction.program_id, false);
    }
    let signers = keys.iter().filter(|(_, signer)| *signer).count();
    let compiled: usize = instructions
        .iter()
        .map(|instruction| {
            1 + compact_len(instruction.accounts.len())
                + instruction.accounts.len()
                + compact_len(instruction.data.len())
                + instruction.data.len()
        })
        .sum();
    compact_len(signers)
        + 64 * signers
        + 3
        + compact_len(keys.len())
        + 32 * keys.len()
        + 32
        + compact_len(instructions.len())
        + compiled
}

/// Bytes in the compact-u16 encoding of `len`.
fn compact_len(len: usize) -> usize {
    match len {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        _ => 3,
    }
}

fn decode_hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}

/// Every protocol template's name and payload, from the pretty-printed fixture.
fn protocol_templates() -> Vec<(String, Vec<u8>)> {
    let mut templates = Vec::new();
    let mut name = None;
    for line in PROTOCOL_EXAMPLES.lines() {
        if let Some(entry) = line
            .strip_prefix("  \"")
            .and_then(|rest| rest.strip_suffix("\": {"))
        {
            name = Some(entry.to_string());
        } else if let Some(hex) = line.trim_start().strip_prefix("\"payload\": \"") {
            let hex = hex.trim_end_matches(',').trim_end_matches('"');
            templates.push((
                name.take().expect("a payload follows its name"),
                decode_hex(hex),
            ));
        }
    }
    templates
}

/// The sizes measured with Solana Kit's transaction encoder and with `solana-transaction`, which
/// the pages and the limits page quote.
#[test]
fn the_size_formula_matches_measured_transactions() {
    let creator = Pubkey::new_unique();
    let (template, _) = find_template_pda(&creator, 42);
    let write = |len: usize| write_template_chunk_instruction(creator, template, 0, &vec![0; len]);
    let create = |len: usize| create_template_instruction(creator, 42, &vec![0; len]);
    assert_eq!(legacy_transaction_size(&creator, &[write(1_023)]), 1_232);
    assert_eq!(legacy_transaction_size(&creator, &[write(1_024)]), 1_233);
    assert_eq!(legacy_transaction_size(&creator, &[write(3_000)]), 3_209);
    assert_eq!(legacy_transaction_size(&creator, &[create(960)]), 1_232);
    assert_eq!(legacy_transaction_size(&creator, &[create(961)]), 1_233);
}

#[test]
fn getting_started_uploads_its_template_in_one_transaction() {
    let payload = start::getting_started().unwrap();

    // The same bytes the TypeScript compiler produces for the guide's sweep.
    let entry = &BENCHMARKS[BENCHMARKS.find("\"sweep-above-a-reserve\": {").unwrap()..];
    let hex = &entry[entry.find("\"templateHex\": \"").unwrap() + 16..];
    assert_eq!(payload, decode_hex(&hex[..hex.find('"').unwrap()]));

    let creator = Pubkey::new_unique();
    let create = create_template_instruction(creator, 7, &payload);
    assert!(legacy_transaction_size(&creator, &[create]) <= TRANSACTION_SIZE_LIMIT);
}

#[test]
fn the_upload_in_pieces_fits_every_transaction_for_every_protocol_template() {
    let templates = protocol_templates();
    let over_1_kb = templates
        .iter()
        .filter(|(_, payload)| payload.len() > 1_024);
    assert!(over_1_kb.count() > 1, "several templates are over 1 KB");

    for (name, payload) in templates {
        let sent = RefCell::new(Vec::new());
        let send = |fee_payer: &Keypair, instructions: &[Instruction]| {
            let size = legacy_transaction_size(&fee_payer.pubkey(), instructions);
            assert!(
                size <= TRANSACTION_SIZE_LIMIT,
                "{name}: a {size}-byte transaction"
            );
            sent.borrow_mut()
                .extend(instructions.iter().map(|ix| ix.data.clone()));
            Ok(Signature)
        };
        start::chunked_upload(send, Keypair::new(), payload.clone()).unwrap();

        // A begin, writes that lay the payload down in order, and a finalize.
        let sent = sent.into_inner();
        assert_eq!(sent.first().unwrap()[0], IX_BEGIN_TEMPLATE, "{name}");
        assert_eq!(sent.last().unwrap(), &vec![IX_FINALIZE_TEMPLATE], "{name}");
        let mut written = Vec::new();
        for data in &sent[1..sent.len() - 1] {
            assert_eq!(data[0], IX_WRITE_TEMPLATE_CHUNK, "{name}");
            let offset = u32::from_le_bytes(data[1..5].try_into().unwrap()) as usize;
            assert_eq!(
                offset,
                written.len(),
                "{name}: writes start where the last ended"
            );
            written.extend_from_slice(&data[5..]);
        }
        assert_eq!(written, payload, "{name}");
    }
}
