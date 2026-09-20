//! Rust client for Ballista: template addresses, lifecycle and run instruction codecs, typed run
//! input encoding, error decoding, and re-exports of the shared authoring builder.
//!
//! Authoring from Rust uses [`ProgramBuilder`], which emits the same bytecode the TypeScript
//! compiler produces; see `examples/author_template.rs`. Running a template from Rust needs only
//! the template address, the account metas in schema order, and inputs encoded with
//! [`RunInputs`]; see `examples/run_template.rs`.

use ballista_common::instruction::*;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

pub use ballista_common;
pub use ballista_common::template::{
    decode_ballista_error, DecodedError, ErrorSource, ProgramBuilder, Segment,
};

pub const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
pub const BALLISTA_ID: Pubkey = ID;
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");
pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey =
    pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const TEMPLATE_SEED: &[u8] = b"template-v2";

pub fn find_template_pda(creator: &Pubkey, template_id: u16) -> (Pubkey, u8) {
    find_template_pda_for_program(creator, template_id, &ID)
}

/// Derives a template address under a specific deployment of the program. Each program version is
/// deployed immutably under its own address, so templates are bound to the deployment that
/// finalized them.
pub fn find_template_pda_for_program(
    creator: &Pubkey,
    template_id: u16,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        program_id,
    )
}

pub fn template_hash(payload: &[u8]) -> [u8; 32] {
    solana_sha256_hasher::hash(payload).to_bytes()
}

/// Encodes run inputs in the order the template declares them.
///
/// ```
/// use ballista_sdk::RunInputs;
///
/// let inputs = RunInputs::new().bool(true).u64(55_000).finish();
/// assert_eq!(inputs, vec![1, 0xd8, 0xd6, 0, 0, 0, 0, 0, 0]);
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RunInputs {
    bytes: Vec<u8>,
}

impl RunInputs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bool(mut self, value: bool) -> Self {
        self.bytes.push(u8::from(value));
        self
    }

    pub fn u64(mut self, value: u64) -> Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn i64(mut self, value: i64) -> Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u128(mut self, value: u128) -> Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn pubkey(mut self, value: &Pubkey) -> Self {
        self.bytes.extend_from_slice(value.as_ref());
        self
    }

    /// A `bytes` input: a little-endian `u16` length followed by the bytes.
    pub fn bytes(mut self, value: &[u8]) -> Self {
        let len = u16::try_from(value.len()).expect("bytes inputs are at most 1,024 bytes");
        self.bytes.extend_from_slice(&len.to_le_bytes());
        self.bytes.extend_from_slice(value);
        self
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

pub fn create_template_instruction(
    creator: Pubkey,
    template_id: u16,
    payload: &[u8],
) -> Instruction {
    let (template, _) = find_template_pda(&creator, template_id);
    let mut data = Vec::with_capacity(35 + payload.len());
    data.push(IX_CREATE_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&template_hash(payload));
    data.extend_from_slice(payload);
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn begin_template_instruction(
    creator: Pubkey,
    template_id: u16,
    payload_len: u32,
    payload_hash: [u8; 32],
) -> Instruction {
    let (template, _) = find_template_pda(&creator, template_id);
    let mut data = Vec::with_capacity(39);
    data.push(IX_BEGIN_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&payload_len.to_le_bytes());
    data.extend_from_slice(&payload_hash);
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn write_template_chunk_instruction(
    creator: Pubkey,
    template: Pubkey,
    offset: u32,
    bytes: &[u8],
) -> Instruction {
    let mut data = Vec::with_capacity(5 + bytes.len());
    data.push(IX_WRITE_TEMPLATE_CHUNK);
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(bytes);
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data,
    }
}

pub fn finalize_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data: vec![IX_FINALIZE_TEMPLATE],
    }
}

pub fn cancel_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data: vec![IX_CANCEL_TEMPLATE],
    }
}

pub fn run_instruction(
    template: Pubkey,
    runtime_accounts: Vec<AccountMeta>,
    input_bytes: &[u8],
) -> Instruction {
    let mut data = Vec::with_capacity(1 + input_bytes.len());
    data.push(IX_RUN);
    data.extend_from_slice(input_bytes);
    let mut accounts = Vec::with_capacity(1 + runtime_accounts.len());
    accounts.push(AccountMeta::new_readonly(template, false));
    accounts.extend(runtime_accounts);
    Instruction {
        program_id: ID,
        accounts,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ballista_common::template::{
        ProgramView, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, VALUE_U64,
    };

    #[test]
    fn instruction_codecs_use_the_v2_discriminators() {
        let creator = Pubkey::new_unique();
        let begin = begin_template_instruction(creator, 9, 10, [7; 32]);
        assert_eq!(begin.data[0], IX_BEGIN_TEMPLATE);
        assert_eq!(&begin.data[1..3], &9u16.to_le_bytes());

        let (template, _) = find_template_pda(&creator, 9);
        let run = run_instruction(template, vec![], &[1, 2]);
        assert_eq!(run.data, vec![IX_RUN, 1, 2]);
        assert_eq!(run.accounts[0].pubkey, template);
    }

    #[test]
    fn run_inputs_encode_like_the_typescript_sdk() {
        let key = Pubkey::new_from_array([9; 32]);
        let inputs = RunInputs::new()
            .bool(true)
            .u64(7)
            .i64(-1)
            .u128(1)
            .pubkey(&key)
            .bytes(&[1, 2, 3])
            .finish();
        let mut expected = vec![1];
        expected.extend_from_slice(&7u64.to_le_bytes());
        expected.extend_from_slice(&(-1i64).to_le_bytes());
        expected.extend_from_slice(&1u128.to_le_bytes());
        expected.extend_from_slice(&[9; 32]);
        expected.extend_from_slice(&[3, 0, 1, 2, 3]);
        assert_eq!(inputs, expected);
    }

    /// The Rust builder and the TypeScript compiler produce identical bytes for the same template
    /// when records are declared in the same order.
    #[test]
    fn rust_authored_transfer_matches_the_typescript_fixture() {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
        let sender = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let recipient = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let lamports = builder.input(VALUE_U64, 0);
        let amount = builder.load_input(lamports);
        let transfer_discriminator = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &[
                (sender, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (recipient, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(transfer_discriminator),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        builder.invoke(cpi, None);
        let bytes = builder.build().unwrap();
        ProgramView::parse(&bytes).unwrap().verify().unwrap();

        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex,
            include_str!("../../../fixtures/system-transfer.hex").trim(),
            "Rust authoring must stay byte-identical to the TypeScript compiler"
        );
    }

    #[test]
    fn error_codes_decode_with_context() {
        let decoded = decode_ballista_error((7 << 16) | 6015).unwrap();
        assert_eq!(decoded.name, "RequirementFailed");
        assert_eq!(decoded.context, 7);
        assert_eq!(decoded.source, ErrorSource::Runtime);
        let verifier = decode_ballista_error((3 << 16) | 6115).unwrap();
        assert_eq!(verifier.name, "InvalidCpi");
        assert_eq!(verifier.source, ErrorSource::Verifier);
        assert_eq!(decode_ballista_error(6021).unwrap().name, "CpiAccountLimitExceeded");
        assert_eq!(decode_ballista_error(6128).unwrap().name, "TooManyAccountGroups");
        assert!(decode_ballista_error(1).is_none());
        assert!(decode_ballista_error(6022).is_none());
        assert!(decode_ballista_error(6129).is_none());
    }
}
