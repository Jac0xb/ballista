//! Typed assembler for Ballista programs.
//!
//! The builder keeps the record tables the wire format expects, hands out account, input, and
//! register indices as it goes, and writes the header from the final counts. Tests use it to
//! construct valid and deliberately invalid programs without hand-computing byte offsets, and Rust
//! authors can use it to produce templates without the TypeScript compiler.

use zerocopy::IntoBytes;

use super::*;

/// One piece of CPI data or one PDA seed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Segment {
    /// Literal bytes previously stored with [`ProgramBuilder::blob`], as `(offset, len)`.
    Literal((u16, u16)),
    /// A register encoded with one of the `DATA_REG_*` kinds.
    Register(u8, u8),
}

/// Incrementally assembles a program payload.
#[derive(Clone, Debug, Default)]
pub struct ProgramBuilder {
    fixed: Vec<AccountConstraint>,
    row: Vec<AccountConstraint>,
    batch_max: u8,
    batch_min: u8,
    flags: u8,
    inputs: Vec<InputDescriptor>,
    instructions: Vec<InstructionRecord>,
    cpis: Vec<CpiDescriptor>,
    cpi_accounts: Vec<CpiAccountRecord>,
    segments: Vec<DataSegment>,
    pubkeys: Vec<PubkeyRecord>,
    blob: Vec<u8>,
    next_register: u8,
}

