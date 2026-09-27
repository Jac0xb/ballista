use core::{fmt, mem::size_of};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const TEMPLATE_PROGRAM_MAGIC: [u8; 4] = *b"BVM1";
pub const TEMPLATE_PROGRAM_VERSION: u8 = 1;

pub const MAX_TEMPLATE_PAYLOAD_LEN: usize = 10_240;
pub const MAX_RUNTIME_ACCOUNTS: usize = 120;
pub const MAX_INPUTS: usize = 32;
pub const MAX_INPUT_BYTES: usize = 1_024;
pub const MAX_REGISTERS: usize = 64;
pub const MAX_VM_INSTRUCTIONS: usize = 128;
pub const MAX_EXPANDED_CPIS: usize = 64;
pub const MAX_CPI_DATA_LEN: usize = 4_096;
/// Upper bound on accounts passed to one CPI; matches the executor's stack-bounded invoke.
pub const MAX_CPI_ACCOUNTS: usize = 64;
pub const MAX_BATCH_STRIDE: usize = 8;
/// Row inputs a batch may declare; each is carried once per iteration in the run data.
pub const MAX_ROW_INPUTS: usize = 8;
/// Parsed input values per run: the fixed inputs plus `iterations × row inputs`.
pub const MAX_INPUT_VALUES: usize = 256;
/// Caller-sized account groups a template may declare and forward to CPIs.
pub const MAX_ACCOUNT_GROUPS: usize = 8;
/// Top-level loops a template may hold. They run one after another and never nest.
pub const MAX_LOOPS: usize = 8;
pub const MAX_PDA_SEEDS: usize = 15;
pub const MAX_PDA_SEED_LEN: usize = 32;
/// Maximum bytes of CPI return data the runtime exposes.
pub const MAX_RETURN_DATA_LEN: usize = 1_024;

/// `Sysvar1nstructions1111111111111111111111111`. Introspection opcodes read the transaction's
/// instructions from this sysvar, which a template declares as a fixed account pinned to it.
pub const INSTRUCTIONS_SYSVAR_ID: [u8; 32] = [
    0x06, 0xa7, 0xd5, 0x17, 0x18, 0x7b, 0xd1, 0x66, 0x35, 0xda, 0xd4, 0x04, 0x55, 0xfd, 0xc2, 0xc0,
    0xc1, 0x24, 0xc6, 0x8f, 0x21, 0x56, 0x75, 0xa5, 0xdb, 0xba, 0xcb, 0x5f, 0x08, 0x00, 0x00, 0x00,
];

pub const NO_INDEX: u8 = u8::MAX;
pub const ITERATION_ACCOUNT_BIT: u8 = 0x80;
/// `LOAD_INPUT` operand bit selecting a row input of the current iteration; the low seven bits
/// are the offset within the row.
pub const ITERATION_INPUT_BIT: u8 = ITERATION_ACCOUNT_BIT;

/// Program header flag: emit a `sol_log_data` event after a successful run.
pub const PROGRAM_FLAG_EMIT_EVENT: u8 = 1 << 0;
pub const PROGRAM_FLAGS_MASK: u8 = PROGRAM_FLAG_EMIT_EVENT;
/// The run event's tag family: the first three bytes of its magic, `BEV1`, which every version of
/// the event keeps. No [`OP_EMIT`] may start its log line with them.
pub const RUN_EVENT_TAG_FAMILY: [u8; 3] = *b"BEV";
/// The shortest literal tag an [`OP_EMIT`] may start with.
pub const MIN_EMIT_TAG_LEN: usize = 4;

/// Instruction flag (read opcodes only): the data offset comes from register `b` instead of the immediate.
pub const INSTRUCTION_FLAG_DYNAMIC_OFFSET: u8 = 1 << 0;

/// Base of the on-chain custom error codes for runtime failures.
pub const RUNTIME_ERROR_BASE: u32 = 6_000;
/// Base of the on-chain custom error codes reserved for verifier failures.
pub const VERIFIER_ERROR_BASE: u32 = 6_100;

/// Runtime error names in code order, starting at [`RUNTIME_ERROR_BASE`]. The program's error
/// enum and the SDKs are checked against this table.
pub const RUNTIME_ERROR_NAMES: [&str; 25] = [
    "InvalidInstructionData",
    "InvalidTemplateAccount",
    "InvalidTemplateProgram",
    "TemplateNotUploading",
    "TemplateNotFinalized",
    "InvalidCreator",
    "InvalidChunkOffset",
    "HashMismatch",
    "InvalidRunInputs",
    "InvalidRuntimeAccount",
    "InvalidAccountRange",
    "InvalidRegister",
    "TypeMismatch",
    "ArithmeticOverflow",
    "DivisionByZero",
    "RequirementFailed",
    "CpiDataTooLarge",
    "InvalidPdaDerivation",
    "MissingReturnData",
    "ReturnDataMismatch",
    "AccountConstraintFailed",
    "CpiAccountLimitExceeded",
    "LoopCountExceeded",
    "InstructionOutOfRange",
    "WritableAccountBytesRead",
];

