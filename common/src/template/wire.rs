use core::{fmt, mem::size_of};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const TEMPLATE_PROGRAM_MAGIC: [u8; 4] = *b"BVM2";
pub const TEMPLATE_PROGRAM_VERSION: u8 = 2;

pub const MAX_TEMPLATE_PAYLOAD_LEN: usize = 10_240;
pub const MAX_RUNTIME_ACCOUNTS: usize = 60;
pub const MAX_INPUTS: usize = 32;
pub const MAX_INPUT_BYTES: usize = 1_024;
pub const MAX_REGISTERS: usize = 64;
pub const MAX_VM_INSTRUCTIONS: usize = 128;
pub const MAX_EXPANDED_CPIS: usize = 64;
pub const MAX_CPI_DATA_LEN: usize = 4_096;
pub const MAX_BATCH_STRIDE: usize = 8;

pub const NO_INDEX: u8 = u8::MAX;
pub const ITERATION_ACCOUNT_BIT: u8 = 0x80;

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
    reserved: [u8; 4],
}

pub const PROGRAM_HEADER_LEN: usize = size_of::<ProgramHeader>();

impl ProgramHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fixed_account_count: u8,
        batch_stride: u8,
        batch_max_iterations: u8,
        input_count: u8,
        register_count: u8,
        instruction_count: u8,
        cpi_count: u8,
        cpi_account_count: u16,
        data_segment_count: u16,
        pubkey_count: u8,
        blob_len: u16,
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
            flags: 0,
            blob_len_le: blob_len.to_le_bytes(),
            reserved: [0; 4],
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
    pub const fn input_count(&self) -> usize {
        self.input_count as usize
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
    pub const fn reserved(&self) -> &[u8; 4] {
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
    pub reserved0: u8,
    pub account_start_le: [u8; 2],
    pub account_len: u8,
    pub segment_len: u8,
    pub segment_start_le: [u8; 2],
    pub max_data_len_le: [u8; 2],
    pub reserved1: [u8; 2],
}

impl CpiDescriptor {
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
        if header.flags() != 0 || header.reserved() != &[0; 4] {
            return Err(TemplateError::InvalidReservedBytes);
        }

        let account_count = header
            .fixed_account_count()
            .checked_add(header.batch_stride())
            .ok_or(TemplateError::CountOverflow)?;
        let (accounts, suffix) = take_records::<AccountConstraint>(remaining, account_count)?;
        remaining = suffix;
        let (inputs, suffix) = take_records::<InputDescriptor>(remaining, header.input_count())?;
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

    pub fn account_constraint(&self, reference: u8, in_loop: bool) -> Option<&AccountConstraint> {
        if reference & ITERATION_ACCOUNT_BIT == 0 {
            return self
                .accounts
                .get(reference as usize)
                .filter(|_| (reference as usize) < self.header.fixed_account_count());
        }
        if !in_loop {
            return None;
        }
        let offset = (reference & !ITERATION_ACCOUNT_BIT) as usize;
        if offset >= self.header.batch_stride() {
            return None;
        }
        self.accounts
            .get(self.header.fixed_account_count() + offset)
    }
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
}

impl fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TemplateError {}
