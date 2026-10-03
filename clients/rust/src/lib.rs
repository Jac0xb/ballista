//! Rust client for Ballista: template addresses, lifecycle and run instruction codecs, typed run
//! input encoding, error decoding, run output decoding from transaction logs, and re-exports of
//! the shared authoring builder.
//!
//! Authoring from Rust uses [`template`], a declarative API that mirrors the TypeScript SDK's
//! `defineTemplate` one to one and compiles to the same bytes, with the same checks: see
//! `examples/docs_templates.rs`. [`template::CompiledTemplate::run`] builds a run by name; see
//! `examples/docs_runs.rs`. Without the definition, [`run_instruction`] takes the account metas in
//! declaration order and inputs encoded with [`RunInputs`]; see `examples/run_template.rs`.
//! [`program_data`]
//! and [`decode_run_event`] read a run's events and `EMIT` output back from the transaction's
//! logs.

use ballista_common::instruction::*;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

mod logs;
pub mod template;

pub use ballista_common;
pub use ballista_common::template::{
    decode_ballista_error, DecodedError, ErrorSource, ProgramBuilder, Segment, MAX_REGISTRIES,
    REGISTRY_SEED,
};
pub use logs::{
    decode_run_event, program_data, BallistaOutput, LogError, ProgramDataLine, RunEvent,
    RUN_EVENT_LEN,
};

/// The address of the pre-release devnet build, which the functions without `_for_program`
/// use. That build has an upgrade authority and rejects templates from this repository: pass
/// your own deployment's address to the `_for_program` functions.
pub const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
pub const BALLISTA_ID: Pubkey = ID;
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");
pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
/// Token-2022. Its token accounts share the SPL Token layout's first 165 bytes.
pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey =
    pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
/// The Instructions sysvar. Introspecting templates declare it as a fixed account pinned to this
/// address; pass it read-only, as `AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR_ID, false)`.
pub const INSTRUCTIONS_SYSVAR_ID: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
/// The Ed25519 signature-verification precompile.
pub const ED25519_PROGRAM_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");
pub const TEMPLATE_SEED: &[u8] = b"template";

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

/// A registry entry's address and bump: `["registry", template, [registry index], key]` under
/// Ballista, where the entry's open creates it. `registry_index` is the registry's position among
/// those the template declares, below [`MAX_REGISTRIES`]. `key` is the 32 bytes the template
/// computes for the entry: an address for a per-caller or per-account entry, all zeros for one
/// template-wide entry.
///
/// Panics if `registry_index` is not below [`MAX_REGISTRIES`]: no template can declare that many
/// registries, so there is no entry to derive. The TypeScript SDK's `findRegistryEntryAddress`
/// throws for the same input.
pub fn find_registry_entry_address(
    template: &Pubkey,
    registry_index: u8,
    key: &[u8; 32],
) -> (Pubkey, u8) {
    find_registry_entry_address_for_program(template, registry_index, key, &ID)
}

/// [`find_registry_entry_address`] under a specific deployment of the program, which owns the
/// entries of the templates it finalized.
///
/// Panics if `registry_index` is not below [`MAX_REGISTRIES`].
pub fn find_registry_entry_address_for_program(
    template: &Pubkey,
    registry_index: u8,
    key: &[u8; 32],
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    assert!(
        (registry_index as usize) < MAX_REGISTRIES,
        "registry_index must be below MAX_REGISTRIES ({MAX_REGISTRIES}), got {registry_index}",
    );
    Pubkey::find_program_address(
        &[REGISTRY_SEED, template.as_ref(), &[registry_index], key],
        program_id,
    )
}

pub fn template_hash(payload: &[u8]) -> [u8; 32] {
    solana_sha256_hasher::hash(payload).to_bytes()
}