pub const ACCOUNT_SIGNER: u8 = 1 << 0;
pub const ACCOUNT_WRITABLE: u8 = 1 << 1;
pub const ACCOUNT_EXECUTABLE: u8 = 1 << 2;
pub const ACCOUNT_FLAGS_MASK: u8 = ACCOUNT_SIGNER | ACCOUNT_WRITABLE | ACCOUNT_EXECUTABLE;

pub const VALUE_BOOL: u8 = 1;
pub const VALUE_U64: u8 = 2;
pub const VALUE_I64: u8 = 3;
pub const VALUE_U128: u8 = 4;
pub const VALUE_PUBKEY: u8 = 5;
pub const VALUE_BYTES: u8 = 6;

pub const OP_LOAD_INPUT: u8 = 1;
pub const OP_CONST_BOOL: u8 = 2;
pub const OP_CONST_U64: u8 = 3;
pub const OP_CONST_I64: u8 = 4;
pub const OP_CONST_U128: u8 = 5;
pub const OP_CONST_PUBKEY: u8 = 6;
pub const OP_CONST_BYTES: u8 = 7;
pub const OP_ACCOUNT_KEY: u8 = 8;
pub const OP_ACCOUNT_OWNER: u8 = 9;
pub const OP_ACCOUNT_LAMPORTS: u8 = 10;
pub const OP_ACCOUNT_DATA_LEN: u8 = 11;
pub const OP_ACCOUNT_IS_EMPTY: u8 = 12;
pub const OP_READ_U64: u8 = 13;
pub const OP_READ_I64: u8 = 14;
pub const OP_READ_U128: u8 = 15;
pub const OP_READ_PUBKEY: u8 = 16;
pub const OP_CLOCK_SLOT: u8 = 17;
pub const OP_CLOCK_TIMESTAMP: u8 = 18;
pub const OP_ADD: u8 = 19;
pub const OP_SUB: u8 = 20;
pub const OP_MUL: u8 = 21;
pub const OP_DIV: u8 = 22;
pub const OP_EQ: u8 = 23;
pub const OP_NE: u8 = 24;
pub const OP_LT: u8 = 25;
pub const OP_LTE: u8 = 26;
pub const OP_GT: u8 = 27;
pub const OP_GTE: u8 = 28;
pub const OP_AND: u8 = 29;
pub const OP_OR: u8 = 30;
pub const OP_NOT: u8 = 31;
pub const OP_MIN: u8 = 32;
pub const OP_MAX: u8 = 33;
pub const OP_SELECT: u8 = 34;
pub const OP_CAST_U64: u8 = 35;
pub const OP_CAST_I64: u8 = 36;
pub const OP_CAST_U128: u8 = 37;
pub const OP_LOOP_INDEX: u8 = 38;
pub const OP_REQUIRE: u8 = 40;
pub const OP_INVOKE: u8 = 41;
pub const OP_FOREACH: u8 = 42;
pub const OP_READ_U8: u8 = 43;
pub const OP_READ_U16: u8 = 44;
pub const OP_READ_U32: u8 = 45;
pub const OP_READ_BOOL: u8 = 46;
pub const OP_DERIVE_PDA: u8 = 47;
/// Reads a typed value from the return data of the immediately preceding unconditional invoke.
pub const OP_RETURN_DATA: u8 = 48;
/// Copies register `a` into `dst`; used to assign loop-carried registers.
pub const OP_MOVE: u8 = 49;
/// Derives a PDA from the seeds in the immediate range plus the bump held in register `b`. Unlike
/// [`OP_DERIVE_PDA`] this performs a single derivation instead of searching for the canonical bump.
pub const OP_CREATE_PDA: u8 = 50;
/// `a × b ÷ c` for three `u64`s or three `u128`s, rounded down, with the product computed exactly.
pub const OP_MUL_DIV: u8 = 51;
/// As [`OP_MUL_DIV`], rounded up.
pub const OP_MUL_DIV_CEIL: u8 = 52;
/// `a mod b` for matching `u64`, `i64` or `u128`; the result takes the dividend's sign.
pub const OP_REM: u8 = 53;
/// `a << b` for a `u64` or `u128` `a` and a `u64` `b`; fails rather than drop a set bit.
pub const OP_SHL: u8 = 54;
/// `a >> b`, rounding down; a shift of the full width or more gives zero.
pub const OP_SHR: u8 = 55;
/// `a & b` for two matching `u64` or `u128` registers.
pub const OP_BIT_AND: u8 = 56;
/// `a | b`, as [`OP_BIT_AND`].
pub const OP_BIT_OR: u8 = 57;
/// `a ^ b`, as [`OP_BIT_AND`].
pub const OP_BIT_XOR: u8 = 58;
/// `10^a` for a `u64` `a` of at most 38, as a `u128`.
pub const OP_POW10: u8 = 59;
/// A four-byte signed read, sign-extended into an `i64` register.
pub const OP_READ_I32: u8 = 60;
/// A loop that runs its body a counted number of times: `a` is the body length, `b` the `u64`
/// register holding the count, read once when the loop starts, `c` the static maximum, and the
/// immediate the carry mask, as for [`OP_FOREACH`]. It writes no register, so `dst` is
/// [`NO_INDEX`]. A count above `c` fails the run.
pub const OP_REPEAT: u8 = 61;
/// Encodes the data segments the immediate names, as CPI data is encoded, and logs the bytes with
/// `sol_log_data` as one field. Writes no register.
///
/// The first segment must be a literal tag of at least [`MIN_EMIT_TAG_LEN`] bytes that does not
/// start with [`RUN_EVENT_TAG_FAMILY`]. A log line names the program that wrote it, Ballista, but
/// not the template, so without a tag a template could log a byte-exact copy of the run event for
/// any template address, and indexers could not tell the copy from the real one.
pub const OP_EMIT: u8 = 62;
/// Encodes the data segments the immediate names and sets the bytes as the run's return data.
/// Allowed once, outside every loop, after the last invoke. Writes no register.
pub const OP_SET_RETURN_DATA: u8 = 63;
/// The number of instructions in the transaction. `a` is the Instructions sysvar account, as for
/// every opcode up to `OP_READ_INSTRUCTION_BYTES`.
pub const OP_INSTRUCTION_COUNT: u8 = 64;
/// The index of the instruction running this template.
pub const OP_INSTRUCTION_INDEX: u8 = 65;
/// The program of the instruction whose `u64` index is in register `b`.
pub const OP_INSTRUCTION_PROGRAM: u8 = 66;
/// How many accounts instruction `b` names.
pub const OP_INSTRUCTION_ACCOUNT_COUNT: u8 = 67;
/// The key of account `c` of instruction `b`, both `u64` registers.
pub const OP_INSTRUCTION_ACCOUNT: u8 = 68;
/// The flags of account `c` of instruction `b`: bit 0 signer, bit 1 writable.
pub const OP_INSTRUCTION_ACCOUNT_FLAGS: u8 = 69;
/// The data length of instruction `b`.
pub const OP_INSTRUCTION_DATA_LEN: u8 = 70;
/// A typed read from instruction `b`'s data at the `u64` offset in register `c`. The immediate is
/// the `OP_READ_*` opcode whose width and result type the read takes.
pub const OP_READ_INSTRUCTION_DATA: u8 = 71;
/// Exactly `immediate` bytes of instruction `b`'s data from the offset in register `c`, borrowed
/// from the sysvar rather than copied.
pub const OP_READ_INSTRUCTION_BYTES: u8 = 72;
/// Exactly `immediate` bytes of account `a`'s data from the `u64` offset in register `b`. The
/// account must be read-only in the transaction, so the bytes can be borrowed for the whole run.
pub const OP_READ_ACCOUNT_BYTES: u8 = 73;
/// The length of the `bytes` value in register `a`, as a `u64`.
pub const OP_BYTES_LEN: u8 = 74;

