//! Property tests: parsing and verifying arbitrary or mutated bytes must never panic.
//!
//! The on-chain program relies on `ProgramView::parse` and `verify` to reject every malformed
//! payload with an error. A panic here would abort finalization with no error code.

use ballista_common::template::{
    ProgramBuilder, ProgramView, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE,
    DATA_REG_U64, VALUE_U64,
};
use proptest::prelude::*;

/// A representative valid program: guarded system transfer inside a batch with a PDA check.
fn representative_program() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
    let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(4, 0);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let literal = builder.blob(&[2, 0, 0, 0]);
    let cpi = builder.cpi(
        system,
        &[
            (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(literal),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.for_each(0, |body| {
        let key = body.account_key(recipient);
        let seed = body.const_bytes(b"seed");
        let derived = body.derive_pda(system, &[Segment::Register(9, seed)]);
        let differs = body.binary(24, derived, key);
        body.invoke(cpi, Some(differs));
    });
    let bytes = builder.build().expect("representative program builds");
    ProgramView::parse(&bytes)
        .and_then(|program| program.verify())
        .expect("representative program verifies");
    bytes
}

fn parse_and_verify(bytes: &[u8]) {
    if let Ok(program) = ProgramView::parse(bytes) {
        let _ = program.verify();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2_048)) {
        parse_and_verify(&bytes);
    }

    #[test]
    fn a_valid_header_with_arbitrary_body_never_panics(
        body in proptest::collection::vec(any::<u8>(), 0..1_024),
        counts in proptest::collection::vec(any::<u8>(), 20),
    ) {
        // Keep magic and version valid so parsing reaches the section arithmetic.
        let mut bytes = b"BVM2\x03".to_vec();
        bytes.extend_from_slice(&counts[..19]);
        bytes.extend_from_slice(&body);
        parse_and_verify(&bytes);
    }

    #[test]
    fn single_byte_mutations_never_panic(index in 0usize..512, value in any::<u8>()) {
        let mut bytes = representative_program();
        if index < bytes.len() {
            bytes[index] = value;
        }
        parse_and_verify(&bytes);
    }

    #[test]
    fn truncations_and_extensions_never_panic(cut in 0usize..512, extra in proptest::collection::vec(any::<u8>(), 0..64)) {
        let mut bytes = representative_program();
        bytes.truncate(cut.min(bytes.len()));
        bytes.extend_from_slice(&extra);
        parse_and_verify(&bytes);
    }

    #[test]
    fn instruction_field_mutations_never_panic(
        instruction in 0usize..8,
        field in 0usize..16,
        value in any::<u8>(),
    ) {
        let mut bytes = representative_program();
        // Header, two fixed accounts, one row account, one input.
        let instruction_start = 24 + 3 * 8 + 4;
        let offset = instruction_start + instruction * 16 + field;
        if offset < bytes.len() {
            bytes[offset] = value;
        }
        parse_and_verify(&bytes);
    }
}