/// Encodes run inputs in the order the template declares them.
///
/// Run data is: one length byte per declared account group, the fixed input values, then one
/// row of values per batch iteration. Call [`RunInputs::groups`] first when the template declares
/// groups, then append fixed values, then each row's values in iteration order.
///
/// ```
/// use ballista_sdk::RunInputs;
///
/// let inputs = RunInputs::new().bool(true).u64(55_000).finish();
/// assert_eq!(inputs, vec![1, 0xd8, 0xd6, 0, 0, 0, 0, 0, 0]);
///
/// // Two groups holding 3 and 0 accounts, one fixed u64, then two rows of one u64 each.
/// let batched = RunInputs::new().groups(&[3, 0]).u64(1).u64(10).u64(20).finish();
/// assert_eq!(batched.len(), 2 + 8 * 3);
/// assert_eq!(&batched[..2], &[3, 0]);
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RunInputs {
    bytes: Vec<u8>,
}

impl RunInputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// The account-group length prefix: one byte per declared group, in declaration order. Must
    /// come before any value.
    pub fn groups(mut self, lengths: &[u8]) -> Self {
        assert!(self.bytes.is_empty(), "group lengths come before the input values");
        self.bytes.extend_from_slice(lengths);
        self
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

/// An Anchor instruction discriminator: the first eight bytes of `sha256("global:<name>")`, where
/// `name` is the instruction handler's snake_case name. An Anchor program expects it at the start
/// of the instruction data, as a template's CPI to one starts with a `Segment::Literal` of it.
pub fn anchor_discriminator(name: &str) -> [u8; 8] {
    let hash = solana_sha256_hasher::hashv(&[b"global:", name.as_bytes()]).to_bytes();
    let mut discriminator = [0; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

pub fn create_template_instruction(
    creator: Pubkey,
    template_id: u16,
    payload: &[u8],
) -> Instruction {
    create_template_instruction_for_program(creator, template_id, payload, &ID)
}

/// [`create_template_instruction`] for the deployment at `program_id`, under which the template's
/// address is derived.
pub fn create_template_instruction_for_program(
    creator: Pubkey,
    template_id: u16,
    payload: &[u8],
    program_id: &Pubkey,
) -> Instruction {
    let (template, _) = find_template_pda_for_program(&creator, template_id, program_id);
    let mut data = Vec::with_capacity(35 + payload.len());
    data.push(IX_CREATE_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&template_hash(payload));
    data.extend_from_slice(payload);
    Instruction {
        program_id: *program_id,
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
    begin_template_instruction_for_program(creator, template_id, payload_len, payload_hash, &ID)
}

/// [`begin_template_instruction`] for the deployment at `program_id`, under which the template's
/// address is derived.
pub fn begin_template_instruction_for_program(
    creator: Pubkey,
    template_id: u16,
    payload_len: u32,
    payload_hash: [u8; 32],
    program_id: &Pubkey,
) -> Instruction {
    let (template, _) = find_template_pda_for_program(&creator, template_id, program_id);
    let mut data = Vec::with_capacity(39);
    data.push(IX_BEGIN_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&payload_len.to_le_bytes());
    data.extend_from_slice(&payload_hash);
    Instruction {
        program_id: *program_id,
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
    write_template_chunk_instruction_for_program(creator, template, offset, bytes, &ID)
}

/// [`write_template_chunk_instruction`] for the deployment at `program_id`. `template` is the
/// address [`find_template_pda_for_program`] derives under it.
pub fn write_template_chunk_instruction_for_program(
    creator: Pubkey,
    template: Pubkey,
    offset: u32,
    bytes: &[u8],
    program_id: &Pubkey,
) -> Instruction {
    let mut data = Vec::with_capacity(5 + bytes.len());
    data.push(IX_WRITE_TEMPLATE_CHUNK);
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(bytes);
    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data,
    }
}

pub fn finalize_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
    finalize_template_instruction_for_program(creator, template, &ID)
}

/// [`finalize_template_instruction`] for the deployment at `program_id`. `template` is the address
/// [`find_template_pda_for_program`] derives under it.
pub fn finalize_template_instruction_for_program(
    creator: Pubkey,
    template: Pubkey,
    program_id: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data: vec![IX_FINALIZE_TEMPLATE],
    }
}

pub fn cancel_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
    cancel_template_instruction_for_program(creator, template, &ID)
}

/// [`cancel_template_instruction`] for the deployment at `program_id`. `template` is the address
/// [`find_template_pda_for_program`] derives under it.
pub fn cancel_template_instruction_for_program(
    creator: Pubkey,
    template: Pubkey,
    program_id: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: *program_id,
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
    run_instruction_for_program(template, runtime_accounts, input_bytes, &ID)
}

/// [`run_instruction`] for the deployment at `program_id`, which finalized `template`.
pub fn run_instruction_for_program(
    template: Pubkey,
    runtime_accounts: Vec<AccountMeta>,
    input_bytes: &[u8],
    program_id: &Pubkey,
) -> Instruction {
    let mut data = Vec::with_capacity(1 + input_bytes.len());
    data.push(IX_RUN);
    data.extend_from_slice(input_bytes);
    let mut accounts = Vec::with_capacity(1 + runtime_accounts.len());
    accounts.push(AccountMeta::new_readonly(template, false));
    accounts.extend(runtime_accounts);
    Instruction {
        program_id: *program_id,
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

    /// Each builder is its `_for_program` variant with `ID`, and the variant addresses the program
    /// it is given, deriving a template address under that program where it derives one.
    #[test]
    fn every_builder_has_a_variant_for_another_deployment() {
        let creator = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let (template, _) = find_template_pda(&creator, 3);
        let (other_template, _) = find_template_pda_for_program(&creator, 3, &other);
        assert_ne!(template, other_template);
        let metas = vec![AccountMeta::new(Pubkey::new_unique(), true)];
        let builders: [(Instruction, Instruction, Instruction); 6] = [
            (
                create_template_instruction(creator, 3, &[1, 2]),
                create_template_instruction_for_program(creator, 3, &[1, 2], &ID),
                create_template_instruction_for_program(creator, 3, &[1, 2], &other),
            ),
            (
                begin_template_instruction(creator, 3, 9, [4; 32]),
                begin_template_instruction_for_program(creator, 3, 9, [4; 32], &ID),
                begin_template_instruction_for_program(creator, 3, 9, [4; 32], &other),
            ),
            (
                write_template_chunk_instruction(creator, template, 5, &[6]),
                write_template_chunk_instruction_for_program(creator, template, 5, &[6], &ID),
                write_template_chunk_instruction_for_program(
                    creator,
                    other_template,
                    5,
                    &[6],
                    &other,
                ),
            ),
            (
                finalize_template_instruction(creator, template),
                finalize_template_instruction_for_program(creator, template, &ID),
                finalize_template_instruction_for_program(creator, other_template, &other),
            ),
            (
                cancel_template_instruction(creator, template),
                cancel_template_instruction_for_program(creator, template, &ID),
                cancel_template_instruction_for_program(creator, other_template, &other),
            ),
            (
                run_instruction(template, metas.clone(), &[7]),
                run_instruction_for_program(template, metas.clone(), &[7], &ID),
                run_instruction_for_program(other_template, metas.clone(), &[7], &other),
            ),
        ];
        for (default, with_id, elsewhere) in builders {
            assert_eq!(default, with_id);
            assert_eq!(default.program_id, ID);
            assert_eq!(elsewhere.program_id, other);
            assert_eq!(elsewhere.data, default.data);
            let template_at = |instruction: &Instruction| {
                instruction
                    .accounts
                    .iter()
                    .position(|meta| meta.pubkey == template || meta.pubkey == other_template)
                    .map(|at| instruction.accounts[at].pubkey)
            };
            assert_eq!(template_at(&default), Some(template), "{default:?}");
            assert_eq!(
                template_at(&elsewhere),
                Some(other_template),
                "{elsewhere:?}"
            );
        }
    }

    #[test]
    fn anchor_discriminators_hash_the_global_namespace() {
        // sha256("global:initialize") and sha256("global:route"), Jupiter's swap, first 8 bytes.
        assert_eq!(
            anchor_discriminator("initialize"),
            [0xaf, 0xaf, 0x6d, 0x1f, 0x0d, 0x98, 0x9b, 0xed]
        );
        assert_eq!(
            anchor_discriminator("route"),
            [0xe5, 0x17, 0xcb, 0x97, 0x7a, 0xe3, 0xad, 0x2a]
        );
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

    /// The vectors in `fixtures/registry-entry-addresses.txt` are the program's own derivation,
    /// checked by its `registry_addresses_match_the_shared_vectors` test.
    #[test]
    fn registry_entry_addresses_match_the_programs_vectors() {
        let bytes = |hex: &str| -> [u8; 32] {
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect();
            bytes.try_into().unwrap()
        };
        let vectors: Vec<&str> = include_str!("../../../fixtures/registry-entry-addresses.txt")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert_eq!(vectors.len(), 64);
        for line in vectors {
            let fields: Vec<&str> = line.split(' ').collect();
            let template = Pubkey::new_from_array(bytes(fields[0]));
            let index: u8 = fields[1].parse().unwrap();
            let key = bytes(fields[2]);
            let expected = (Pubkey::new_from_array(bytes(fields[3])), fields[4].parse().unwrap());
            assert_eq!(find_registry_entry_address(&template, index, &key), expected, "{line}");
        }
    }

    /// The TypeScript SDK's `findRegistryEntryAddress` throws a `RangeError` for the same input:
    /// no template can declare `MAX_REGISTRIES` registries, so there is no entry to derive.
    #[test]
    #[should_panic(expected = "registry_index must be below MAX_REGISTRIES")]
    fn find_registry_entry_address_panics_at_max_registries() {
        let template = Pubkey::new_unique();
        find_registry_entry_address(&template, MAX_REGISTRIES as u8, &[0; 32]);
    }

    #[test]
    fn well_known_addresses_match_the_shared_constants() {
        assert_eq!(
            INSTRUCTIONS_SYSVAR_ID.to_bytes(),
            ballista_common::template::INSTRUCTIONS_SYSVAR_ID
        );
        assert_eq!(INSTRUCTIONS_SYSVAR_ID, solana_program::sysvar::instructions::ID);
        assert_eq!(ED25519_PROGRAM_ID, solana_program::ed25519_program::ID);
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
        assert_eq!(decode_ballista_error(6022).unwrap().name, "LoopCountExceeded");
        assert_eq!(decode_ballista_error(6129).unwrap().name, "InvalidLoop");
        assert!(decode_ballista_error(1).is_none());
        assert_eq!(decode_ballista_error(6130).unwrap().name, "InvalidOutput");
        assert_eq!(decode_ballista_error(6023).unwrap().name, "InstructionOutOfRange");
        assert_eq!(
            decode_ballista_error((5 << 16) | 6024).unwrap().name,
            "WritableAccountBytesRead"
        );
        assert_eq!(decode_ballista_error(6131).unwrap().name, "InvalidIntrospection");
        assert_eq!(
            decode_ballista_error((3 << 16) | 6025).unwrap().name,
            "InvalidRegistryEntry"
        );
        assert_eq!(decode_ballista_error(6026).unwrap().name, "RegistryReentry");
        assert_eq!(decode_ballista_error(6132).unwrap().name, "InvalidRegistry");
        assert!(decode_ballista_error(6027).is_none());
        assert_eq!(decode_ballista_error(6133).unwrap().name, "InvalidAccountGroup");
        assert!(decode_ballista_error(6134).is_none());
    }
}