pub const DATA_LITERAL: u8 = 0;
pub const DATA_REG_U8: u8 = 1;
pub const DATA_REG_U16: u8 = 2;
pub const DATA_REG_U32: u8 = 3;
pub const DATA_REG_U64: u8 = 4;
pub const DATA_REG_I64: u8 = 5;
pub const DATA_REG_U128: u8 = 6;
pub const DATA_REG_PUBKEY: u8 = 7;
pub const DATA_REG_BOOL: u8 = 8;
pub const DATA_REG_BYTES: u8 = 9;

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct ProgramHeader {
    magic: [u8; 4],
    version: u8,
    fixed_account_count: u8,
    batch_stride: u8,
    batch_max_iterations: u8,
    input_count: u8,
    register_count: u8,
    instruction_count: u8,
    cpi_count: u8,
    cpi_account_count_le: [u8; 2],
    data_segment_count_le: [u8; 2],
    pubkey_count: u8,
    flags: u8,
    blob_len_le: [u8; 2],
    batch_min_iterations: u8,
    row_input_count: u8,
    account_group_count: u8,
    reserved: [u8; 1],
}

pub const PROGRAM_HEADER_LEN: usize = size_of::<ProgramHeader>();

impl ProgramHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fixed_account_count: u8,
        batch_stride: u8,
        batch_max_iterations: u8,
        batch_min_iterations: u8,
        input_count: u8,
        register_count: u8,
        instruction_count: u8,
        cpi_count: u8,
        cpi_account_count: u16,
        data_segment_count: u16,
        pubkey_count: u8,
        flags: u8,
        blob_len: u16,
        row_input_count: u8,
        account_group_count: u8,
    ) -> Self {
        Self {
            magic: TEMPLATE_PROGRAM_MAGIC,
            version: TEMPLATE_PROGRAM_VERSION,
            fixed_account_count,
            batch_stride,
            batch_max_iterations,
            input_count,
            register_count,
            instruction_count,
            cpi_count,
            cpi_account_count_le: cpi_account_count.to_le_bytes(),
            data_segment_count_le: data_segment_count.to_le_bytes(),
            pubkey_count,
            flags,
            blob_len_le: blob_len.to_le_bytes(),
            batch_min_iterations,
            row_input_count,
            account_group_count,
            reserved: [0; 1],
        }
    }

    pub const fn magic(&self) -> &[u8; 4] {
        &self.magic
    }
    pub const fn version(&self) -> u8 {
        self.version
    }
    pub const fn fixed_account_count(&self) -> usize {
        self.fixed_account_count as usize
    }
    pub const fn batch_stride(&self) -> usize {
        self.batch_stride as usize
    }
    pub const fn batch_max_iterations(&self) -> usize {
        self.batch_max_iterations as usize
    }
    pub const fn batch_min_iterations(&self) -> usize {
        self.batch_min_iterations as usize
    }
    pub const fn input_count(&self) -> usize {
        self.input_count as usize
    }
    /// Inputs carried once per batch iteration, declared after the fixed inputs.
    pub const fn row_input_count(&self) -> usize {
        self.row_input_count as usize
    }
    /// Fixed plus row input descriptors: the length of the inputs table.
    pub const fn total_input_count(&self) -> usize {
        self.input_count as usize + self.row_input_count as usize
    }
    /// Caller-sized account groups that follow the batch rows in the runtime accounts.
    pub const fn account_group_count(&self) -> usize {
        self.account_group_count as usize
    }
    pub const fn register_count(&self) -> usize {
        self.register_count as usize
    }
    pub const fn instruction_count(&self) -> usize {
        self.instruction_count as usize
    }
    pub const fn cpi_count(&self) -> usize {
        self.cpi_count as usize
    }
    pub fn cpi_account_count(&self) -> usize {
        u16::from_le_bytes(self.cpi_account_count_le) as usize
    }
    pub fn data_segment_count(&self) -> usize {
        u16::from_le_bytes(self.data_segment_count_le) as usize
    }
    pub const fn pubkey_count(&self) -> usize {
        self.pubkey_count as usize
    }
    pub fn blob_len(&self) -> usize {
        u16::from_le_bytes(self.blob_len_le) as usize
    }
    pub const fn flags(&self) -> u8 {
        self.flags
    }
    pub const fn reserved(&self) -> &[u8; 1] {
        &self.reserved
    }
    pub fn as_bytes(&self) -> &[u8] {
        IntoBytes::as_bytes(self)
    }
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct AccountConstraint {
    pub flags: u8,
    pub address_index: u8,
    pub owner_index: u8,
    pub reserved: u8,
    pub min_data_len_le: [u8; 4],
}