impl ProgramBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a fixed runtime account and returns its reference.
    pub fn account(
        &mut self,
        flags: u8,
        address: Option<[u8; 32]>,
        owner: Option<[u8; 32]>,
        min_data_len: u32,
    ) -> u8 {
        let constraint = self.constraint(flags, address, owner, min_data_len);
        self.fixed.push(constraint);
        (self.fixed.len() - 1) as u8
    }

    /// Declares one account of the batch row and returns its iteration reference.
    pub fn row_account(
        &mut self,
        flags: u8,
        address: Option<[u8; 32]>,
        owner: Option<[u8; 32]>,
        min_data_len: u32,
    ) -> u8 {
        let constraint = self.constraint(flags, address, owner, min_data_len);
        self.row.push(constraint);
        ITERATION_ACCOUNT_BIT | (self.row.len() - 1) as u8
    }

    /// Sets the batch iteration bounds. The stride is the number of row accounts declared.
    pub fn batch(&mut self, max_iterations: u8, min_iterations: u8) -> &mut Self {
        self.batch_max = max_iterations;
        self.batch_min = min_iterations;
        self
    }

    /// Sets the program header flags.
    pub fn flags(&mut self, flags: u8) -> &mut Self {
        self.flags = flags;
        self
    }

    /// Declares a run input and returns its index.
    pub fn input(&mut self, value_type: u8, max_len: u16) -> u8 {
        self.inputs.push(InputDescriptor {
            value_type,
            reserved: 0,
            max_len_le: max_len.to_le_bytes(),
        });
        (self.inputs.len() - 1) as u8
    }

    /// Interns a constant pubkey and returns its table index.
    pub fn pubkey(&mut self, bytes: [u8; 32]) -> u8 {
        if let Some(index) = self
            .pubkeys
            .iter()
            .position(|record| record.bytes == bytes)
        {
            return index as u8;
        }
        self.pubkeys.push(PubkeyRecord { bytes });
        (self.pubkeys.len() - 1) as u8
    }

    /// Appends literal bytes to the blob and returns their `(offset, len)`.
    pub fn blob(&mut self, bytes: &[u8]) -> (u16, u16) {
        let offset = self.blob.len() as u16;
        self.blob.extend_from_slice(bytes);
        (offset, bytes.len() as u16)
    }

    /// Allocates the next register index without emitting an instruction.
    pub fn register(&mut self) -> u8 {
        let register = self.next_register;
        self.next_register += 1;
        register
    }

    /// Appends a raw record and returns its instruction index.
    pub fn emit(&mut self, record: InstructionRecord) -> usize {
        self.instructions.push(record);
        self.instructions.len() - 1
    }

    /// Emits an instruction that writes a fresh register and returns that register.
    pub fn op(&mut self, opcode: u8, a: u8, b: u8, c: u8, immediate: u64) -> u8 {
        let dst = self.register();
        self.emit(record(opcode, dst, a, b, c, 0, immediate));
        dst
    }

    pub fn load_input(&mut self, input: u8) -> u8 {
        self.op(OP_LOAD_INPUT, input, NO_INDEX, NO_INDEX, 0)
    }

    pub fn const_bool(&mut self, value: bool) -> u8 {
        self.op(OP_CONST_BOOL, u8::from(value), NO_INDEX, NO_INDEX, 0)
    }

    pub fn const_u64(&mut self, value: u64) -> u8 {
        self.op(OP_CONST_U64, NO_INDEX, NO_INDEX, NO_INDEX, value)
    }

    pub fn const_i64(&mut self, value: i64) -> u8 {
        self.op(OP_CONST_I64, NO_INDEX, NO_INDEX, NO_INDEX, value as u64)
    }

    pub fn const_u128(&mut self, value: u128) -> u8 {
        let (offset, len) = self.blob(&value.to_le_bytes());
        self.op(OP_CONST_U128, NO_INDEX, NO_INDEX, NO_INDEX, range_immediate(offset, len))
    }

    pub fn const_pubkey(&mut self, value: [u8; 32]) -> u8 {
        let index = self.pubkey(value);
        self.op(OP_CONST_PUBKEY, index, NO_INDEX, NO_INDEX, 0)
    }

    pub fn const_bytes(&mut self, value: &[u8]) -> u8 {
        let (offset, len) = self.blob(value);
        self.op(OP_CONST_BYTES, NO_INDEX, NO_INDEX, NO_INDEX, range_immediate(offset, len))
    }

    pub fn account_key(&mut self, account: u8) -> u8 {
        self.op(OP_ACCOUNT_KEY, account, NO_INDEX, NO_INDEX, 0)
    }

    pub fn account_owner(&mut self, account: u8) -> u8 {
        self.op(OP_ACCOUNT_OWNER, account, NO_INDEX, NO_INDEX, 0)
    }

    pub fn account_lamports(&mut self, account: u8) -> u8 {
        self.op(OP_ACCOUNT_LAMPORTS, account, NO_INDEX, NO_INDEX, 0)
    }

    pub fn account_data_len(&mut self, account: u8) -> u8 {
        self.op(OP_ACCOUNT_DATA_LEN, account, NO_INDEX, NO_INDEX, 0)
    }

    pub fn account_is_empty(&mut self, account: u8) -> u8 {
        self.op(OP_ACCOUNT_IS_EMPTY, account, NO_INDEX, NO_INDEX, 0)
    }

    /// Emits a fixed-offset account data read with one of the `OP_READ_*` opcodes.
    pub fn read(&mut self, opcode: u8, account: u8, offset: u64) -> u8 {
        self.op(opcode, account, NO_INDEX, NO_INDEX, offset)
    }

    pub fn clock_slot(&mut self) -> u8 {
        self.op(OP_CLOCK_SLOT, NO_INDEX, NO_INDEX, NO_INDEX, 0)
    }

    pub fn clock_timestamp(&mut self) -> u8 {
        self.op(OP_CLOCK_TIMESTAMP, NO_INDEX, NO_INDEX, NO_INDEX, 0)
    }

    /// Emits a two-operand instruction (arithmetic, comparison, or boolean).
    pub fn binary(&mut self, opcode: u8, left: u8, right: u8) -> u8 {
        self.op(opcode, left, right, NO_INDEX, 0)
    }

    pub fn not(&mut self, value: u8) -> u8 {
        self.op(OP_NOT, value, NO_INDEX, NO_INDEX, 0)
    }

    pub fn select(&mut self, condition: u8, if_true: u8, if_false: u8) -> u8 {
        self.op(OP_SELECT, condition, if_true, if_false, 0)
    }

    pub fn cast(&mut self, opcode: u8, value: u8) -> u8 {
        self.op(opcode, value, NO_INDEX, NO_INDEX, 0)
    }

    pub fn loop_index(&mut self) -> u8 {
        self.op(OP_LOOP_INDEX, NO_INDEX, NO_INDEX, NO_INDEX, 0)
    }

    pub fn require(&mut self, condition: u8) {
        self.emit(record(OP_REQUIRE, NO_INDEX, condition, NO_INDEX, NO_INDEX, 0, 0));
    }

    /// Copies `source` into an existing register, typically a loop-carried one.
    pub fn mov(&mut self, dst: u8, source: u8) {
        self.emit(record(OP_MOVE, dst, source, NO_INDEX, NO_INDEX, 0, 0));
    }

    /// Emits an account data read whose offset comes from a `u64` register.
    pub fn read_dynamic(&mut self, opcode: u8, account: u8, offset_register: u8) -> u8 {
        let dst = self.register();
        self.emit(record(
            opcode,
            dst,
            account,
            offset_register,
            NO_INDEX,
            INSTRUCTION_FLAG_DYNAMIC_OFFSET,
            0,
        ));
        dst
    }

    /// Declares a CPI and returns its descriptor index. The declared maximum data length is the
    /// sum of the segment widths; bytes segments contribute zero and need
    /// [`ProgramBuilder::set_cpi_max_data_len`].
    pub fn cpi(&mut self, program: u8, accounts: &[(u8, u8)], segments: &[Segment]) -> u8 {
        let account_start = self.cpi_accounts.len() as u16;
        for (account, flags) in accounts {
            self.cpi_accounts.push(CpiAccountRecord {
                account: *account,
                flags: *flags,
            });
        }
        let segment_start = self.segments.len() as u16;
        let mut max_data_len = 0u16;
        for segment in segments {
            max_data_len = max_data_len.saturating_add(self.push_segment(*segment));
        }
        self.cpis.push(CpiDescriptor {
            program_account: program,
            reserved0: 0,
            account_start_le: account_start.to_le_bytes(),
            account_len: accounts.len() as u8,
            segment_len: segments.len() as u8,
            segment_start_le: segment_start.to_le_bytes(),
            max_data_len_le: max_data_len.to_le_bytes(),
            reserved1: [0; 2],
        });
        (self.cpis.len() - 1) as u8
    }

    /// Overrides the declared maximum data length of a CPI.
    pub fn set_cpi_max_data_len(&mut self, cpi: u8, len: u16) {
        self.cpis[cpi as usize].max_data_len_le = len.to_le_bytes();
    }

    /// Emits an invoke of `cpi`, optionally guarded by a boolean register.
    pub fn invoke(&mut self, cpi: u8, condition: Option<u8>) {
        self.emit(record(
            OP_INVOKE,
            NO_INDEX,
            cpi,
            condition.unwrap_or(NO_INDEX),
            NO_INDEX,
            0,
            0,
        ));
    }

    /// Emits a FOREACH whose body is produced by `body` and returns the FOREACH index.
    pub fn for_each(&mut self, carry_mask: u64, body: impl FnOnce(&mut Self)) -> usize {
        let index = self.emit(record(
            OP_FOREACH,
            NO_INDEX,
            0,
            NO_INDEX,
            NO_INDEX,
            0,
            carry_mask,
        ));
        body(self);
        let body_len = (self.instructions.len() - index - 1) as u8;
        self.instructions[index].a = body_len;
        index
    }

    /// Pushes PDA seed segments and emits DERIVE_PDA against `program`.
    pub fn derive_pda(&mut self, program: u8, seeds: &[Segment]) -> u8 {
        let start = self.segments.len() as u16;
        for seed in seeds {
            self.push_segment(*seed);
        }
        self.op(
            OP_DERIVE_PDA,
            program,
            NO_INDEX,
            NO_INDEX,
            range_immediate(start, seeds.len() as u16),
        )
    }

    /// Mutable access to the emitted instructions, for negative tests.
    pub fn instructions_mut(&mut self) -> &mut Vec<InstructionRecord> {
        &mut self.instructions
    }

    /// Mutable access to the account constraints (fixed first, then row), for negative tests.
    pub fn accounts_mut(&mut self) -> (&mut Vec<AccountConstraint>, &mut Vec<AccountConstraint>) {
        (&mut self.fixed, &mut self.row)
    }

    /// Mutable access to the data segments, for negative tests.
    pub fn segments_mut(&mut self) -> &mut Vec<DataSegment> {
        &mut self.segments
    }

    /// Mutable access to the CPI descriptors, for negative tests.
    pub fn cpis_mut(&mut self) -> &mut Vec<CpiDescriptor> {
        &mut self.cpis
    }

    /// Serializes the program. Only the payload size is checked here; callers verify separately.
    pub fn build(&self) -> Result<Vec<u8>, TemplateError> {
        let header = ProgramHeader::new(
            self.fixed.len() as u8,
            self.row.len() as u8,
            self.batch_max,
            self.batch_min,
            self.inputs.len() as u8,
            self.next_register,
            self.instructions.len() as u8,
            self.cpis.len() as u8,
            self.cpi_accounts.len() as u16,
            self.segments.len() as u16,
            self.pubkeys.len() as u8,
            self.flags,
            self.blob.len() as u16,
        );
        let mut bytes = header.as_bytes().to_vec();
        for constraint in self.fixed.iter().chain(self.row.iter()) {
            bytes.extend_from_slice(constraint.as_bytes());
        }
        for input in &self.inputs {
            bytes.extend_from_slice(input.as_bytes());
        }
        for instruction in &self.instructions {
            bytes.extend_from_slice(instruction.as_bytes());
        }
        for cpi in &self.cpis {
            bytes.extend_from_slice(cpi.as_bytes());
        }
        for account in &self.cpi_accounts {
            bytes.extend_from_slice(account.as_bytes());
        }
        for segment in &self.segments {
            bytes.extend_from_slice(segment.as_bytes());
        }
        for pubkey in &self.pubkeys {
            bytes.extend_from_slice(pubkey.as_bytes());
        }
        bytes.extend_from_slice(&self.blob);
        if bytes.len() > MAX_TEMPLATE_PAYLOAD_LEN {
            return Err(TemplateError::PayloadTooLarge(bytes.len()));
        }
        Ok(bytes)
    }

    fn constraint(
        &mut self,
        flags: u8,
        address: Option<[u8; 32]>,
        owner: Option<[u8; 32]>,
        min_data_len: u32,
    ) -> AccountConstraint {
        AccountConstraint {
            flags,
            address_index: address.map_or(NO_INDEX, |key| self.pubkey(key)),
            owner_index: owner.map_or(NO_INDEX, |key| self.pubkey(key)),
            reserved: 0,
            min_data_len_le: min_data_len.to_le_bytes(),
        }
    }

    fn push_segment(&mut self, segment: Segment) -> u16 {
        let (record, width) = match segment {
            Segment::Literal((offset, len)) => (
                DataSegment {
                    kind: DATA_LITERAL,
                    register: NO_INDEX,
                    offset_le: offset.to_le_bytes(),
                    len_le: len.to_le_bytes(),
                    reserved: [0; 2],
                },
                len,
            ),
            Segment::Register(kind, register) => (
                DataSegment {
                    kind,
                    register,
                    offset_le: [0; 2],
                    len_le: [0; 2],
                    reserved: [0; 2],
                },
                segment_width(kind),
            ),
        };
        self.segments.push(record);
        width
    }
}

