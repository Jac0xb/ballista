//! An owned model of a template payload, decoded by byte offset from the tables in
//! `docs/reference/wire-format.md`.
//!
//! It deliberately does not use the zerocopy records in `ballista_common::template::wire`: the
//! checker and the mutators work on this model, so a mistake in `wire.rs` cannot hide itself by
//! being shared. Every multi-byte field is little-endian.

/// Payload magic and the one bytecode version the program accepts.
pub const MAGIC: [u8; 4] = *b"BVM1";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 24;
pub const ACCOUNT_LEN: usize = 8;
pub const INPUT_LEN: usize = 4;
pub const INSTRUCTION_LEN: usize = 16;
pub const CPI_LEN: usize = 12;
pub const CPI_ACCOUNT_LEN: usize = 2;
pub const SEGMENT_LEN: usize = 8;
pub const PUBKEY_LEN: usize = 32;

/// The 24-byte program header, field for field.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Header {
    pub magic: [u8; 4],
    pub version: u8,
    pub fixed_accounts: u8,
    pub batch_stride: u8,
    pub max_rows: u8,
    pub fixed_inputs: u8,
    pub registers: u8,
    pub instructions: u8,
    pub cpis: u8,
    pub cpi_accounts: u16,
    pub segments: u16,
    pub pubkeys: u8,
    pub flags: u8,
    pub blob_len: u16,
    pub min_rows: u8,
    pub row_inputs: u8,
    pub account_groups: u8,
    pub reserved: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Account {
    pub flags: u8,
    pub address: u8,
    pub owner: u8,
    pub reserved: u8,
    pub min_len: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Input {
    pub value_type: u8,
    pub reserved: u8,
    pub max_len: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Instr {
    pub op: u8,
    pub dst: u8,
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub flags: u8,
    pub imm: u64,
    pub reserved: [u8; 2],
}

impl Instr {
    pub const fn new(op: u8, dst: u8, a: u8, b: u8, c: u8, imm: u64) -> Self {
        Self { op, dst, a, b, c, flags: 0, imm, reserved: [0; 2] }
    }

    /// A packed range: start in the low 32 bits, length in the high 32.
    pub fn range(&self) -> (usize, usize) {
        ((self.imm & 0xffff_ffff) as usize, (self.imm >> 32) as usize)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Cpi {
    pub program: u8,
    pub group: u8,
    pub account_start: u16,
    pub account_len: u8,
    pub segment_len: u8,
    pub segment_start: u16,
    pub max_data_len: u16,
    pub reserved: [u8; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpiAccount {
    pub account: u8,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Segment {
    pub kind: u8,
    pub register: u8,
    pub offset: u16,
    pub len: u16,
    pub reserved: [u8; 2],
}

/// A whole payload. `accounts` holds the fixed accounts then one batch row; `inputs` the fixed
/// inputs then the row inputs, as on the wire.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Program {
    pub header: Header,
    pub accounts: Vec<Account>,
    pub inputs: Vec<Input>,
    pub instrs: Vec<Instr>,
    pub cpis: Vec<Cpi>,
    pub cpi_accounts: Vec<CpiAccount>,
    pub segments: Vec<Segment>,
    pub pubkeys: Vec<[u8; 32]>,
    pub blob: Vec<u8>,
}

/// Why a byte string is not a payload of the documented layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    /// Shorter than the header, or than the sections its counts declare.
    Truncated,
    /// Bytes left over, or missing, after the blob the header declares.
    LengthMismatch,
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    let mut value = [0u8; 8];
    value.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(value)
}

impl Header {
    fn decode(bytes: &[u8]) -> Self {
        Self {
            magic: [bytes[0], bytes[1], bytes[2], bytes[3]],
            version: bytes[4],
            fixed_accounts: bytes[5],
            batch_stride: bytes[6],
            max_rows: bytes[7],
            fixed_inputs: bytes[8],
            registers: bytes[9],
            instructions: bytes[10],
            cpis: bytes[11],
            cpi_accounts: u16_at(bytes, 12),
            segments: u16_at(bytes, 14),
            pubkeys: bytes[16],
            flags: bytes[17],
            blob_len: u16_at(bytes, 18),
            min_rows: bytes[20],
            row_inputs: bytes[21],
            account_groups: bytes[22],
            reserved: bytes[23],
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.magic);
        out.push(self.version);
        out.push(self.fixed_accounts);
        out.push(self.batch_stride);
        out.push(self.max_rows);
        out.push(self.fixed_inputs);
        out.push(self.registers);
        out.push(self.instructions);
        out.push(self.cpis);
        out.extend_from_slice(&self.cpi_accounts.to_le_bytes());
        out.extend_from_slice(&self.segments.to_le_bytes());
        out.push(self.pubkeys);
        out.push(self.flags);
        out.extend_from_slice(&self.blob_len.to_le_bytes());
        out.push(self.min_rows);
        out.push(self.row_inputs);
        out.push(self.account_groups);
        out.push(self.reserved);
    }
}

impl Program {
    /// Splits `bytes` into sections by the header's counts. Checks only the structure: the header
    /// fits, every section fits, and the blob ends the payload exactly. Magic, version, flags and
    /// the size cap are left to the checker, so the mutators can decode anything shaped right.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < HEADER_LEN {
            return Err(DecodeError::Truncated);
        }
        let header = Header::decode(bytes);
        let mut at = HEADER_LEN;
        let mut take = |count: usize, size: usize| -> Result<usize, DecodeError> {
            let start = at;
            let end = start + count * size;
            if end > bytes.len() {
                return Err(DecodeError::Truncated);
            }
            at = end;
            Ok(start)
        };
        let account_count = header.fixed_accounts as usize + header.batch_stride as usize;
        let accounts_at = take(account_count, ACCOUNT_LEN)?;
        let input_count = header.fixed_inputs as usize + header.row_inputs as usize;
        let inputs_at = take(input_count, INPUT_LEN)?;
        let instrs_at = take(header.instructions as usize, INSTRUCTION_LEN)?;
        let cpis_at = take(header.cpis as usize, CPI_LEN)?;
        let cpi_accounts_at = take(header.cpi_accounts as usize, CPI_ACCOUNT_LEN)?;
        let segments_at = take(header.segments as usize, SEGMENT_LEN)?;
        let pubkeys_at = take(header.pubkeys as usize, PUBKEY_LEN)?;
        let blob_at = at;
        if bytes.len() - blob_at != header.blob_len as usize {
            return Err(DecodeError::LengthMismatch);
        }

        let accounts = (0..account_count)
            .map(|index| {
                let at = accounts_at + index * ACCOUNT_LEN;
                Account {
                    flags: bytes[at],
                    address: bytes[at + 1],
                    owner: bytes[at + 2],
                    reserved: bytes[at + 3],
                    min_len: u32_at(bytes, at + 4),
                }
            })
            .collect();
        let inputs = (0..input_count)
            .map(|index| {
                let at = inputs_at + index * INPUT_LEN;
                Input { value_type: bytes[at], reserved: bytes[at + 1], max_len: u16_at(bytes, at + 2) }
            })
            .collect();
        let instrs = (0..header.instructions as usize)
            .map(|index| {
                let at = instrs_at + index * INSTRUCTION_LEN;
                Instr {
                    op: bytes[at],
                    dst: bytes[at + 1],
                    a: bytes[at + 2],
                    b: bytes[at + 3],
                    c: bytes[at + 4],
                    flags: bytes[at + 5],
                    imm: u64_at(bytes, at + 6),
                    reserved: [bytes[at + 14], bytes[at + 15]],
                }
            })
            .collect();
        let cpis = (0..header.cpis as usize)
            .map(|index| {
                let at = cpis_at + index * CPI_LEN;
                Cpi {
                    program: bytes[at],
                    group: bytes[at + 1],
                    account_start: u16_at(bytes, at + 2),
                    account_len: bytes[at + 4],
                    segment_len: bytes[at + 5],
                    segment_start: u16_at(bytes, at + 6),
                    max_data_len: u16_at(bytes, at + 8),
                    reserved: [bytes[at + 10], bytes[at + 11]],
                }
            })
            .collect();
        let cpi_accounts = (0..header.cpi_accounts as usize)
            .map(|index| {
                let at = cpi_accounts_at + index * CPI_ACCOUNT_LEN;
                CpiAccount { account: bytes[at], flags: bytes[at + 1] }
            })
            .collect();
        let segments = (0..header.segments as usize)
            .map(|index| {
                let at = segments_at + index * SEGMENT_LEN;
                Segment {
                    kind: bytes[at],
                    register: bytes[at + 1],
                    offset: u16_at(bytes, at + 2),
                    len: u16_at(bytes, at + 4),
                    reserved: [bytes[at + 6], bytes[at + 7]],
                }
            })
            .collect();
        let pubkeys = (0..header.pubkeys as usize)
            .map(|index| {
                let at = pubkeys_at + index * PUBKEY_LEN;
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes[at..at + 32]);
                key
            })
            .collect();
        Ok(Self {
            header,
            accounts,
            inputs,
            instrs,
            cpis,
            cpi_accounts,
            segments,
            pubkeys,
            blob: bytes[blob_at..].to_vec(),
        })
    }

    /// Writes the header as it stands and every section in order. Call [`Program::sync_counts`]
    /// first for a payload whose counts match its sections.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.blob.len() + 16 * self.instrs.len());
        self.header.encode(&mut out);
        for account in &self.accounts {
            out.push(account.flags);
            out.push(account.address);
            out.push(account.owner);
            out.push(account.reserved);
            out.extend_from_slice(&account.min_len.to_le_bytes());
        }
        for input in &self.inputs {
            out.push(input.value_type);
            out.push(input.reserved);
            out.extend_from_slice(&input.max_len.to_le_bytes());
        }
        for instr in &self.instrs {
            out.extend_from_slice(&[instr.op, instr.dst, instr.a, instr.b, instr.c, instr.flags]);
            out.extend_from_slice(&instr.imm.to_le_bytes());
            out.extend_from_slice(&instr.reserved);
        }
        for cpi in &self.cpis {
            out.push(cpi.program);
            out.push(cpi.group);
            out.extend_from_slice(&cpi.account_start.to_le_bytes());
            out.push(cpi.account_len);
            out.push(cpi.segment_len);
            out.extend_from_slice(&cpi.segment_start.to_le_bytes());
            out.extend_from_slice(&cpi.max_data_len.to_le_bytes());
            out.extend_from_slice(&cpi.reserved);
        }
        for record in &self.cpi_accounts {
            out.push(record.account);
            out.push(record.flags);
        }
        for segment in &self.segments {
            out.push(segment.kind);
            out.push(segment.register);
            out.extend_from_slice(&segment.offset.to_le_bytes());
            out.extend_from_slice(&segment.len.to_le_bytes());
            out.extend_from_slice(&segment.reserved);
        }
        for key in &self.pubkeys {
            out.extend_from_slice(key);
        }
        out.extend_from_slice(&self.blob);
        out
    }

    /// Makes every header count match its section. The batch stride and the row input count are
    /// kept where they fit, and the fixed counts take the rest. Counts past a field's width are
    /// truncated, as a hand-written payload would be.
    pub fn sync_counts(&mut self) {
        let header = &mut self.header;
        header.batch_stride = header.batch_stride.min(self.accounts.len().min(255) as u8);
        header.fixed_accounts = (self.accounts.len() - header.batch_stride as usize) as u8;
        header.row_inputs = header.row_inputs.min(self.inputs.len().min(255) as u8);
        header.fixed_inputs = (self.inputs.len() - header.row_inputs as usize) as u8;
        header.instructions = self.instrs.len() as u8;
        header.cpis = self.cpis.len() as u8;
        header.cpi_accounts = self.cpi_accounts.len() as u16;
        header.segments = self.segments.len() as u16;
        header.pubkeys = self.pubkeys.len() as u8;
        header.blob_len = self.blob.len() as u16;
    }

    pub fn fixed_count(&self) -> usize {
        self.header.fixed_accounts as usize
    }

    pub fn stride(&self) -> usize {
        self.header.batch_stride as usize
    }
}