impl AccountConstraint {
    pub fn min_data_len(&self) -> usize {
        u32::from_le_bytes(self.min_data_len_le) as usize
    }
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct InputDescriptor {
    pub value_type: u8,
    pub reserved: u8,
    pub max_len_le: [u8; 2],
}

impl InputDescriptor {
    pub fn max_len(&self) -> usize {
        u16::from_le_bytes(self.max_len_le) as usize
    }
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct InstructionRecord {
    pub opcode: u8,
    pub dst: u8,
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub flags: u8,
    pub immediate_le: [u8; 8],
    pub reserved: [u8; 2],
}

impl InstructionRecord {
    pub fn immediate(&self) -> u64 {
        u64::from_le_bytes(self.immediate_le)
    }
    pub fn blob_range(&self) -> (usize, usize) {
        let value = self.immediate();
        (value as u32 as usize, (value >> 32) as u32 as usize)
    }
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct CpiDescriptor {
    pub program_account: u8,
    /// Account group forwarded after the declared accounts, or `NO_INDEX` for none.
    pub account_group: u8,
    pub account_start_le: [u8; 2],
    pub account_len: u8,
    pub segment_len: u8,
    pub segment_start_le: [u8; 2],
    pub max_data_len_le: [u8; 2],
    pub reserved1: [u8; 2],
}

impl CpiDescriptor {
    /// The account group this CPI forwards, if any.
    pub fn account_group(&self) -> Option<usize> {
        (self.account_group != NO_INDEX).then_some(self.account_group as usize)
    }
    pub fn account_start(&self) -> usize {
        u16::from_le_bytes(self.account_start_le) as usize
    }
    pub fn segment_start(&self) -> usize {
        u16::from_le_bytes(self.segment_start_le) as usize
    }
    pub fn max_data_len(&self) -> usize {
        u16::from_le_bytes(self.max_data_len_le) as usize
    }
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct CpiAccountRecord {
    pub account: u8,
    pub flags: u8,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct DataSegment {
    pub kind: u8,
    pub register: u8,
    pub offset_le: [u8; 2],
    pub len_le: [u8; 2],
    pub reserved: [u8; 2],
}

impl DataSegment {
    pub fn offset(&self) -> usize {
        u16::from_le_bytes(self.offset_le) as usize
    }
    pub fn len(&self) -> usize {
        u16::from_le_bytes(self.len_le) as usize
    }
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct PubkeyRecord {
    pub bytes: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
pub struct ProgramView<'data> {
    pub header: &'data ProgramHeader,
    pub accounts: &'data [AccountConstraint],
    pub inputs: &'data [InputDescriptor],
    pub instructions: &'data [InstructionRecord],
    pub cpis: &'data [CpiDescriptor],
    pub cpi_accounts: &'data [CpiAccountRecord],
    pub data_segments: &'data [DataSegment],
    pub pubkeys: &'data [PubkeyRecord],
    pub blob: &'data [u8],
}

impl<'data> ProgramView<'data> {
    pub fn parse(data: &'data [u8]) -> Result<Self, TemplateError> {
        if data.len() > MAX_TEMPLATE_PAYLOAD_LEN {
            return Err(TemplateError::PayloadTooLarge(data.len()));
        }
        let (header, mut remaining) =
            ProgramHeader::ref_from_prefix(data).map_err(|_| TemplateError::Truncated)?;
        if header.magic() != &TEMPLATE_PROGRAM_MAGIC {
            return Err(TemplateError::InvalidMagic);
        }
        if header.version() != TEMPLATE_PROGRAM_VERSION {
            return Err(TemplateError::UnsupportedVersion(header.version()));
        }
        if header.flags() & !PROGRAM_FLAGS_MASK != 0 || header.reserved() != &[0; 1] {
            return Err(TemplateError::InvalidReservedBytes);
        }

        let account_count = header
            .fixed_account_count()
            .checked_add(header.batch_stride())
            .ok_or(TemplateError::CountOverflow)?;
        let (accounts, suffix) = take_records::<AccountConstraint>(remaining, account_count)?;
        remaining = suffix;
        let (inputs, suffix) =
            take_records::<InputDescriptor>(remaining, header.total_input_count())?;
        remaining = suffix;
        let (instructions, suffix) =
            take_records::<InstructionRecord>(remaining, header.instruction_count())?;
        remaining = suffix;
        let (cpis, suffix) = take_records::<CpiDescriptor>(remaining, header.cpi_count())?;
        remaining = suffix;
        let (cpi_accounts, suffix) =
            take_records::<CpiAccountRecord>(remaining, header.cpi_account_count())?;
        remaining = suffix;
        let (data_segments, suffix) =
            take_records::<DataSegment>(remaining, header.data_segment_count())?;
        remaining = suffix;
        let (pubkeys, suffix) = take_records::<PubkeyRecord>(remaining, header.pubkey_count())?;
        remaining = suffix;

        if remaining.len() != header.blob_len() {
            return Err(TemplateError::SectionLengthMismatch);
        }

        Ok(Self {
            header,
            accounts,
            inputs,
            instructions,
            cpis,
            cpi_accounts,
            data_segments,
            pubkeys,
            blob: remaining,
        })
    }

    /// Splits a payload that [`ProgramView::parse`] and [`ProgramView::verify`] already accepted,
    /// as they did for every finalized template.
    ///
    /// It checks the magic and version, so a payload of another layout is never read as this
    /// one, and that the sections the header declares fill the payload exactly, which keeps every
    /// slice in bounds. It skips the checks on reserved bytes, flags and the size cap, which
    /// verification settled when the template was finalized. Returns `None` where `parse` would
    /// fail on the structure.
    #[inline(always)]
    pub fn parse_finalized(data: &'data [u8]) -> Option<Self> {
        let (header, mut remaining) = ProgramHeader::ref_from_prefix(data).ok()?;
        if header.magic != TEMPLATE_PROGRAM_MAGIC || header.version != TEMPLATE_PROGRAM_VERSION {
            return None;
        }
        let accounts = take_prefix::<AccountConstraint>(
            &mut remaining,
            header.fixed_account_count() + header.batch_stride(),
        )?;
        let inputs = take_prefix::<InputDescriptor>(&mut remaining, header.total_input_count())?;
        let instructions =
            take_prefix::<InstructionRecord>(&mut remaining, header.instruction_count())?;
        let cpis = take_prefix::<CpiDescriptor>(&mut remaining, header.cpi_count())?;
        let cpi_accounts =
            take_prefix::<CpiAccountRecord>(&mut remaining, header.cpi_account_count())?;
        let data_segments =
            take_prefix::<DataSegment>(&mut remaining, header.data_segment_count())?;
        let pubkeys = take_prefix::<PubkeyRecord>(&mut remaining, header.pubkey_count())?;
        if remaining.len() != header.blob_len() {
            return None;
        }
        Some(Self {
            header,
            accounts,
            inputs,
            instructions,
            cpis,
            cpi_accounts,
            data_segments,
            pubkeys,
            blob: remaining,
        })
    }

    /// The constraint an account reference names: a fixed account, or inside a loop over the batch
    /// rows a row account.
    pub fn account_constraint(&self, reference: u8, in_row_loop: bool) -> Option<&AccountConstraint> {
        if reference & ITERATION_ACCOUNT_BIT == 0 {
            return self
                .accounts
                .get(reference as usize)
                .filter(|_| (reference as usize) < self.header.fixed_account_count());
        }
        if !in_row_loop {
            return None;
        }
        let offset = (reference & !ITERATION_ACCOUNT_BIT) as usize;
        if offset >= self.header.batch_stride() {
            return None;
        }
        self.accounts
            .get(self.header.fixed_account_count() + offset)
    }

    /// The descriptor a `LOAD_INPUT` operand names: a fixed input, or inside a loop over the batch
    /// rows a row input.
    pub fn input_descriptor(&self, reference: u8, in_row_loop: bool) -> Option<&InputDescriptor> {
        if reference & ITERATION_INPUT_BIT == 0 {
            return self
                .inputs
                .get(reference as usize)
                .filter(|_| (reference as usize) < self.header.input_count());
        }
        if !in_row_loop {
            return None;
        }
        let offset = (reference & !ITERATION_INPUT_BIT) as usize;
        if offset >= self.header.row_input_count() {
            return None;
        }
        self.inputs.get(self.header.input_count() + offset)
    }
}

/// Splits `count` records off the front of `data`. Every count a header holds fits in 16 bits and
/// every record in 32 bytes, so the byte length cannot overflow.
#[inline(always)]
fn take_prefix<'data, T>(data: &mut &'data [u8], count: usize) -> Option<&'data [T]>
where
    T: FromBytes + KnownLayout + Immutable,
{
    let (records, suffix) = <[T]>::ref_from_prefix_with_elems(*data, count).ok()?;
    *data = suffix;
    Some(records)
}

fn take_records<T>(data: &[u8], count: usize) -> Result<(&[T], &[u8]), TemplateError>
where
    T: FromBytes + KnownLayout + Immutable,
{
    let byte_len = size_of::<T>()
        .checked_mul(count)
        .ok_or(TemplateError::CountOverflow)?;
    let (section, suffix) = data
        .split_at_checked(byte_len)
        .ok_or(TemplateError::Truncated)?;
    let records =
        <[T]>::ref_from_bytes_with_elems(section, count).map_err(|_| TemplateError::Truncated)?;
    Ok((records, suffix))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateError {
    Truncated,
    PayloadTooLarge(usize),
    InvalidMagic,
    UnsupportedVersion(u8),
    InvalidReservedBytes,
    SectionLengthMismatch,
    CountOverflow,
    TooManyAccounts,
    TooManyInputs,
    TooManyRegisters,
    TooManyInstructions,
    InvalidBatch,
    InvalidAccountConstraint(usize),
    InvalidInput(usize),
    InvalidInstruction(usize),
    InvalidCpi(usize),
    InvalidDataSegment(usize),
    InvalidRegister(u8),
    RegisterNotInitialized(u8),
    TypeMismatch,
    InvalidBlobRange,
    ExcessiveCpiExpansion,
    /// An instruction record carries flag bits its opcode does not accept.
    InvalidFlags(usize),
    /// A loop-carried register is not initialized before the loop or changes type inside it.
    InvalidCarry(u8),
    /// A fixed-offset read extends past the account's declared minimum data length.
    ReadOutOfBounds(usize),
    /// A CPI descriptor lists more than `MAX_CPI_ACCOUNTS` accounts.
    TooManyCpiAccounts(usize),
    /// A return-data read does not directly follow an unconditional invoke, or its range is invalid.
    InvalidReturnData(usize),
    /// Minimum iterations exceed the maximum, or are set without a batch.
    InvalidMinIterations,
    /// The header declares more than `MAX_ACCOUNT_GROUPS` account groups.
    TooManyAccountGroups,
    /// A `REPEAT` with an empty body, a zero maximum or a destination register, or inside another
    /// loop; a loop past `MAX_LOOPS`; or a row account or row input named inside a `REPEAT` body.
    InvalidLoop(usize),
    /// An `EMIT` or `SET_RETURN_DATA` can encode more than `MAX_RETURN_DATA_LEN` bytes, or a
    /// `SET_RETURN_DATA` repeats, sits in a loop, or precedes an invoke.
    InvalidOutput(usize),
    /// An introspection opcode's account is not a fixed account pinned to the Instructions sysvar.
    InvalidIntrospection(usize),
}

impl TemplateError {
    /// Stable on-chain custom error code and 16-bit context for this verifier failure.
    ///
    /// Codes start at [`VERIFIER_ERROR_BASE`] and follow the variant order below. The context
    /// carries the offending index, register, or version where the variant has one.
    pub fn code(&self) -> (u32, u16) {
        let clamp = |value: usize| u16::try_from(value).unwrap_or(u16::MAX);
        let (index, context) = match *self {
            TemplateError::Truncated => (0, 0),
            TemplateError::PayloadTooLarge(len) => (1, clamp(len)),
            TemplateError::InvalidMagic => (2, 0),
            TemplateError::UnsupportedVersion(version) => (3, version as u16),
            TemplateError::InvalidReservedBytes => (4, 0),
            TemplateError::SectionLengthMismatch => (5, 0),
            TemplateError::CountOverflow => (6, 0),
            TemplateError::TooManyAccounts => (7, 0),
            TemplateError::TooManyInputs => (8, 0),
            TemplateError::TooManyRegisters => (9, 0),
            TemplateError::TooManyInstructions => (10, 0),
            TemplateError::InvalidBatch => (11, 0),
            TemplateError::InvalidAccountConstraint(index) => (12, clamp(index)),
            TemplateError::InvalidInput(index) => (13, clamp(index)),
            TemplateError::InvalidInstruction(index) => (14, clamp(index)),
            TemplateError::InvalidCpi(index) => (15, clamp(index)),
            TemplateError::InvalidDataSegment(index) => (16, clamp(index)),
            TemplateError::InvalidRegister(register) => (17, register as u16),
            TemplateError::RegisterNotInitialized(register) => (18, register as u16),
            TemplateError::TypeMismatch => (19, 0),
            TemplateError::InvalidBlobRange => (20, 0),
            TemplateError::ExcessiveCpiExpansion => (21, 0),
            TemplateError::InvalidFlags(index) => (22, clamp(index)),
            TemplateError::InvalidCarry(register) => (23, register as u16),
            TemplateError::ReadOutOfBounds(index) => (24, clamp(index)),
            TemplateError::TooManyCpiAccounts(index) => (25, clamp(index)),
            TemplateError::InvalidReturnData(index) => (26, clamp(index)),
            TemplateError::InvalidMinIterations => (27, 0),
            TemplateError::TooManyAccountGroups => (28, 0),
            TemplateError::InvalidLoop(index) => (29, clamp(index)),
            TemplateError::InvalidOutput(index) => (30, clamp(index)),
            TemplateError::InvalidIntrospection(index) => (31, clamp(index)),
        };
        (VERIFIER_ERROR_BASE + index, context)
    }
}

/// Verifier error names in code order, shared with the SDK through
/// `fixtures/verifier-error-names.txt`.
pub const VERIFIER_ERROR_NAMES: [&str; 32] = [
    "Truncated",
    "PayloadTooLarge",
    "InvalidMagic",
    "UnsupportedVersion",
    "InvalidReservedBytes",
    "SectionLengthMismatch",
    "CountOverflow",
    "TooManyAccounts",
    "TooManyInputs",
    "TooManyRegisters",
    "TooManyInstructions",
    "InvalidBatch",
    "InvalidAccountConstraint",
    "InvalidInput",
    "InvalidInstruction",
    "InvalidCpi",
    "InvalidDataSegment",
    "InvalidRegister",
    "RegisterNotInitialized",
    "TypeMismatch",
    "InvalidBlobRange",
    "ExcessiveCpiExpansion",
    "InvalidFlags",
    "InvalidCarry",
    "ReadOutOfBounds",
    "TooManyCpiAccounts",
    "InvalidReturnData",
    "InvalidMinIterations",
    "TooManyAccountGroups",
    "InvalidLoop",
    "InvalidOutput",
    "InvalidIntrospection",
];

/// Packs an error kind and a 16-bit context into one custom program error code.
///
/// The low 16 bits hold the kind (runtime kinds start at 6000, verifier kinds at
/// [`VERIFIER_ERROR_BASE`]); the high 16 bits hold the program counter, account index, or other
/// context that identifies where the failure happened.
pub const fn encode_error(kind: u32, context: u16) -> u32 {
    kind | ((context as u32) << 16)
}

/// Splits a code produced by [`encode_error`] back into its kind and context.
pub const fn decode_error(code: u32) -> (u32, u16) {
    (code & 0xffff, (code >> 16) as u16)
}

/// Where a decoded Ballista error came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorSource {
    /// Raised while running a template; the context is usually a program counter.
    Runtime,
    /// Raised while verifying a template at create or finalize.
    Verifier,
}

/// A Ballista custom error code split into its parts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedError {
    pub code: u32,
    pub kind: u32,
    pub name: &'static str,
    pub context: u16,
    pub source: ErrorSource,
}

/// Decodes a custom program error code. Returns `None` for codes outside Ballista's ranges, which
/// belong to an invoked program and pass through `Run` unchanged.
pub fn decode_ballista_error(code: u32) -> Option<DecodedError> {
    let (kind, context) = decode_error(code);
    let (names, base, source) = if kind >= VERIFIER_ERROR_BASE {
        (&VERIFIER_ERROR_NAMES[..], VERIFIER_ERROR_BASE, ErrorSource::Verifier)
    } else {
        (&RUNTIME_ERROR_NAMES[..], RUNTIME_ERROR_BASE, ErrorSource::Runtime)
    };
    let name = names.get(kind.checked_sub(base)? as usize)?;
    Some(DecodedError {
        code,
        kind,
        name,
        context,
        source,
    })
}

impl fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TemplateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips_min_iterations_and_flags() {
        let header = ProgramHeader::new(1, 1, 4, 2, 0, 0, 1, 0, 0, 0, 0, PROGRAM_FLAG_EMIT_EVENT, 0, 0, 0);
        assert_eq!(header.version(), 1);
        assert_eq!(header.batch_min_iterations(), 2);
        assert_eq!(header.flags(), PROGRAM_FLAG_EMIT_EVENT);
        assert_eq!(PROGRAM_HEADER_LEN, 24);
    }