/// Builds an instruction record.
pub fn record(opcode: u8, dst: u8, a: u8, b: u8, c: u8, flags: u8, immediate: u64) -> InstructionRecord {
    InstructionRecord {
        opcode,
        dst,
        a,
        b,
        c,
        flags,
        immediate_le: immediate.to_le_bytes(),
        reserved: [0; 2],
    }
}

/// Packs a `(start, len)` pair the way `InstructionRecord::blob_range` unpacks it.
pub fn range_immediate(start: u16, len: u16) -> u64 {
    start as u64 | ((len as u64) << 32)
}

/// Encoded width of a register data segment kind; zero for bytes and unknown kinds.
pub fn segment_width(kind: u8) -> u16 {
    match kind {
        DATA_REG_U8 | DATA_REG_BOOL => 1,
        DATA_REG_U16 => 2,
        DATA_REG_U32 => 4,
        DATA_REG_U64 | DATA_REG_I64 => 8,
        DATA_REG_U128 => 16,
        DATA_REG_PUBKEY => 32,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_system_transfer_program_that_verifies_like_the_compiler_fixture() {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let amount = builder.input(VALUE_U64, 0);
        let amount_register = builder.load_input(amount);
        let literal = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &[
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (destination, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(literal),
                Segment::Register(DATA_REG_U64, amount_register),
            ],
        );
        builder.invoke(cpi, None);

        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(program.header.version(), TEMPLATE_PROGRAM_VERSION);
        assert_eq!(bytes.len(), 152, "same size as the TypeScript fixture");
        assert_eq!(
            program.verify().unwrap(),
            VerificationStats {
                fixed_accounts: 3,
                batch_stride: 0,
                batch_max_iterations: 0,
                inputs: 1,
                registers: 1,
                instructions: 2,
                cpis: 1,
                max_expanded_cpis: 1,
                max_cpi_data_len: 12,
            }
        );
    }

    #[test]
    fn for_each_patches_the_body_length_and_interns_pubkeys() {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let same_system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(4, 0);
        let amount = builder.const_u64(7);
        let literal = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &[(recipient, ACCOUNT_WRITABLE)],
            &[Segment::Literal(literal), Segment::Register(DATA_REG_U64, amount)],
        );
        let index = builder.for_each(0, |body| {
            let key = body.account_key(recipient);
            let same = body.binary(OP_EQ, key, key);
            body.require(same);
            body.invoke(cpi, None);
        });
        let _ = same_system;

        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(program.pubkeys.len(), 1);
        assert_eq!(program.instructions[index].opcode, OP_FOREACH);
        assert_eq!(program.instructions[index].a, 4);
        let stats = program.verify().unwrap();
        assert_eq!(stats.batch_stride, 1);
        assert_eq!(stats.max_expanded_cpis, 4);
    }

    #[test]
    fn oversized_payloads_are_rejected_at_build_time() {
        let mut builder = ProgramBuilder::new();
        builder.blob(&[0; MAX_TEMPLATE_PAYLOAD_LEN]);
        builder.const_bool(true);
        assert!(matches!(
            builder.build(),
            Err(TemplateError::PayloadTooLarge(_))
        ));
    }
}
