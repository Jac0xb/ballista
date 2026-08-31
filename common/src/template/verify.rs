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

                let mut body_registers = registers;
                let (body_cpis, body_max_data) =
                    self.verify_range(body_start, body_end, true, &mut body_registers)?;
                root_cpis = root_cpis
                    .checked_add(
                        body_cpis
                            .checked_mul(header.batch_max_iterations())
                            .ok_or(TemplateError::CountOverflow)?,
                    )
                    .ok_or(TemplateError::CountOverflow)?;
                max_cpi_data_len = max_cpi_data_len.max(body_max_data);
                program_counter = body_end;
                continue;
            }

            let (cpis, data_len) =
                self.verify_instruction(instruction, program_counter, false, &mut registers)?;
            root_cpis += cpis;
            max_cpi_data_len = max_cpi_data_len.max(data_len);
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
        for index in start..end {
            let instruction = &self.instructions[index];
            self.verify_record_header(instruction, index)?;
            if instruction.opcode == OP_FOREACH {
                return Err(TemplateError::InvalidBatch);
            }
            let (instruction_cpis, data_len) =
                self.verify_instruction(instruction, index, in_loop, registers)?;
            cpis += instruction_cpis;
            max_data_len = max_data_len.max(data_len);
        }
        Ok((cpis, max_data_len))
    }

    fn verify_record_header(
        &self,
        instruction: &InstructionRecord,
        index: usize,
    ) -> Result<(), TemplateError> {
        if instruction.flags != 0 || instruction.reserved != [0; 2] {
            return Err(TemplateError::InvalidInstruction(index));
        }
        Ok(())
    }

    fn verify_instruction(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        in_loop: bool,
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
            OP_ACCOUNT_LAMPORTS | OP_ACCOUNT_DATA_LEN | OP_READ_U64 | OP_READ_U8 | OP_READ_U16
            | OP_READ_U32 => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_ACCOUNT_IS_EMPTY | OP_READ_BOOL => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_READ_I64 => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_I64))?;
            }
            OP_READ_U128 => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U128))?;
            }
            OP_READ_PUBKEY => {
                self.require_account(instruction.a, in_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
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

    fn verify_cpi(
        &self,
        index: usize,
        in_loop: bool,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<usize, TemplateError> {
        let descriptor = self
            .cpis
            .get(index)
            .ok_or(TemplateError::InvalidCpi(index))?;
        if descriptor.reserved0 != 0 || descriptor.reserved1 != [0; 2] {
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

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEM_TRANSFER_HEX: &str = "42564d3202030000010102010200020001000400000000000400ff000000000003ffff000000000002ffff000000000002000000010000ffff000000000000000000000029ff00ffff000000000000000000000000000000020200000c0000000103020200ff0000040000000400000000000000010101010101010101010101010101010101010101010101010101010101010102000000";

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