    #[test]
    fn error_codes_are_unique_and_round_trip_context() {
        let variants = [
            TemplateError::Truncated,
            TemplateError::PayloadTooLarge(70_000),
            TemplateError::InvalidMagic,
            TemplateError::UnsupportedVersion(2),
            TemplateError::InvalidReservedBytes,
            TemplateError::SectionLengthMismatch,
            TemplateError::CountOverflow,
            TemplateError::TooManyAccounts,
            TemplateError::TooManyInputs,
            TemplateError::TooManyRegisters,
            TemplateError::TooManyInstructions,
            TemplateError::InvalidBatch,
            TemplateError::InvalidAccountConstraint(3),
            TemplateError::InvalidInput(4),
            TemplateError::InvalidInstruction(5),
            TemplateError::InvalidCpi(6),
            TemplateError::InvalidDataSegment(7),
            TemplateError::InvalidRegister(8),
            TemplateError::RegisterNotInitialized(9),
            TemplateError::TypeMismatch,
            TemplateError::InvalidBlobRange,
            TemplateError::ExcessiveCpiExpansion,
            TemplateError::InvalidFlags(10),
            TemplateError::InvalidCarry(11),
            TemplateError::ReadOutOfBounds(12),
            TemplateError::TooManyCpiAccounts(13),
            TemplateError::InvalidReturnData(14),
            TemplateError::InvalidMinIterations,
            TemplateError::TooManyAccountGroups,
            TemplateError::InvalidLoop(15),
            TemplateError::InvalidOutput(15),
            TemplateError::InvalidIntrospection(15),
        ];
        let mut codes: Vec<u32> = variants.iter().map(|error| error.code().0).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), variants.len());
        assert_eq!(codes[0], VERIFIER_ERROR_BASE);
        assert_eq!(*codes.last().unwrap(), VERIFIER_ERROR_BASE + 31);
        for variant in &variants {
            let (code, _) = variant.code();
            let name = format!("{variant:?}");
            let bare = name.split('(').next().unwrap();
            assert_eq!(
                VERIFIER_ERROR_NAMES[(code - VERIFIER_ERROR_BASE) as usize],
                bare,
                "names must follow code order"
            );
        }

        let (kind, context) = TemplateError::PayloadTooLarge(70_000).code();
        assert_eq!(context, u16::MAX, "oversized contexts clamp");
        assert_eq!(decode_error(encode_error(kind, context)), (kind, context));
        assert_eq!(TemplateError::InvalidCpi(6).code(), (VERIFIER_ERROR_BASE + 15, 6));
        assert_eq!(TemplateError::InvalidLoop(4).code(), (VERIFIER_ERROR_BASE + 29, 4));
        assert_eq!(TemplateError::InvalidOutput(9).code(), (VERIFIER_ERROR_BASE + 30, 9));
        let decoded = decode_ballista_error(encode_error(VERIFIER_ERROR_BASE + 30, 9));
        assert_eq!(decoded.map(|error| error.name), Some("InvalidOutput"));
        assert_eq!(decode_ballista_error(VERIFIER_ERROR_BASE + 32), None);
    }
}
