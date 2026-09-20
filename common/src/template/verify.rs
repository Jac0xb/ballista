use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationStats {
    pub fixed_accounts: u8,
    pub batch_stride: u8,
    pub batch_max_iterations: u8,
    pub inputs: u8,
    pub registers: u8,
    pub instructions: u8,
    pub cpis: u8,
    pub max_expanded_cpis: u8,
    pub max_cpi_data_len: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RegisterInfo {
    value_type: u8,
    bytes_max_len: usize,
}

impl RegisterInfo {
    const fn scalar(value_type: u8) -> Self {
        Self {
            value_type,
            bytes_max_len: 0,
        }
    }

    const fn bytes(max_len: usize) -> Self {
        Self {
            value_type: VALUE_BYTES,
            bytes_max_len: max_len,
        }
    }

    const fn is_numeric(self) -> bool {
        matches!(self.value_type, VALUE_U64 | VALUE_I64 | VALUE_U128)
    }
}

impl ProgramView<'_> {
    pub fn verify(&self) -> Result<VerificationStats, TemplateError> {
        let header = self.header;
        if header.input_count() > MAX_INPUTS {
            return Err(TemplateError::TooManyInputs);
        }
        if header.register_count() > MAX_REGISTERS {
            return Err(TemplateError::TooManyRegisters);
        }
        if header.instruction_count() == 0 || header.instruction_count() > MAX_VM_INSTRUCTIONS {
            return Err(TemplateError::TooManyInstructions);
        }

        let runtime_account_capacity = header
            .fixed_account_count()
            .checked_add(
                header
                    .batch_stride()
                    .checked_mul(header.batch_max_iterations())
                    .ok_or(TemplateError::CountOverflow)?,
            )
            .ok_or(TemplateError::CountOverflow)?;
        if runtime_account_capacity > MAX_RUNTIME_ACCOUNTS {
            return Err(TemplateError::TooManyAccounts);
        }
        if header.batch_stride() > MAX_BATCH_STRIDE
            || (header.batch_stride() == 0) != (header.batch_max_iterations() == 0)
        {
            return Err(TemplateError::InvalidBatch);
        }
        if header.batch_min_iterations() > header.batch_max_iterations() {
            return Err(TemplateError::InvalidMinIterations);
        }

        for (index, constraint) in self.accounts.iter().enumerate() {
            if constraint.flags & !ACCOUNT_FLAGS_MASK != 0
                || constraint.reserved != 0
                || !self.valid_optional_pubkey(constraint.address_index)
                || !self.valid_optional_pubkey(constraint.owner_index)
            {
                return Err(TemplateError::InvalidAccountConstraint(index));
            }
        }

        for (index, input) in self.inputs.iter().enumerate() {
            if input.reserved != 0 || !is_value_type(input.value_type) {
                return Err(TemplateError::InvalidInput(index));
            }
            if input.value_type == VALUE_BYTES {
                if input.max_len() == 0 || input.max_len() > MAX_INPUT_BYTES {
                    return Err(TemplateError::InvalidInput(index));
                }
            } else if input.max_len() != 0 {
                return Err(TemplateError::InvalidInput(index));
            }
        }

        let mut registers = [None; MAX_REGISTERS];
        let mut program_counter = 0usize;
        let mut root_cpis = 0usize;
        let mut foreach_count = 0usize;
        let mut max_cpi_data_len = 0usize;
        // The previous root-level instruction, if the previous instruction was not a loop body.
        let mut previous: Option<&InstructionRecord> = None;

        while program_counter < self.instructions.len() {
            let instruction = &self.instructions[program_counter];
            self.verify_record_header(instruction, program_counter)?;
            if instruction.opcode == OP_FOREACH {
                foreach_count += 1;
                if header.batch_stride() == 0 || foreach_count > 1 || instruction.a == 0 {
                    return Err(TemplateError::InvalidBatch);
                }
                let body_start = program_counter + 1;
                let body_end = body_start
                    .checked_add(instruction.a as usize)
                    .ok_or(TemplateError::CountOverflow)?;
                if body_end > self.instructions.len() {
                    return Err(TemplateError::InvalidInstruction(program_counter));
                }

                let carry = instruction.immediate();
                self.verify_carry_before(carry, &registers)?;
                let mut body_registers = registers;
                let (body_cpis, body_max_data) =
                    self.verify_range(body_start, body_end, true, &mut body_registers)?;
                verify_carry_after(carry, &registers, &body_registers)?;
                root_cpis = root_cpis
                    .checked_add(
                        body_cpis
                            .checked_mul(header.batch_max_iterations())
                            .ok_or(TemplateError::CountOverflow)?,
                    )
                    .ok_or(TemplateError::CountOverflow)?;
                max_cpi_data_len = max_cpi_data_len.max(body_max_data);
                program_counter = body_end;
                previous = None;
                continue;
            }

            let (cpis, data_len) = self.verify_instruction(
                instruction,
                program_counter,
                false,
                previous,
                &mut registers,
            )?;
            root_cpis += cpis;
            max_cpi_data_len = max_cpi_data_len.max(data_len);
            previous = Some(instruction);
            program_counter += 1;
        }

        if (header.batch_stride() == 0 && foreach_count != 0)
            || (header.batch_stride() != 0 && foreach_count != 1)
        {
            return Err(TemplateError::InvalidBatch);
        }
        if root_cpis > MAX_EXPANDED_CPIS {
            return Err(TemplateError::ExcessiveCpiExpansion);
        }
        // Descriptors that no instruction invokes still shape the executor's scratch buffers.
        for index in 0..self.cpis.len() {
            self.verify_cpi_shape(index)?;
        }

        Ok(VerificationStats {
            fixed_accounts: header.fixed_account_count() as u8,
            batch_stride: header.batch_stride() as u8,
            batch_max_iterations: header.batch_max_iterations() as u8,
            inputs: header.input_count() as u8,
            registers: header.register_count() as u8,
            instructions: header.instruction_count() as u8,
            cpis: header.cpi_count() as u8,
            max_expanded_cpis: root_cpis as u8,
            max_cpi_data_len: max_cpi_data_len as u16,
        })
    }

    fn verify_range(
        &self,
        start: usize,
        end: usize,
        in_loop: bool,
        registers: &mut [Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(usize, usize), TemplateError> {
        let mut cpis = 0usize;
        let mut max_data_len = 0usize;
        let mut previous: Option<&InstructionRecord> = None;
        for index in start..end {
            let instruction = &self.instructions[index];
            self.verify_record_header(instruction, index)?;
            if instruction.opcode == OP_FOREACH {
                return Err(TemplateError::InvalidBatch);
            }
            let (instruction_cpis, data_len) =
                self.verify_instruction(instruction, index, in_loop, previous, registers)?;
            cpis += instruction_cpis;
            max_data_len = max_data_len.max(data_len);
            previous = Some(instruction);
        }
        Ok((cpis, max_data_len))
    }

    fn verify_record_header(
        &self,
        instruction: &InstructionRecord,
        index: usize,
    ) -> Result<(), TemplateError> {
        if instruction.reserved != [0; 2] {
            return Err(TemplateError::InvalidInstruction(index));
        }
        let allowed_flags = if read_width(instruction.opcode) != 0 {
            INSTRUCTION_FLAG_DYNAMIC_OFFSET
        } else {
            0
        };
        if instruction.flags & !allowed_flags != 0 {
            return Err(TemplateError::InvalidFlags(index));
        }
        Ok(())
    }

    /// Every carried register must exist and hold a value before the loop starts, so a run with
    /// zero iterations still leaves it readable afterwards.
    fn verify_carry_before(
        &self,
        carry: u64,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(), TemplateError> {
        for register in 0..MAX_REGISTERS as u8 {
            if carry & (1u64 << register) == 0 {
                continue;
            }
            if register as usize >= self.header.register_count()
                || registers[register as usize].is_none()
            {
                return Err(TemplateError::InvalidCarry(register));
            }
        }
        Ok(())
    }

    /// `previous` is the instruction immediately before this one within the same range (root or
    /// loop body), or `None` at a range boundary.
    fn verify_instruction(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        in_loop: bool,
        previous: Option<&InstructionRecord>,
        registers: &mut [Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(usize, usize), TemplateError> {
        let scalar = |value_type| RegisterInfo::scalar(value_type);
        match instruction.opcode {
            OP_LOAD_INPUT => {
                let input = self
                    .inputs
                    .get(instruction.a as usize)
                    .ok_or(TemplateError::InvalidInstruction(instruction_index))?;
                let info = if input.value_type == VALUE_BYTES {
                    RegisterInfo::bytes(input.max_len())
                } else {
                    scalar(input.value_type)
                };
                self.write_register(registers, instruction.dst, info)?;
            }
            OP_CONST_BOOL => {
                if instruction.a > 1 {
                    return Err(TemplateError::InvalidInstruction(instruction_index));
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_CONST_U64 => self.write_register(registers, instruction.dst, scalar(VALUE_U64))?,
            OP_CONST_I64 => self.write_register(registers, instruction.dst, scalar(VALUE_I64))?,
            OP_CONST_U128 => {
                let (offset, len) = instruction.blob_range();
                if len != 16 || !valid_range(self.blob.len(), offset, len) {
                    return Err(TemplateError::InvalidBlobRange);
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_U128))?;
            }
            OP_CONST_PUBKEY => {
                if instruction.a as usize >= self.pubkeys.len() {
                    return Err(TemplateError::InvalidInstruction(instruction_index));
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
            }
            OP_CONST_BYTES => {
                let (offset, len) = instruction.blob_range();
                if !valid_range(self.blob.len(), offset, len) || len > MAX_INPUT_BYTES {
                    return Err(TemplateError::InvalidBlobRange);
                }
                self.write_register(registers, instruction.dst, RegisterInfo::bytes(len))?;
            }
            OP_ACCOUNT_KEY | OP_ACCOUNT_OWNER => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
            }
            OP_ACCOUNT_LAMPORTS | OP_ACCOUNT_DATA_LEN => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_ACCOUNT_IS_EMPTY => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_U128
            | OP_READ_PUBKEY | OP_READ_BOOL => {
                if instruction.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0 {
                    self.require_account(instruction.a, in_loop)?;
                    self.require_type(registers, instruction.b, VALUE_U64)?;
                    if instruction.immediate() != 0 {
                        return Err(TemplateError::InvalidFlags(instruction_index));
                    }
                } else {
                    self.verify_read_bounds(instruction, instruction_index, in_loop)?;
                }
                let value_type = match instruction.opcode {
                    OP_READ_I64 => VALUE_I64,
                    OP_READ_U128 => VALUE_U128,
                    OP_READ_PUBKEY => VALUE_PUBKEY,
                    OP_READ_BOOL => VALUE_BOOL,
                    _ => VALUE_U64,
                };
                self.write_register(registers, instruction.dst, scalar(value_type))?;
            }
            OP_CLOCK_SLOT => self.write_register(registers, instruction.dst, scalar(VALUE_U64))?,
            OP_CLOCK_TIMESTAMP => {
                self.write_register(registers, instruction.dst, scalar(VALUE_I64))?
            }
            OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left != right || !left.is_numeric() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, left)?;
            }
            OP_EQ | OP_NE => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left.value_type != right.value_type {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_LT | OP_LTE | OP_GT | OP_GTE => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left != right || !left.is_numeric() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_AND | OP_OR => {
                self.require_type(registers, instruction.a, VALUE_BOOL)?;
                self.require_type(registers, instruction.b, VALUE_BOOL)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_NOT => {
                self.require_type(registers, instruction.a, VALUE_BOOL)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_SELECT => {
                self.require_type(registers, instruction.a, VALUE_BOOL)?;
                let if_true = self.read_register(registers, instruction.b)?;
                let if_false = self.read_register(registers, instruction.c)?;
                if if_true.value_type != if_false.value_type {
                    return Err(TemplateError::TypeMismatch);
                }
                let selected = if if_true.value_type == VALUE_BYTES {
                    RegisterInfo::bytes(if_true.bytes_max_len.max(if_false.bytes_max_len))
                } else {
                    if_true
                };
                self.write_register(registers, instruction.dst, selected)?;
            }
            OP_CAST_U64 | OP_CAST_I64 | OP_CAST_U128 => {
                let source = self.read_register(registers, instruction.a)?;
                if !source.is_numeric() {
                    return Err(TemplateError::TypeMismatch);
                }
                let target = match instruction.opcode {
                    OP_CAST_U64 => VALUE_U64,
                    OP_CAST_I64 => VALUE_I64,
                    _ => VALUE_U128,
                };
                self.write_register(registers, instruction.dst, scalar(target))?;
            }
            OP_LOOP_INDEX => {
                if !in_loop {
                    return Err(TemplateError::InvalidInstruction(instruction_index));
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_MOVE => {
                let source = self.read_register(registers, instruction.a)?;
                self.write_register(registers, instruction.dst, source)?;
            }
            OP_RETURN_DATA => {
                // Return data is only meaningful straight after the CPI that produced it, and only
                // when that CPI always runs.
                let follows_invoke = previous
                    .is_some_and(|record| record.opcode == OP_INVOKE && record.b == NO_INDEX);
                let width = read_width(instruction.a);
                let end = usize::try_from(instruction.immediate())
                    .ok()
                    .and_then(|offset| offset.checked_add(width));
                if !follows_invoke || width == 0 || end.is_none_or(|end| end > MAX_RETURN_DATA_LEN)
                {
                    return Err(TemplateError::InvalidReturnData(instruction_index));
                }
                let value_type = match instruction.a {
                    OP_READ_I64 => VALUE_I64,
                    OP_READ_U128 => VALUE_U128,
                    OP_READ_PUBKEY => VALUE_PUBKEY,
                    OP_READ_BOOL => VALUE_BOOL,
                    _ => VALUE_U64,
                };
                self.write_register(registers, instruction.dst, scalar(value_type))?;
            }
            OP_DERIVE_PDA => {
                let program = self
                    .account_constraint(instruction.a, in_loop)
                    .ok_or(TemplateError::InvalidInstruction(instruction_index))?;
                if program.flags & ACCOUNT_EXECUTABLE == 0 {
                    return Err(TemplateError::InvalidInstruction(instruction_index));
                }
                let (segment_start, segment_len) = instruction.blob_range();
                let segment_end = segment_start
                    .checked_add(segment_len)
                    .ok_or(TemplateError::CountOverflow)?;
                if segment_len == 0
                    || segment_len > MAX_PDA_SEEDS
                    || segment_end > self.data_segments.len()
                {
                    return Err(TemplateError::InvalidInstruction(instruction_index));
                }
                for (offset, segment) in self.data_segments[segment_start..segment_end]
                    .iter()
                    .enumerate()
                {
                    let seed_len =
                        self.verify_pda_seed_segment(segment_start + offset, segment, registers)?;
                    if seed_len > MAX_PDA_SEED_LEN {
                        return Err(TemplateError::InvalidDataSegment(segment_start + offset));
                    }
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
            }
            OP_REQUIRE => self.require_type(registers, instruction.a, VALUE_BOOL)?,
            OP_INVOKE => {
                if instruction.b != NO_INDEX {
                    self.require_type(registers, instruction.b, VALUE_BOOL)?;
                }
                let max_data = self.verify_cpi(instruction.a as usize, in_loop, registers)?;
                return Ok((1, max_data));
            }
            OP_FOREACH => return Err(TemplateError::InvalidBatch),
            _ => return Err(TemplateError::InvalidInstruction(instruction_index)),
        }
        Ok((0, 0))
    }

    fn verify_pda_seed_segment(
        &self,
        index: usize,
        segment: &DataSegment,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<usize, TemplateError> {
        if segment.reserved != [0; 2] {
            return Err(TemplateError::InvalidDataSegment(index));
        }
        if segment.kind == DATA_LITERAL {
            if segment.register != NO_INDEX
                || !valid_range(self.blob.len(), segment.offset(), segment.len())
            {
                return Err(TemplateError::InvalidDataSegment(index));
            }
            return Ok(segment.len());
        }
        if segment.offset() != 0 || segment.len() != 0 {
            return Err(TemplateError::InvalidDataSegment(index));
        }
        let len = match segment.kind {
            DATA_REG_U8 | DATA_REG_U16 | DATA_REG_U32 | DATA_REG_U64 => {
                let register = self.read_register(registers, segment.register)?;
                if !matches!(register.value_type, VALUE_U64 | VALUE_U128) {
                    return Err(TemplateError::TypeMismatch);
                }
                match segment.kind {
                    DATA_REG_U8 => 1,
                    DATA_REG_U16 => 2,
                    DATA_REG_U32 => 4,
                    _ => 8,
                }
            }
            DATA_REG_I64 => {
                self.require_type(registers, segment.register, VALUE_I64)?;
                8
            }
            DATA_REG_U128 => {
                self.require_type(registers, segment.register, VALUE_U128)?;
                16
            }
            DATA_REG_PUBKEY => {
                self.require_type(registers, segment.register, VALUE_PUBKEY)?;
                32
            }
            DATA_REG_BOOL => {
                self.require_type(registers, segment.register, VALUE_BOOL)?;
                1
            }
            DATA_REG_BYTES => {
                let register = self.read_register(registers, segment.register)?;
                if register.value_type != VALUE_BYTES {
                    return Err(TemplateError::TypeMismatch);
                }
                register.bytes_max_len
            }
            _ => return Err(TemplateError::InvalidDataSegment(index)),
        };
        Ok(len)
    }

    /// A fixed-offset read must stay inside the account's declared minimum data length, so the
    /// runtime never fails on an offset the author could have caught at compile time.
    fn verify_read_bounds(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        in_loop: bool,
    ) -> Result<(), TemplateError> {
        let constraint = self
            .account_constraint(instruction.a, in_loop)
            .ok_or(TemplateError::InvalidAccountConstraint(instruction.a as usize))?;
        let width = read_width(instruction.opcode);
        let end = usize::try_from(instruction.immediate())
            .ok()
            .and_then(|offset| offset.checked_add(width))
            .ok_or(TemplateError::ReadOutOfBounds(instruction_index))?;
        if end > constraint.min_data_len() {
            return Err(TemplateError::ReadOutOfBounds(instruction_index));
        }
        Ok(())
    }

    /// Structural checks that apply to every CPI descriptor, invoked or not.
    fn verify_cpi_shape(&self, index: usize) -> Result<&CpiDescriptor, TemplateError> {
        let descriptor = self
            .cpis
            .get(index)
            .ok_or(TemplateError::InvalidCpi(index))?;
        if descriptor.reserved0 != 0 || descriptor.reserved1 != [0; 2] {
            return Err(TemplateError::InvalidCpi(index));
        }
        if descriptor.account_len as usize > MAX_CPI_ACCOUNTS {
            return Err(TemplateError::TooManyCpiAccounts(index));
        }
        if descriptor.max_data_len() > MAX_CPI_DATA_LEN {
            return Err(TemplateError::InvalidCpi(index));
        }
        let account_end = descriptor
            .account_start()
            .checked_add(descriptor.account_len as usize)
            .ok_or(TemplateError::CountOverflow)?;
        let segment_end = descriptor
            .segment_start()
            .checked_add(descriptor.segment_len as usize)
            .ok_or(TemplateError::CountOverflow)?;
        if account_end > self.cpi_accounts.len() || segment_end > self.data_segments.len() {
            return Err(TemplateError::InvalidCpi(index));
        }
        Ok(descriptor)
    }

    fn verify_cpi(
        &self,
        index: usize,
        in_loop: bool,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<usize, TemplateError> {
        let descriptor = self.verify_cpi_shape(index)?;
        let account_end = descriptor.account_start() + descriptor.account_len as usize;
        let segment_end = descriptor.segment_start() + descriptor.segment_len as usize;
        let program = self
            .account_constraint(descriptor.program_account, in_loop)
            .ok_or(TemplateError::InvalidCpi(index))?;
        if program.flags & ACCOUNT_EXECUTABLE == 0 {
            return Err(TemplateError::InvalidCpi(index));
        }

        for cpi_account in &self.cpi_accounts[descriptor.account_start()..account_end] {
            if cpi_account.flags & !(ACCOUNT_SIGNER | ACCOUNT_WRITABLE) != 0 {
                return Err(TemplateError::InvalidCpi(index));
            }
            let constraint = self
                .account_constraint(cpi_account.account, in_loop)
                .ok_or(TemplateError::InvalidCpi(index))?;
            if cpi_account.flags & !constraint.flags != 0 {
                return Err(TemplateError::InvalidCpi(index));
            }
        }

        let mut max_len = 0usize;
        for (segment_index, segment) in self.data_segments[descriptor.segment_start()..segment_end]
            .iter()
            .enumerate()
        {
            if segment.reserved != [0; 2] {
                return Err(TemplateError::InvalidDataSegment(segment_index));
            }
            let len = match segment.kind {
                DATA_LITERAL => {
                    if !valid_range(self.blob.len(), segment.offset(), segment.len()) {
                        return Err(TemplateError::InvalidBlobRange);
                    }
                    segment.len()
                }
                DATA_REG_U8 | DATA_REG_U16 | DATA_REG_U32 | DATA_REG_U64 => {
                    let register = self.read_register(registers, segment.register)?;
                    if !matches!(register.value_type, VALUE_U64 | VALUE_U128) {
                        return Err(TemplateError::TypeMismatch);
                    }
                    match segment.kind {
                        DATA_REG_U8 => 1,
                        DATA_REG_U16 => 2,
                        DATA_REG_U32 => 4,
                        _ => 8,
                    }
                }
                DATA_REG_I64 => {
                    self.require_type(registers, segment.register, VALUE_I64)?;
                    8
                }
                DATA_REG_U128 => {
                    self.require_type(registers, segment.register, VALUE_U128)?;
                    16
                }
                DATA_REG_PUBKEY => {
                    self.require_type(registers, segment.register, VALUE_PUBKEY)?;
                    32
                }
                DATA_REG_BOOL => {
                    self.require_type(registers, segment.register, VALUE_BOOL)?;
                    1
                }
                DATA_REG_BYTES => {
                    let register = self.read_register(registers, segment.register)?;
                    if register.value_type != VALUE_BYTES {
                        return Err(TemplateError::TypeMismatch);
                    }
                    register.bytes_max_len
                }
                _ => return Err(TemplateError::InvalidDataSegment(segment_index)),
            };
            max_len = max_len
                .checked_add(len)
                .ok_or(TemplateError::CountOverflow)?;
        }
        if max_len > MAX_CPI_DATA_LEN || descriptor.max_data_len() != max_len {
            return Err(TemplateError::InvalidCpi(index));
        }
        Ok(max_len)
    }

    fn valid_optional_pubkey(&self, index: u8) -> bool {
        index == NO_INDEX || (index as usize) < self.pubkeys.len()
    }

    fn require_account(&self, reference: u8, in_loop: bool) -> Result<(), TemplateError> {
        self.account_constraint(reference, in_loop)
            .map(|_| ())
            .ok_or(TemplateError::InvalidAccountConstraint(reference as usize))
    }

    fn write_register(
        &self,
        registers: &mut [Option<RegisterInfo>; MAX_REGISTERS],
        index: u8,
        value: RegisterInfo,
    ) -> Result<(), TemplateError> {
        if index as usize >= self.header.register_count() {
            return Err(TemplateError::InvalidRegister(index));
        }
        registers[index as usize] = Some(value);
        Ok(())
    }

    fn read_register(
        &self,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
        index: u8,
    ) -> Result<RegisterInfo, TemplateError> {
        if index as usize >= self.header.register_count() {
            return Err(TemplateError::InvalidRegister(index));
        }
        registers[index as usize].ok_or(TemplateError::RegisterNotInitialized(index))
    }

    fn require_type(
        &self,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
        index: u8,
        value_type: u8,
    ) -> Result<(), TemplateError> {
        if self.read_register(registers, index)?.value_type != value_type {
            return Err(TemplateError::TypeMismatch);
        }
        Ok(())
    }
}

const fn is_value_type(value_type: u8) -> bool {
    matches!(
        value_type,
        VALUE_BOOL | VALUE_U64 | VALUE_I64 | VALUE_U128 | VALUE_PUBKEY | VALUE_BYTES
    )
}

fn valid_range(total: usize, offset: usize, len: usize) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= total)
}

/// A carried register must leave the loop body with exactly the type it entered with.
fn verify_carry_after(
    carry: u64,
    before: &[Option<RegisterInfo>; MAX_REGISTERS],
    after: &[Option<RegisterInfo>; MAX_REGISTERS],
) -> Result<(), TemplateError> {
    for register in 0..MAX_REGISTERS as u8 {
        if carry & (1u64 << register) != 0 && before[register as usize] != after[register as usize]
        {
            return Err(TemplateError::InvalidCarry(register));
        }
    }
    Ok(())
}

/// Bytes read from account data by each `OP_READ_*` opcode.
pub const fn read_width(opcode: u8) -> usize {
    match opcode {
        OP_READ_U8 | OP_READ_BOOL => 1,
        OP_READ_U16 => 2,
        OP_READ_U32 => 4,
        OP_READ_U64 | OP_READ_I64 => 8,
        OP_READ_U128 => 16,
        OP_READ_PUBKEY => 32,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_bytes(bytes: &[u8]) -> Result<VerificationStats, TemplateError> {
        ProgramView::parse(bytes)?.verify()
    }

    fn verify_builder(builder: &ProgramBuilder) -> Result<VerificationStats, TemplateError> {
        verify_bytes(&builder.build()?)
    }

    /// Emits a constant of the requested type, or allocates an uninitialized register for `None`.
    fn typed_register(builder: &mut ProgramBuilder, value_type: Option<u8>) -> u8 {
        match value_type {
            None => builder.register(),
            Some(VALUE_BOOL) => builder.const_bool(true),
            Some(VALUE_U64) => builder.const_u64(1),
            Some(VALUE_I64) => builder.const_i64(-1),
            Some(VALUE_U128) => builder.const_u128(1),
            Some(VALUE_PUBKEY) => builder.const_pubkey([9; 32]),
            Some(VALUE_BYTES) => builder.const_bytes(&[1, 2, 3]),
            Some(other) => panic!("unsupported value type {other}"),
        }
    }

    /// A single-CPI system transfer built with `account_flags` on the source and destination.
    fn transfer_builder(source_flags: u8, destination_flags: u8) -> (ProgramBuilder, u8) {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let source = builder.account(source_flags, None, None, 0);
        let destination = builder.account(destination_flags, None, None, 0);
        let amount = builder.const_u64(5);
        let literal = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &[
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (destination, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(literal),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        (builder, cpi)
    }

    #[test]
    fn header_limits_are_enforced_at_the_boundary() {
        // Register count lives at header byte 9 and does not change any section size.
        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        let mut bytes = builder.build().unwrap();
        bytes[9] = MAX_REGISTERS as u8;
        assert!(verify_bytes(&bytes).is_ok());
        bytes[9] = MAX_REGISTERS as u8 + 1;
        assert_eq!(verify_bytes(&bytes), Err(TemplateError::TooManyRegisters));

        let mut builder = ProgramBuilder::new();
        let condition = builder.const_bool(true);
        for _ in 0..MAX_VM_INSTRUCTIONS - 1 {
            builder.require(condition);
        }
        assert!(verify_builder(&builder).is_ok(), "128 instructions are allowed");
        builder.require(condition);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::TooManyInstructions)
        );

        let mut builder = ProgramBuilder::new();
        for _ in 0..MAX_INPUTS + 1 {
            builder.input(VALUE_U64, 0);
        }
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TooManyInputs));

        let mut builder = ProgramBuilder::new();
        for _ in 0..MAX_RUNTIME_ACCOUNTS + 1 {
            builder.account(0, None, None, 0);
        }
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TooManyAccounts));

        // Eight row accounts times eight iterations is 64 runtime slots.
        let mut builder = ProgramBuilder::new();
        for _ in 0..MAX_BATCH_STRIDE {
            builder.row_account(0, None, None, 0);
        }
        builder.batch(8, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::TooManyAccounts));

        let mut builder = ProgramBuilder::new();
        for _ in 0..MAX_BATCH_STRIDE + 1 {
            builder.row_account(0, None, None, 0);
        }
        builder.batch(1, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));
    }

    #[test]
    fn header_magic_version_flags_and_reserved_bytes_are_checked_at_parse() {
        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        let bytes = builder.build().unwrap();

        let mut wrong_magic = bytes.clone();
        wrong_magic[0] = b'X';
        assert_eq!(
            ProgramView::parse(&wrong_magic).unwrap_err(),
            TemplateError::InvalidMagic
        );

        let mut old_version = bytes.clone();
        old_version[4] = 2;
        assert_eq!(
            ProgramView::parse(&old_version).unwrap_err(),
            TemplateError::UnsupportedVersion(2)
        );

        let mut unknown_flag = bytes.clone();
        unknown_flag[17] = 0x80;
        assert_eq!(
            ProgramView::parse(&unknown_flag).unwrap_err(),
            TemplateError::InvalidReservedBytes
        );

        let mut reserved = bytes.clone();
        reserved[23] = 1;
        assert_eq!(
            ProgramView::parse(&reserved).unwrap_err(),
            TemplateError::InvalidReservedBytes
        );

        let mut short = bytes.clone();
        short.truncate(PROGRAM_HEADER_LEN - 1);
        assert_eq!(
            ProgramView::parse(&short).unwrap_err(),
            TemplateError::Truncated
        );

        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            ProgramView::parse(&trailing).unwrap_err(),
            TemplateError::SectionLengthMismatch
        );
    }

    #[test]
    fn every_opcode_rejects_uninitialized_or_mistyped_operands() {
        // (opcode, type of a, type of b, expected verification outcome)
        let cases: &[(u8, Option<u8>, Option<u8>, Result<(), TemplateError>)] = &[
            (OP_ADD, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_SUB, Some(VALUE_I64), Some(VALUE_I64), Ok(())),
            (OP_MUL, Some(VALUE_U128), Some(VALUE_U128), Ok(())),
            (OP_ADD, Some(VALUE_U64), Some(VALUE_I64), Err(TemplateError::TypeMismatch)),
            (OP_DIV, Some(VALUE_BOOL), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_MIN, Some(VALUE_PUBKEY), Some(VALUE_PUBKEY), Err(TemplateError::TypeMismatch)),
            (OP_ADD, None, Some(VALUE_U64), Err(TemplateError::RegisterNotInitialized(0))),
            (OP_ADD, Some(VALUE_U64), None, Err(TemplateError::RegisterNotInitialized(1))),
            (OP_LT, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_LT, Some(VALUE_PUBKEY), Some(VALUE_PUBKEY), Err(TemplateError::TypeMismatch)),
            (OP_GTE, Some(VALUE_BOOL), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_LTE, Some(VALUE_BYTES), Some(VALUE_BYTES), Err(TemplateError::TypeMismatch)),
            (OP_EQ, Some(VALUE_PUBKEY), Some(VALUE_PUBKEY), Ok(())),
            (OP_NE, Some(VALUE_BYTES), Some(VALUE_BYTES), Ok(())),
            (OP_EQ, Some(VALUE_BOOL), Some(VALUE_BOOL), Ok(())),
            (OP_EQ, Some(VALUE_BYTES), Some(VALUE_PUBKEY), Err(TemplateError::TypeMismatch)),
            (OP_EQ, Some(VALUE_U64), Some(VALUE_U128), Err(TemplateError::TypeMismatch)),
            (OP_AND, Some(VALUE_BOOL), Some(VALUE_BOOL), Ok(())),
            (OP_AND, Some(VALUE_BOOL), Some(VALUE_U64), Err(TemplateError::TypeMismatch)),
            (OP_OR, Some(VALUE_U64), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_NOT, Some(VALUE_BOOL), None, Ok(())),
            (OP_NOT, Some(VALUE_U64), None, Err(TemplateError::TypeMismatch)),
            (OP_CAST_I64, Some(VALUE_U64), None, Ok(())),
            (OP_CAST_U128, Some(VALUE_I64), None, Ok(())),
            (OP_CAST_U64, Some(VALUE_U128), None, Ok(())),
            (OP_CAST_I64, Some(VALUE_BOOL), None, Err(TemplateError::TypeMismatch)),
            (OP_CAST_U64, Some(VALUE_PUBKEY), None, Err(TemplateError::TypeMismatch)),
            (OP_CAST_U128, None, None, Err(TemplateError::RegisterNotInitialized(0))),
            (OP_REQUIRE, Some(VALUE_BOOL), None, Ok(())),
            (OP_REQUIRE, Some(VALUE_U64), None, Err(TemplateError::TypeMismatch)),
            (OP_REQUIRE, None, None, Err(TemplateError::RegisterNotInitialized(0))),
            (OP_LOAD_INPUT, None, None, Err(TemplateError::InvalidInstruction(0))),
            (OP_LOOP_INDEX, None, None, Err(TemplateError::InvalidInstruction(0))),
            (39, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),
            (0xfe, Some(VALUE_U64), Some(VALUE_U64), Err(TemplateError::InvalidInstruction(2))),
        ];
        for (opcode, a, b, expected) in cases {
            let mut builder = ProgramBuilder::new();
            let register_a = typed_register(&mut builder, *a);
            let register_b = typed_register(&mut builder, *b);
            if *opcode == OP_REQUIRE {
                builder.require(register_a);
            } else {
                builder.op(*opcode, register_a, register_b, NO_INDEX, 0);
            }
            let result = verify_builder(&builder).map(|_| ());
            assert_eq!(&result, expected, "opcode {opcode} with a={a:?} b={b:?}");
        }
    }

    #[test]
    fn loop_shape_rules() {
        // A batch schema without a FOREACH.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Stride without iterations, and iterations without stride.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(0, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        let mut builder = ProgramBuilder::new();
        builder.batch(3, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Two loops.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Nested loop.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| {
            body.for_each(0, |inner| inner.require(condition));
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Empty body.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        builder.const_bool(true);
        builder.for_each(0, |_| {});
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Body length past the end of the program.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        let index = builder.for_each(0, |body| body.require(condition));
        builder.instructions_mut()[index].a = 200;
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(index))
        );

        // Loop index outside a loop, and iteration accounts outside a loop.
        let mut builder = ProgramBuilder::new();
        builder.loop_index();
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(1, 0);
        builder.account_key(row);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidAccountConstraint(row as usize))
        );

        // A valid batch reports the expanded CPI count and accepts loop-local reads.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(30, 0);
        let amount = builder.const_u64(1);
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
            let index = body.loop_index();
            let lamports = body.account_lamports(recipient);
            let positive = body.binary(OP_GTE, lamports, index);
            body.require(positive);
            body.invoke(cpi, None);
        });
        let stats = verify_builder(&builder).unwrap();
        assert_eq!(stats.max_expanded_cpis, 30);
        assert_eq!(stats.batch_stride, 1);
    }

    #[test]
    fn cpi_privilege_and_shape_rules() {
        // Escalating signer or writable beyond the account schema.
        let (mut builder, cpi) = transfer_builder(ACCOUNT_WRITABLE, ACCOUNT_WRITABLE);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));
        let (mut builder, cpi) = transfer_builder(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, 0);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));
        let (mut builder, cpi) = transfer_builder(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, ACCOUNT_WRITABLE);
        builder.invoke(cpi, None);
        assert!(verify_builder(&builder).is_ok());

        // Program account must be executable.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(0, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        // Reserved bytes, declared length mismatch, unknown account flags, bad CPI index.
        let (mut builder, cpi) = transfer_builder(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, ACCOUNT_WRITABLE);
        builder.invoke(cpi, None);
        builder.cpis_mut()[0].reserved0 = 1;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        let (mut builder, cpi) = transfer_builder(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, ACCOUNT_WRITABLE);
        builder.invoke(cpi, None);
        builder.set_cpi_max_data_len(cpi, 11);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));
        builder.set_cpi_max_data_len(cpi, 13);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[(program, ACCOUNT_EXECUTABLE)], &[]);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        let mut builder = ProgramBuilder::new();
        builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.invoke(5, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(5)));

        // Guard register must be a bool.
        let (mut builder, cpi) = transfer_builder(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, ACCOUNT_WRITABLE);
        let number = builder.const_u64(1);
        builder.invoke(cpi, Some(number));
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));

        // Iteration account referenced from a root CPI.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let row = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(1, 0);
        let cpi = builder.cpi(program, &[(row, ACCOUNT_WRITABLE)], &[]);
        builder.invoke(cpi, None);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        // Worst-case expansion above 64.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(33, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.for_each(0, |body| {
            body.invoke(cpi, None);
            body.invoke(cpi, None);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::ExcessiveCpiExpansion)
        );
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(32, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.for_each(0, |body| {
            body.invoke(cpi, None);
            body.invoke(cpi, None);
        });
        assert_eq!(verify_builder(&builder).unwrap().max_expanded_cpis, 64);
    }

    #[test]
    fn pda_seed_rules() {
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.derive_pda(program, &[]);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let seed = builder.const_u64(1);
        let seeds = vec![Segment::Register(DATA_REG_U64, seed); MAX_PDA_SEEDS + 1];
        builder.derive_pda(program, &seeds);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(1))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let seed = builder.const_bytes(&[7; MAX_PDA_SEED_LEN + 1]);
        builder.derive_pda(program, &[Segment::Register(DATA_REG_BYTES, seed)]);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidDataSegment(0))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.blob(&[1, 2]);
        builder.derive_pda(program, &[Segment::Literal((1, 5))]);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidDataSegment(0))
        );

        let mut builder = ProgramBuilder::new();
        let not_program = builder.account(0, Some([1; 32]), None, 0);
        let seed = builder.const_u64(1);
        builder.derive_pda(not_program, &[Segment::Register(DATA_REG_U64, seed)]);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(1))
        );

        // Segment range past the table.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.op(OP_DERIVE_PDA, program, NO_INDEX, NO_INDEX, range_immediate(0, 1));
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );

        // Fifteen 32-byte seeds of mixed kinds verify.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let owner = builder.account(0, None, None, 0);
        let key = builder.account_key(owner);
        let bytes = builder.const_bytes(&[3; 32]);
        let literal = builder.blob(b"seed");
        let flag = builder.const_bool(true);
        let mut seeds = vec![
            Segment::Register(DATA_REG_PUBKEY, key),
            Segment::Register(DATA_REG_BYTES, bytes),
            Segment::Literal(literal),
            Segment::Register(DATA_REG_BOOL, flag),
        ];
        seeds.resize(MAX_PDA_SEEDS, Segment::Register(DATA_REG_PUBKEY, key));
        let derived = builder.derive_pda(program, &seeds);
        let matches = builder.binary(OP_EQ, derived, key);
        builder.require(matches);
        assert!(verify_builder(&builder).is_ok());
    }

    #[test]
    fn select_and_loop_register_typing() {
        // Select widens bytes to the larger branch; a CPI bytes segment must declare that width.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let condition = builder.const_bool(true);
        let short = builder.const_bytes(&[1, 2, 3]);
        let long = builder.const_bytes(&[1, 2, 3, 4, 5]);
        let selected = builder.select(condition, short, long);
        let cpi = builder.cpi(program, &[], &[Segment::Register(DATA_REG_BYTES, selected)]);
        builder.invoke(cpi, None);
        builder.set_cpi_max_data_len(cpi, 5);
        assert_eq!(verify_builder(&builder).unwrap().max_cpi_data_len, 5);
        builder.set_cpi_max_data_len(cpi, 3);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        // Select branch and condition typing.
        let mut builder = ProgramBuilder::new();
        let condition = builder.const_bool(true);
        let number = builder.const_u64(1);
        let flag = builder.const_bool(false);
        builder.select(condition, number, flag);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));
        let mut builder = ProgramBuilder::new();
        let number = builder.const_u64(1);
        builder.select(number, number, number);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));

        // Registers written inside the loop body are not visible after it.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(1, 0);
        let mut inner = NO_INDEX;
        builder.for_each(0, |body| {
            inner = body.const_u64(1);
        });
        let same = builder.binary(OP_EQ, inner, inner);
        builder.require(same);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::RegisterNotInitialized(inner))
        );

        // Registers written before the loop stay visible inside and after it.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(1, 0);
        let outer = builder.const_u64(1);
        builder.for_each(0, |body| {
            let same = body.binary(OP_EQ, outer, outer);
            body.require(same);
        });
        let same = builder.binary(OP_EQ, outer, outer);
        builder.require(same);
        assert!(verify_builder(&builder).is_ok());
    }

    #[test]
    fn data_segment_rules() {
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let signed = builder.const_i64(-1);
        let cpi = builder.cpi(program, &[], &[Segment::Register(DATA_REG_U8, signed)]);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let literal = builder.blob(&[1]);
        builder.derive_pda(program, &[Segment::Literal(literal)]);
        builder.segments_mut()[0].register = 0;
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidDataSegment(0))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let literal = builder.blob(&[1]);
        let cpi = builder.cpi(program, &[], &[Segment::Literal(literal)]);
        builder.invoke(cpi, None);
        builder.segments_mut()[0].reserved = [1, 0];
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidDataSegment(0))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let value = builder.const_u64(1);
        let cpi = builder.cpi(program, &[], &[Segment::Register(0xfe, value)]);
        builder.invoke(cpi, None);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidDataSegment(0))
        );

        // A register-backed PDA seed with a literal offset set is malformed.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let value = builder.const_u64(1);
        builder.derive_pda(program, &[Segment::Register(DATA_REG_U64, value)]);
        builder.segments_mut()[0].len_le = [1, 0];
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidDataSegment(0))
        );

        // Data above 4096 bytes is rejected even when declared honestly.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let literal = builder.blob(&[0; MAX_CPI_DATA_LEN + 1]);
        let cpi = builder.cpi(program, &[], &[Segment::Literal(literal)]);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));
    }

    #[test]
    fn blob_pubkey_account_and_input_ranges() {
        let mut builder = ProgramBuilder::new();
        builder.blob(&[0; 16]);
        builder.op(OP_CONST_U128, NO_INDEX, NO_INDEX, NO_INDEX, range_immediate(0, 15));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBlobRange));

        let mut builder = ProgramBuilder::new();
        builder.blob(&[0; 16]);
        builder.op(OP_CONST_U128, NO_INDEX, NO_INDEX, NO_INDEX, range_immediate(1, 16));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBlobRange));

        let mut builder = ProgramBuilder::new();
        builder.const_bytes(&[0; MAX_INPUT_BYTES + 1]);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBlobRange));

        let mut builder = ProgramBuilder::new();
        builder.op(OP_CONST_PUBKEY, 0, NO_INDEX, NO_INDEX, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );

        let mut builder = ProgramBuilder::new();
        builder.account(0, Some([1; 32]), None, 0);
        builder.const_bool(true);
        builder.accounts_mut().0[0].address_index = 5;
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidAccountConstraint(0))
        );
        builder.accounts_mut().0[0].address_index = 0;
        builder.accounts_mut().0[0].owner_index = 1;
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidAccountConstraint(0))
        );
        builder.accounts_mut().0[0].owner_index = NO_INDEX;
        builder.accounts_mut().0[0].flags = 0x10;
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidAccountConstraint(0))
        );
        builder.accounts_mut().0[0].flags = 0;
        builder.accounts_mut().0[0].reserved = 1;
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidAccountConstraint(0))
        );

        let mut builder = ProgramBuilder::new();
        builder.input(VALUE_BYTES, 0);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidInput(0)));
        let mut builder = ProgramBuilder::new();
        builder.input(VALUE_BYTES, MAX_INPUT_BYTES as u16 + 1);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidInput(0)));
        let mut builder = ProgramBuilder::new();
        builder.input(VALUE_U64, 1);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidInput(0)));
        let mut builder = ProgramBuilder::new();
        builder.input(VALUE_U64, 0);
        builder.input(0xfe, 0);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidInput(1)));
        let mut builder = ProgramBuilder::new();
        let input = builder.input(VALUE_BYTES, 8);
        let loaded = builder.load_input(input);
        let same = builder.binary(OP_EQ, loaded, loaded);
        builder.require(same);
        assert!(verify_builder(&builder).is_ok());
        let mut builder = ProgramBuilder::new();
        builder.load_input(3);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );
    }

    #[test]
    fn cpi_account_counts_are_bounded_even_when_never_invoked() {
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let target = builder.account(0, None, None, 0);
        let accounts = vec![(target, 0u8); MAX_CPI_ACCOUNTS + 1];
        let cpi = builder.cpi(program, &accounts, &[]);
        builder.invoke(cpi, None);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::TooManyCpiAccounts(0))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let target = builder.account(0, None, None, 0);
        let accounts = vec![(target, 0u8); MAX_CPI_ACCOUNTS];
        let cpi = builder.cpi(program, &accounts, &[]);
        builder.invoke(cpi, None);
        assert!(verify_builder(&builder).is_ok(), "exactly 64 accounts are allowed");

        // A descriptor that no instruction invokes still has to be well formed, because the
        // executor sizes its scratch buffers from every descriptor's declared data length.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let dead = builder.cpi(program, &[], &[]);
        builder.set_cpi_max_data_len(dead, u16::MAX);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let target = builder.account(0, None, None, 0);
        let accounts = vec![(target, 0u8); MAX_CPI_ACCOUNTS + 1];
        builder.cpi(program, &accounts, &[]);
        builder.const_bool(true);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::TooManyCpiAccounts(0))
        );

        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.cpi(program, &[], &[]);
        builder.cpis_mut()[0].account_start_le = 9u16.to_le_bytes();
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));
    }

    #[test]
    fn fixed_offset_reads_must_fit_the_declared_minimum_data_length() {
        let read_at = |offset: u64, min_data_len: u32| {
            let mut builder = ProgramBuilder::new();
            let account = builder.account(0, None, Some([2; 32]), min_data_len);
            let value = builder.read(OP_READ_U64, account, offset);
            let same = builder.binary(OP_EQ, value, value);
            builder.require(same);
            verify_builder(&builder)
        };
        assert!(read_at(56, 64).is_ok());
        assert_eq!(read_at(57, 64), Err(TemplateError::ReadOutOfBounds(0)));
        assert_eq!(read_at(0, 0), Err(TemplateError::ReadOutOfBounds(0)));
        assert_eq!(read_at(u64::MAX, 64), Err(TemplateError::ReadOutOfBounds(0)));

        // Every read width is checked against its own size.
        let widths = [
            (OP_READ_BOOL, 1u64),
            (OP_READ_U8, 1),
            (OP_READ_U16, 2),
            (OP_READ_U32, 4),
            (OP_READ_U64, 8),
            (OP_READ_I64, 8),
            (OP_READ_U128, 16),
            (OP_READ_PUBKEY, 32),
        ];
        for (opcode, width) in widths {
            let mut builder = ProgramBuilder::new();
            let account = builder.account(0, None, None, 40);
            let value = builder.read(opcode, account, 40 - width);
            let same = builder.binary(OP_EQ, value, value);
            builder.require(same);
            assert!(verify_builder(&builder).is_ok(), "opcode {opcode} at the boundary");
            let mut builder = ProgramBuilder::new();
            let account = builder.account(0, None, None, 40);
            let value = builder.read(opcode, account, 41 - width);
            let same = builder.binary(OP_EQ, value, value);
            builder.require(same);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::ReadOutOfBounds(0)),
                "opcode {opcode} one past the boundary"
            );
        }

        // Row accounts use the row constraint.
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 16);
        builder.batch(2, 0);
        builder.for_each(0, |body| {
            let value = body.read(OP_READ_U64, row, 8);
            let same = body.binary(OP_EQ, value, value);
            body.require(same);
        });
        assert!(verify_builder(&builder).is_ok());
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 16);
        builder.batch(2, 0);
        builder.for_each(0, |body| {
            let value = body.read(OP_READ_U64, row, 9);
            let same = body.binary(OP_EQ, value, value);
            body.require(same);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::ReadOutOfBounds(1))
        );
    }

    #[test]
    fn loop_carried_registers_must_be_initialized_and_keep_their_type() {
        // Sum each row's lamports into a carried register, then enforce a budget after the loop.
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let total = builder.const_u64(0);
        let budget = builder.const_u64(1_000);
        builder.for_each(1 << total, |body| {
            let lamports = body.account_lamports(row);
            let sum = body.binary(OP_ADD, total, lamports);
            body.mov(total, sum);
        });
        let within = builder.binary(OP_LTE, total, budget);
        builder.require(within);
        assert!(verify_builder(&builder).is_ok());

        // Never initialized before the loop.
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let total = builder.register();
        builder.for_each(1 << total, |body| {
            let lamports = body.account_lamports(row);
            body.mov(total, lamports);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidCarry(total))
        );

        // Retyped inside the body.
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let total = builder.const_u64(0);
        builder.for_each(1 << total, |body| {
            let key = body.account_key(row);
            body.mov(total, key);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidCarry(total))
        );

        // Bytes must keep the same maximum length.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let short = builder.const_bytes(&[1, 2]);
        builder.for_each(1 << short, |body| {
            let long = body.const_bytes(&[1, 2, 3]);
            body.mov(short, long);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidCarry(short))
        );

        // A carry bit above the register count.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let flag = builder.const_bool(true);
        builder.for_each(1 << 63, |body| body.require(flag));
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidCarry(63))
        );

        // Move follows ordinary register typing.
        let mut builder = ProgramBuilder::new();
        let value = builder.const_u64(1);
        let target = builder.register();
        builder.mov(target, value);
        let same = builder.binary(OP_EQ, target, value);
        builder.require(same);
        assert!(verify_builder(&builder).is_ok());
        let mut builder = ProgramBuilder::new();
        let target = builder.register();
        builder.mov(target, 5);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidRegister(5))
        );
    }

    #[test]
    fn minimum_iterations_are_bounded_by_the_maximum() {
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 3);
        let flag = builder.const_bool(true);
        builder.for_each(0, |body| body.require(flag));
        assert!(verify_builder(&builder).is_ok());
        builder.batch(3, 4);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidMinIterations)
        );

        let mut builder = ProgramBuilder::new();
        builder.batch(0, 1);
        builder.const_bool(true);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidMinIterations)
        );
    }

    #[test]
    fn dynamic_offset_reads_take_a_u64_register_and_skip_the_static_bound() {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(64);
        let value = builder.read_dynamic(OP_READ_U64, account, offset);
        let same = builder.binary(OP_EQ, value, value);
        builder.require(same);
        assert!(verify_builder(&builder).is_ok());

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_i64(64);
        builder.read_dynamic(OP_READ_U64, account, offset);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(64);
        builder.read_dynamic(OP_READ_U64, account, offset);
        builder.instructions_mut()[1].immediate_le = 8u64.to_le_bytes();
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidFlags(1)));

        let mut builder = ProgramBuilder::new();
        let value = builder.const_u64(1);
        builder.binary(OP_ADD, value, value);
        builder.instructions_mut()[1].flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidFlags(1)));

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 8);
        builder.read(OP_READ_U64, account, 0);
        builder.instructions_mut()[0].flags = 2;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidFlags(0)));
    }

    #[test]
    fn return_data_reads_must_directly_follow_an_unconditional_invoke() {
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.invoke(cpi, None);
        let size = builder.return_data(OP_READ_U64, 0);
        let expected = builder.const_u64(165);
        let same = builder.binary(OP_EQ, size, expected);
        builder.require(same);
        assert!(verify_builder(&builder).is_ok());

        // Every read width is accepted and typed like the matching account read.
        for (opcode, width) in [
            (OP_READ_BOOL, 1u64),
            (OP_READ_U8, 1),
            (OP_READ_U16, 2),
            (OP_READ_U32, 4),
            (OP_READ_U64, 8),
            (OP_READ_I64, 8),
            (OP_READ_U128, 16),
            (OP_READ_PUBKEY, 32),
        ] {
            let mut builder = ProgramBuilder::new();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let cpi = builder.cpi(program, &[], &[]);
            builder.invoke(cpi, None);
            let value = builder.return_data(opcode, MAX_RETURN_DATA_LEN as u64 - width);
            let same = builder.binary(OP_EQ, value, value);
            builder.require(same);
            assert!(verify_builder(&builder).is_ok(), "opcode {opcode} at the end");
            let mut builder = ProgramBuilder::new();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let cpi = builder.cpi(program, &[], &[]);
            builder.invoke(cpi, None);
            builder.return_data(opcode, MAX_RETURN_DATA_LEN as u64 - width + 1);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidReturnData(1)),
                "opcode {opcode} past the end"
            );
        }

        // Not a read opcode.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.invoke(cpi, None);
        builder.return_data(OP_ADD, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(1))
        );

        // Preceded by a guarded invoke.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        let guard = builder.const_bool(true);
        builder.invoke(cpi, Some(guard));
        builder.return_data(OP_READ_U64, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(2))
        );

        // Preceded by something other than an invoke, or by nothing at all.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.invoke(cpi, None);
        builder.const_bool(true);
        builder.return_data(OP_READ_U64, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(2))
        );
        let mut builder = ProgramBuilder::new();
        builder.return_data(OP_READ_U64, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(0))
        );

        // The invoke must be in the same range: first instruction of a loop body, or the first
        // root instruction after a loop whose body ends with an invoke.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.invoke(cpi, None);
        builder.for_each(0, |body| {
            body.return_data(OP_READ_U64, 0);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(2))
        );
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.for_each(0, |body| body.invoke(cpi, None));
        builder.return_data(OP_READ_U64, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(2))
        );

        // Inside a loop body right after an unconditional invoke is fine.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.for_each(0, |body| {
            body.invoke(cpi, None);
            let value = body.return_data(OP_READ_U64, 0);
            let same = body.binary(OP_EQ, value, value);
            body.require(same);
        });
        assert!(verify_builder(&builder).is_ok());
    }

    #[test]
    fn instruction_reserved_bytes_must_be_zero() {
        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        builder.instructions_mut()[0].reserved = [0, 1];
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );

        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        builder.instructions_mut()[0].flags = 1;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidFlags(0)));

        let mut builder = ProgramBuilder::new();
        builder.op(OP_CONST_BOOL, 2, NO_INDEX, NO_INDEX, 0);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );

        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        builder.instructions_mut()[0].dst = 7;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidRegister(7)));
    }

    const SYSTEM_TRANSFER_HEX: &str = "42564d3203030000010102010200020001000400000000000400ff000000000003ffff000000000002ffff000000000002000000010000ffff000000000000000000000029ff00ffff000000000000000000000000000000020200000c0000000103020200ff0000040000000400000000000000010101010101010101010101010101010101010101010101010101010101010102000000";
    const ATA_ASSERTION_HEX: &str = "42564d32030500000006070000000300000000000000000004ffff000000000004ffff000000000000ffff000000000000ffff000000000000ffff0000000000080004ffff0000000000000000000000080102ffff0000000000000000000000080201ffff0000000000000000000000080303ffff00000000000000000000002f0400ffff000000000003000000000017050004ff000000000000000000000028ff05ffff0000000000000000000000070100000000000007020000000000000703000000000000";

    #[test]
    fn typescript_system_transfer_fixture_is_zero_copy_and_valid() {
        let bytes = decode_hex(SYSTEM_TRANSFER_HEX);
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(program.header.as_bytes().as_ptr(), bytes.as_ptr());
        assert_eq!(program.instructions.len(), 2);
        assert_eq!(program.cpis.len(), 1);
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
    fn rejects_truncated_and_invalid_register_programs() {
        let mut bytes = decode_hex(SYSTEM_TRANSFER_HEX);
        assert_eq!(
            ProgramView::parse(&bytes[..bytes.len() - 1]).unwrap_err(),
            TemplateError::SectionLengthMismatch
        );

        // The first instruction destination register is byte 24 + (3 * 8) + 4 + 1.
        bytes[53] = 1;
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(program.verify(), Err(TemplateError::InvalidRegister(1)));
    }

    #[test]
    fn rejects_trailing_unknown_and_type_invalid_programs() {
        let mut trailing = decode_hex(SYSTEM_TRANSFER_HEX);
        trailing.push(0);
        assert_eq!(
            ProgramView::parse(&trailing).unwrap_err(),
            TemplateError::SectionLengthMismatch
        );

        let mut unknown = decode_hex(SYSTEM_TRANSFER_HEX);
        // Header (24), three account records (24), and one input record (4).
        unknown[52] = 0xfe;
        assert_eq!(
            ProgramView::parse(&unknown).unwrap().verify(),
            Err(TemplateError::InvalidInstruction(0))
        );

        let mut wrong_type = decode_hex(SYSTEM_TRANSFER_HEX);
        // Change the sole input from u64 to bool; its u64 CPI encoding must then fail.
        wrong_type[48] = VALUE_BOOL;
        assert_eq!(
            ProgramView::parse(&wrong_type).unwrap().verify(),
            Err(TemplateError::TypeMismatch)
        );
    }

    #[test]
    fn typescript_ata_assertion_fixture_verifies_pda_seeds() {
        let bytes = decode_hex(ATA_ASSERTION_HEX);
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(program.data_segments.len(), 3);
        assert_eq!(
            program.verify().unwrap(),
            VerificationStats {
                fixed_accounts: 5,
                batch_stride: 0,
                batch_max_iterations: 0,
                inputs: 0,
                registers: 6,
                instructions: 7,
                cpis: 0,
                max_expanded_cpis: 0,
                max_cpi_data_len: 0,
            }
        );

        let mut excessive_seeds = bytes.clone();
        // Header (24), five accounts (40), instruction four, then the high u32 of immediate.
        excessive_seeds[138] = 16;
        assert_eq!(
            ProgramView::parse(&excessive_seeds).unwrap().verify(),
            Err(TemplateError::InvalidInstruction(4))
        );

        let mut wrong_seed_type = bytes;
        // Data segments begin after the 24-byte header, five accounts, and seven instructions.
        wrong_seed_type[176] = DATA_REG_BYTES;
        assert_eq!(
            ProgramView::parse(&wrong_seed_type).unwrap().verify(),
            Err(TemplateError::TypeMismatch)
        );
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).unwrap();
                let low = (pair[1] as char).to_digit(16).unwrap();
                ((high << 4) | low) as u8
            })
            .collect()
    }
}
