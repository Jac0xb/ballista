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

/// The verifier's static knowledge about one register: its value type and, for `bytes`, the
/// maximum length it can hold. Public so formal specifications can state typing invariants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterInfo {
    pub value_type: u8,
    pub bytes_max_len: usize,
}

impl RegisterInfo {
    pub const fn scalar(value_type: u8) -> Self {
        Self {
            value_type,
            bytes_max_len: 0,
        }
    }

    pub const fn bytes(max_len: usize) -> Self {
        Self {
            value_type: VALUE_BYTES,
            bytes_max_len: max_len,
        }
    }

    pub const fn is_numeric(self) -> bool {
        matches!(self.value_type, VALUE_U64 | VALUE_I64 | VALUE_U128)
    }

    /// Whether this register holds one of the two types the bitwise, shift, and multiply-divide
    /// opcodes accept: `i64` is signed and excluded.
    pub const fn is_unsigned(self) -> bool {
        matches!(self.value_type, VALUE_U64 | VALUE_U128)
    }
}

/// Where an instruction sits, which decides what it may name. `LOOP_INDEX` needs a loop; row
/// accounts and row inputs need a loop over the batch rows. A count loop has an index but no
/// rows, so one flag cannot say both.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopScope {
    /// Outside every loop.
    Root,
    /// Inside a `FOREACH` body.
    Rows,
    /// Inside a `REPEAT` body.
    Count,
}

impl LoopScope {
    /// Whether `LOOP_INDEX` is allowed here.
    pub const fn in_loop(self) -> bool {
        !matches!(self, LoopScope::Root)
    }

    /// Whether row accounts and row inputs are allowed here.
    pub const fn in_row_loop(self) -> bool {
        matches!(self, LoopScope::Rows)
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
        if header.row_input_count() > MAX_ROW_INPUTS || header.total_input_count() > MAX_INPUTS {
            return Err(TemplateError::TooManyInputs);
        }
        // Row inputs are carried once per iteration, so they need a batch to iterate over.
        if header.row_input_count() != 0 && header.batch_stride() == 0 {
            return Err(TemplateError::InvalidBatch);
        }
        let input_values = header
            .input_count()
            .checked_add(
                header
                    .row_input_count()
                    .checked_mul(header.batch_max_iterations())
                    .ok_or(TemplateError::CountOverflow)?,
            )
            .ok_or(TemplateError::CountOverflow)?;
        if input_values > MAX_INPUT_VALUES {
            return Err(TemplateError::TooManyInputs);
        }
        if header.account_group_count() > MAX_ACCOUNT_GROUPS {
            return Err(TemplateError::TooManyAccountGroups);
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
        let mut loops = 0usize;
        let mut row_loops = 0usize;
        let mut max_cpi_data_len = 0usize;
        // The previous root-level instruction, if the previous instruction was not a loop body.
        let mut previous: Option<&InstructionRecord> = None;

        while program_counter < self.instructions.len() {
            let instruction = &self.instructions[program_counter];
            self.verify_record_header(instruction, program_counter)?;
            if matches!(instruction.opcode, OP_FOREACH | OP_REPEAT) {
                loops += 1;
                if loops > MAX_LOOPS {
                    return Err(TemplateError::InvalidLoop(program_counter));
                }
                // A FOREACH makes one pass per batch row. A REPEAT makes at most `c` passes,
                // counted by the u64 its register `b` holds when the loop starts.
                let (scope, max_passes) = if instruction.opcode == OP_FOREACH {
                    if header.batch_stride() == 0 || instruction.a == 0 {
                        return Err(TemplateError::InvalidBatch);
                    }
                    row_loops += 1;
                    (LoopScope::Rows, header.batch_max_iterations())
                } else {
                    // A REPEAT writes no register, so its destination is reserved: a stored
                    // template is never verified again, so a field accepted with any value now
                    // could never take a meaning later. FOREACH shipped without this check.
                    if instruction.a == 0 || instruction.c == 0 || instruction.dst != NO_INDEX {
                        return Err(TemplateError::InvalidLoop(program_counter));
                    }
                    self.require_type(&registers, instruction.b, VALUE_U64)?;
                    (LoopScope::Count, instruction.c as usize)
                };
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
                    self.verify_range(body_start, body_end, scope, &mut body_registers)?;
                verify_carry_after(carry, &registers, &body_registers)?;
                // The worst case runs every loop to its maximum.
                root_cpis = root_cpis
                    .checked_add(
                        body_cpis
                            .checked_mul(max_passes)
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
                LoopScope::Root,
                previous,
                &mut registers,
            )?;
            root_cpis += cpis;
            max_cpi_data_len = max_cpi_data_len.max(data_len);
            previous = Some(instruction);
            program_counter += 1;
        }

        // A batch is iterated by at least one FOREACH, and a FOREACH needs a batch to iterate.
        if (header.batch_stride() == 0) != (row_loops == 0) {
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

    /// Verifies one instruction record against a register typing state, exactly as the full
    /// verifier does inside `verify`. Exposed for formal specifications that state the
    /// per-instruction soundness property: whatever this accepts, the executor runs without a
    /// structural error and leaves `dst` holding the type recorded here.
    pub fn verify_single_instruction(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        scope: LoopScope,
        previous: Option<&InstructionRecord>,
        registers: &mut [Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(usize, usize), TemplateError> {
        self.verify_record_header(instruction, instruction_index)?;
        self.verify_instruction(instruction, instruction_index, scope, previous, registers)
    }

    /// Verifies a loop body. Loops never nest: `verify_instruction` rejects a loop instruction,
    /// since only `verify` starts one, at the root.
    fn verify_range(
        &self,
        start: usize,
        end: usize,
        scope: LoopScope,
        registers: &mut [Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(usize, usize), TemplateError> {
        let mut cpis = 0usize;
        let mut max_data_len = 0usize;
        let mut previous: Option<&InstructionRecord> = None;
        for index in start..end {
            let instruction = &self.instructions[index];
            self.verify_record_header(instruction, index)?;
            let (instruction_cpis, data_len) =
                self.verify_instruction(instruction, index, scope, previous, registers)?;
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
        scope: LoopScope,
        previous: Option<&InstructionRecord>,
        registers: &mut [Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(usize, usize), TemplateError> {
        let scalar = |value_type| RegisterInfo::scalar(value_type);
        // Row accounts and row inputs resolve only in a FOREACH body. A REPEAT body has an index
        // but no rows, and naming a row there has its own error.
        let in_row_loop = scope.in_row_loop();
        if scope == LoopScope::Count && self.names_row(instruction) {
            return Err(TemplateError::InvalidLoop(instruction_index));
        }
        match instruction.opcode {
            OP_LOAD_INPUT => {
                let input = self
                    .input_descriptor(instruction.a, in_row_loop)
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
                self.require_account(instruction.a, in_row_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
            }
            OP_ACCOUNT_LAMPORTS | OP_ACCOUNT_DATA_LEN => {
                self.require_account(instruction.a, in_row_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_ACCOUNT_IS_EMPTY => {
                self.require_account(instruction.a, in_row_loop)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_BOOL))?;
            }
            OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_U128
            | OP_READ_PUBKEY | OP_READ_BOOL | OP_READ_I32 => {
                self.refuse_entry_data(instruction, instruction_index)?;
                if instruction.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET != 0 {
                    self.require_account(instruction.a, in_row_loop)?;
                    self.require_type(registers, instruction.b, VALUE_U64)?;
                    if instruction.immediate() != 0 {
                        return Err(TemplateError::InvalidFlags(instruction_index));
                    }
                } else {
                    self.verify_read_bounds(instruction, instruction_index, in_row_loop)?;
                }
                self.write_register(
                    registers,
                    instruction.dst,
                    scalar(read_type(instruction.opcode)),
                )?;
            }
            OP_CLOCK_SLOT => self.write_register(registers, instruction.dst, scalar(VALUE_U64))?,
            OP_CLOCK_TIMESTAMP => {
                self.write_register(registers, instruction.dst, scalar(VALUE_I64))?
            }
            OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_REM | OP_MIN | OP_MAX => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left != right || !left.is_numeric() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, left)?;
            }
            OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left != right || !left.is_unsigned() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, left)?;
            }
            OP_SHL | OP_SHR => {
                let value = self.read_register(registers, instruction.a)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                if !value.is_unsigned() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, value)?;
            }
            OP_MUL_DIV | OP_MUL_DIV_CEIL => {
                let a = self.read_register(registers, instruction.a)?;
                let b = self.read_register(registers, instruction.b)?;
                let c = self.read_register(registers, instruction.c)?;
                if a != b || a != c || !a.is_unsigned() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, a)?;
            }
            OP_POW10 => {
                self.require_type(registers, instruction.a, VALUE_U64)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U128))?;
            }
            OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_INSTRUCTION_PROGRAM | OP_INSTRUCTION_ACCOUNT_COUNT | OP_INSTRUCTION_DATA_LEN => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                let value_type = if instruction.opcode == OP_INSTRUCTION_PROGRAM {
                    VALUE_PUBKEY
                } else {
                    VALUE_U64
                };
                self.write_register(registers, instruction.dst, scalar(value_type))?;
            }
            OP_INSTRUCTION_ACCOUNT | OP_INSTRUCTION_ACCOUNT_FLAGS => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.require_type(registers, instruction.c, VALUE_U64)?;
                let value_type = if instruction.opcode == OP_INSTRUCTION_ACCOUNT {
                    VALUE_PUBKEY
                } else {
                    VALUE_U64
                };
                self.write_register(registers, instruction.dst, scalar(value_type))?;
            }
            OP_READ_INSTRUCTION_DATA => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.require_type(registers, instruction.c, VALUE_U64)?;
                // The immediate names a read opcode, as `RETURN_DATA`'s `a` does. `read_type` falls
                // back to `u64` for any other opcode, so the selector must be a read first.
                let selector = u8::try_from(instruction.immediate())
                    .ok()
                    .filter(|selector| read_width(*selector) != 0)
                    .ok_or(TemplateError::InvalidInstruction(instruction_index))?;
                self.write_register(registers, instruction.dst, scalar(read_type(selector)))?;
            }
            OP_READ_INSTRUCTION_BYTES => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.require_type(registers, instruction.c, VALUE_U64)?;
                let len = byte_read_len(instruction, instruction_index)?;
                self.write_register(registers, instruction.dst, RegisterInfo::bytes(len))?;
            }
            OP_READ_ACCOUNT_BYTES => {
                self.refuse_entry_data(instruction, instruction_index)?;
                let account = self
                    .account_constraint(instruction.a, in_row_loop)
                    .ok_or(TemplateError::InvalidAccountConstraint(instruction.a as usize))?;
                // The run refuses a byte read of an account its instruction can write. A fixed
                // account declared writable is writable in every run, so such a read could never
                // succeed; a row account is left to that run-time check.
                if instruction.a & ITERATION_ACCOUNT_BIT == 0 && account.flags & ACCOUNT_WRITABLE != 0 {
                    return Err(TemplateError::InvalidInstruction(instruction_index));
                }
                self.require_type(registers, instruction.b, VALUE_U64)?;
                let len = byte_read_len(instruction, instruction_index)?;
                self.write_register(registers, instruction.dst, RegisterInfo::bytes(len))?;
            }
            OP_BYTES_LEN => {
                self.require_type(registers, instruction.a, VALUE_BYTES)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_OPEN_REGISTRY => {
                self.verify_open_registry(instruction, instruction_index, scope, registers)?;
                // Creating a pre-funded entry takes three CPIs; an open never encodes CPI data.
                return Ok((REGISTRY_OPEN_CPIS, 0));
            }
            OP_READ_REGISTRY => {
                if instruction.b != NO_INDEX || instruction.c != NO_INDEX {
                    return Err(TemplateError::InvalidRegistry(instruction_index));
                }
                let field = self.registry_field(instruction, instruction_index, instruction.a, false)?;
                self.write_register(registers, instruction.dst, scalar(read_type(field.selector)))?;
            }
            OP_WRITE_REGISTRY => {
                let invalid = TemplateError::InvalidRegistry(instruction_index);
                if instruction.dst != NO_INDEX || instruction.c != NO_INDEX {
                    return Err(invalid);
                }
                let field = self.registry_field(instruction, instruction_index, instruction.b, true)?;
                let value = self.read_register(registers, instruction.a).map_err(|_| invalid)?;
                if value.value_type != read_type(field.selector) {
                    return Err(invalid);
                }
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
                if !scope.in_loop() {
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
                self.write_register(registers, instruction.dst, scalar(read_type(instruction.a)))?;
            }
            OP_DERIVE_PDA => {
                self.verify_pda_seeds(instruction, instruction_index, in_row_loop, registers)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
            }
            OP_CREATE_PDA => {
                // The bump completes the seed list, so it is a plain u64 the template computed or
                // read from an input; the executor rejects one that does not fit in a byte.
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.verify_pda_seeds(instruction, instruction_index, in_row_loop, registers)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_PUBKEY))?;
            }
            OP_EMIT => {
                self.verify_output(instruction, instruction_index, registers)?;
                self.verify_emit_tag(instruction, instruction_index)?;
            }
            OP_SET_RETURN_DATA => {
                self.verify_output(instruction, instruction_index, registers)?;
                // The runtime clears return data whenever a program is invoked, CPIs included, so
                // what a run returns is set once, outside every loop, after its last invoke. Loops
                // run forward, so every instruction that can run later sits at a later index.
                // Checked after `verify_output`, as `EMIT`'s tag is, so a bad segment reports its
                // own error first.
                let later = self
                    .instructions
                    .get(instruction_index.saturating_add(1)..)
                    .unwrap_or(&[]);
                if scope.in_loop()
                    || later
                        .iter()
                        .any(|record| matches!(record.opcode, OP_INVOKE | OP_SET_RETURN_DATA))
                {
                    return Err(TemplateError::InvalidOutput(instruction_index));
                }
            }
            OP_REQUIRE => self.require_type(registers, instruction.a, VALUE_BOOL)?,
            OP_INVOKE => {
                if instruction.b != NO_INDEX {
                    self.require_type(registers, instruction.b, VALUE_BOOL)?;
                }
                let max_data = self.verify_cpi(instruction.a as usize, in_row_loop, registers)?;
                return Ok((1, max_data));
            }
            // `verify` starts every loop at the root, so a loop reaching here is inside a body, or
            // verified one instruction at a time. A FOREACH keeps the error it shipped with.
            OP_FOREACH => return Err(TemplateError::InvalidBatch),
            OP_REPEAT => return Err(TemplateError::InvalidLoop(instruction_index)),
            _ => return Err(TemplateError::InvalidInstruction(instruction_index)),
        }
        Ok((0, 0))
    }

    /// Shared by `DERIVE_PDA` and `CREATE_PDA`: the program account must be executable and the
    /// immediate must name a non-empty, in-bounds run of seed segments no longer than a seed.
    fn verify_pda_seeds(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        in_row_loop: bool,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(), TemplateError> {
        let program = self
            .account_constraint(instruction.a, in_row_loop)
            .ok_or(TemplateError::InvalidInstruction(instruction_index))?;
        if program.flags & ACCOUNT_EXECUTABLE == 0 {
            return Err(TemplateError::InvalidInstruction(instruction_index));
        }
        let (segment_start, segment_len) = instruction.blob_range();
        let segment_end = segment_start
            .checked_add(segment_len)
            .ok_or(TemplateError::CountOverflow)?;
        if segment_len == 0 || segment_len > MAX_PDA_SEEDS || segment_end > self.data_segments.len()
        {
            return Err(TemplateError::InvalidInstruction(instruction_index));
        }
        for (offset, segment) in self.data_segments[segment_start..segment_end]
            .iter()
            .enumerate()
        {
            let seed_len = self.verify_segment(segment_start + offset, segment, registers)?;
            if seed_len > MAX_PDA_SEED_LEN {
                return Err(TemplateError::InvalidDataSegment(segment_start + offset));
            }
        }
        Ok(())
    }

    /// Shared by `EMIT` and `SET_RETURN_DATA`. The record names no register, and its immediate
    /// names a non-empty, in-bounds run of data segments, encoded as invocation data is. Their
    /// widths, a `bytes` register counted at its maximum length, must sum to at most
    /// `MAX_RETURN_DATA_LEN`, the return-data limit, which also bounds a log line.
    fn verify_output(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(), TemplateError> {
        if [instruction.dst, instruction.a, instruction.b, instruction.c] != [NO_INDEX; 4] {
            return Err(TemplateError::InvalidInstruction(instruction_index));
        }
        let (segment_start, segment_len) = instruction.blob_range();
        let segment_end = segment_start
            .checked_add(segment_len)
            .ok_or(TemplateError::CountOverflow)?;
        if segment_len == 0 || segment_end > self.data_segments.len() {
            return Err(TemplateError::InvalidInstruction(instruction_index));
        }
        let mut max_len = 0usize;
        for (offset, segment) in self.data_segments[segment_start..segment_end]
            .iter()
            .enumerate()
        {
            let len = self.verify_segment(segment_start + offset, segment, registers)?;
            max_len = max_len
                .checked_add(len)
                .ok_or(TemplateError::CountOverflow)?;
        }
        if max_len > MAX_RETURN_DATA_LEN {
            return Err(TemplateError::InvalidOutput(instruction_index));
        }
        Ok(())
    }

    /// `OPEN_REGISTRY`, every rule `InvalidRegistry`: at the root; a valid [`RegistryOpen`] with a
    /// registry index below [`MAX_REGISTRIES`] and 1 to [`MAX_REGISTRY_SIZE`] field bytes; no
    /// destination; the entry a fixed account declared writable and nothing else; the payer a
    /// fixed account declared signer and writable; the System program account fixed and pinned to
    /// the System program; the key [`NO_INDEX`] or a set `pubkey` register. Against the opens
    /// before it: its entry account not opened already, the same size as any open of the same
    /// registry index, at most [`MAX_REGISTRY_OPENS`] in all, and no `SET_RETURN_DATA` before it,
    /// since creating an entry calls the System program and a CPI clears return data. Against the
    /// whole program: no CPI lists the entry account writable.
    ///
    /// The other records are found by scanning, as `SET_RETURN_DATA` scans the records after it:
    /// the per-instruction signature Certora verifies against has no room for a slot table. A
    /// template that opens no entry never scans its CPIs.
    fn verify_open_registry(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        scope: LoopScope,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(), TemplateError> {
        let invalid = TemplateError::InvalidRegistry(instruction_index);
        let open = RegistryOpen::decode(instruction.immediate()).ok_or(invalid)?;
        if scope != LoopScope::Root
            || instruction.dst != NO_INDEX
            || usize::from(open.index) >= MAX_REGISTRIES
            || !(1..=MAX_REGISTRY_SIZE).contains(&usize::from(open.size))
        {
            return Err(invalid);
        }
        // `in_row_loop` is false, so a row account never matches. An entry is an account Ballista
        // owns at an address the key picks, never a signer or a program, owned by the System
        // program and empty until its open creates it: a signer or executable flag, an address or
        // owner pin, or a data-length floor would fail the run's account checks or the creation.
        let entry = self.account_constraint(instruction.a, false).ok_or(invalid)?;
        if entry.flags != ACCOUNT_WRITABLE
            || entry.address_index != NO_INDEX
            || entry.owner_index != NO_INDEX
            || entry.min_data_len() != 0
        {
            return Err(invalid);
        }
        let payer = self.account_constraint(instruction.c, false).ok_or(invalid)?;
        let signer_and_writable = ACCOUNT_SIGNER | ACCOUNT_WRITABLE;
        if payer.flags & signer_and_writable != signer_and_writable {
            return Err(invalid);
        }
        let system_program = self
            .account_constraint(open.system_program, false)
            .filter(|constraint| constraint.address_index != NO_INDEX)
            .and_then(|constraint| self.pubkeys.get(constraint.address_index as usize))
            .is_some_and(|address| address.bytes == SYSTEM_PROGRAM_ADDRESS);
        if !system_program {
            return Err(invalid);
        }
        if instruction.b != NO_INDEX
            && !matches!(
                self.read_register(registers, instruction.b),
                Ok(info) if info.value_type == VALUE_PUBKEY
            )
        {
            return Err(invalid);
        }
        let mut opens = 0usize;
        for record in self.instructions.get(..instruction_index).unwrap_or(&[]) {
            match record.opcode {
                OP_SET_RETURN_DATA => return Err(invalid),
                OP_OPEN_REGISTRY => {
                    opens += 1;
                    let earlier = RegistryOpen::decode(record.immediate()).ok_or(invalid)?;
                    if record.a == instruction.a
                        || (earlier.index == open.index && earlier.size != open.size)
                    {
                        return Err(invalid);
                    }
                }
                _ => {}
            }
        }
        if opens >= MAX_REGISTRY_OPENS {
            return Err(invalid);
        }
        // No CPI lists the entry writable, wherever its invoke sits. After the open, the entry's
        // borrow mark fails every such CPI with `RegistryReentry`. Before it, no template needs
        // one: only Ballista writes an entry, through its fields once it is open, and an entry
        // holds its rent and nothing else. The CPI account records are scanned directly, a few
        // bytes against a pass over every instruction to find the invokes, so a CPI that nothing
        // invokes is refused as well. A row account or a group member that turns out to be the
        // entry is left to the run's check.
        if self
            .cpi_accounts
            .iter()
            .any(|meta| meta.account == instruction.a && meta.flags & ACCOUNT_WRITABLE != 0)
        {
            return Err(invalid);
        }
        Ok(())
    }

    /// The field a `READ_REGISTRY` or `WRITE_REGISTRY` names, every rule `InvalidRegistry`: a
    /// valid [`RegistryField`] whose selector is a read opcode (for a write, one of the five whose
    /// width holds every value of its type), inside the size the open of `entry` at a lower pc
    /// declared. Opens are only at the root and loops run forward, so that open has run by the
    /// time this instruction does, wherever it sits.
    fn registry_field(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        entry: u8,
        write: bool,
    ) -> Result<RegistryField, TemplateError> {
        let invalid = TemplateError::InvalidRegistry(instruction_index);
        let field = RegistryField::decode(instruction.immediate()).ok_or(invalid)?;
        let width = read_width(field.selector);
        let holds_its_type = matches!(
            field.selector,
            OP_READ_BOOL | OP_READ_U64 | OP_READ_I64 | OP_READ_U128 | OP_READ_PUBKEY
        );
        if width == 0 || (write && !holds_its_type) {
            return Err(invalid);
        }
        let size = self
            .instructions
            .get(..instruction_index)
            .unwrap_or(&[])
            .iter()
            .find(|record| record.opcode == OP_OPEN_REGISTRY && record.a == entry)
            .and_then(|record| RegistryOpen::decode(record.immediate()))
            .ok_or(invalid)?
            .size;
        if !valid_range(usize::from(size), usize::from(field.offset), width) {
            return Err(invalid);
        }
        Ok(field)
    }

    /// Refuses an account-data read, a read opcode or `READ_ACCOUNT_BYTES`, of an account any
    /// `OPEN_REGISTRY` names, wherever either sits: an entry's fields are read with
    /// `READ_REGISTRY`, which needs the open first. Before the open, the entry is not yet marked,
    /// so a CPI between such a read and the open could change the entry, and a write based on the
    /// read would lose that update. After the open, the read would fail on the mark. The entry's
    /// key, owner, lamports and data length stay readable, since none of them is a field.
    ///
    /// An open accepts only a fixed account declared writable, so the scan for one runs only for
    /// such an account: most data reads name an account the template cannot write, and scanning
    /// the program for each would make a large template's upload quadratic. A read of any other
    /// account an open names still fails, at that open.
    fn refuse_entry_data(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
    ) -> Result<(), TemplateError> {
        let may_be_entry = self
            .account_constraint(instruction.a, false)
            .is_some_and(|account| account.flags & ACCOUNT_WRITABLE != 0);
        let opened = may_be_entry
            && self
                .instructions
                .iter()
                .any(|record| record.opcode == OP_OPEN_REGISTRY && record.a == instruction.a);
        if opened {
            return Err(TemplateError::InvalidRegistry(instruction_index));
        }
        Ok(())
    }

    /// An `EMIT` starts with a literal tag of at least `MIN_EMIT_TAG_LEN` bytes outside the run
    /// event's family, `RUN_EVENT_TAG_FAMILY`. A log line names the program that wrote it but not
    /// the template, so an untagged line could be a byte-exact run event for any template address.
    /// Checked after `verify_output`, so a bad segment reports its own error first.
    fn verify_emit_tag(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
    ) -> Result<(), TemplateError> {
        let (segment_start, _) = instruction.blob_range();
        let tag = self
            .data_segments
            .get(segment_start)
            .filter(|segment| segment.kind == DATA_LITERAL)
            .and_then(|segment| {
                self.blob
                    .get(segment.offset()..segment.offset() + segment.len())
            });
        match tag {
            Some(tag)
                if tag.len() >= MIN_EMIT_TAG_LEN && !tag.starts_with(&RUN_EVENT_TAG_FAMILY) =>
            {
                Ok(())
            }
            _ => Err(TemplateError::InvalidOutput(instruction_index)),
        }
    }

    /// Checks one data segment of a PDA seed or an output and returns the most bytes it can
    /// encode: a literal's length, a register kind's width, or a `bytes` register's maximum
    /// length. Invocation data applies the same widths in `verify_cpi`.
    ///
    /// Inlined into both callers. With the outputs as a second caller, the compiler kept it out of
    /// line, and creating a cookbook template that derives a PDA cost about 80 compute units more.
    #[inline(always)]
    fn verify_segment(
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
        in_row_loop: bool,
    ) -> Result<(), TemplateError> {
        let constraint = self
            .account_constraint(instruction.a, in_row_loop)
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
        if descriptor.reserved1 != [0; 2] {
            return Err(TemplateError::InvalidCpi(index));
        }
        if let Some(group) = descriptor.account_group() {
            if group >= self.header.account_group_count() {
                return Err(TemplateError::InvalidCpi(index));
            }
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
        in_row_loop: bool,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<usize, TemplateError> {
        let descriptor = self.verify_cpi_shape(index)?;
        let account_end = descriptor.account_start() + descriptor.account_len as usize;
        let segment_end = descriptor.segment_start() + descriptor.segment_len as usize;
        let program = self
            .account_constraint(descriptor.program_account, in_row_loop)
            .ok_or(TemplateError::InvalidCpi(index))?;
        if program.flags & ACCOUNT_EXECUTABLE == 0 {
            return Err(TemplateError::InvalidCpi(index));
        }

        for cpi_account in &self.cpi_accounts[descriptor.account_start()..account_end] {
            if cpi_account.flags & !(ACCOUNT_SIGNER | ACCOUNT_WRITABLE) != 0 {
                return Err(TemplateError::InvalidCpi(index));
            }
            let constraint = self
                .account_constraint(cpi_account.account, in_row_loop)
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

    /// Introspection reads the Instructions sysvar, so `a` must be a fixed account pinned to its
    /// address. The executor borrows that account's data for the whole run on the strength of it.
    fn require_instructions_sysvar(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
    ) -> Result<(), TemplateError> {
        // `in_row_loop` is false, so a row account never matches, even inside a loop body.
        let pinned = self
            .account_constraint(instruction.a, false)
            .filter(|constraint| constraint.address_index != NO_INDEX)
            .and_then(|constraint| self.pubkeys.get(constraint.address_index as usize))
            .is_some_and(|address| address.bytes == INSTRUCTIONS_SYSVAR_ID);
        if !pinned {
            return Err(TemplateError::InvalidIntrospection(instruction_index));
        }
        Ok(())
    }

    fn require_account(&self, reference: u8, in_row_loop: bool) -> Result<(), TemplateError> {
        self.account_constraint(reference, in_row_loop)
            .map(|_| ())
            .ok_or(TemplateError::InvalidAccountConstraint(reference as usize))
    }

    /// Whether `instruction` names a row account or a row input, which only a FOREACH body has.
    /// An operand this does not list still cannot name a row outside a FOREACH: the lookup
    /// rejects it, only with that operand's own error instead of `InvalidLoop`.
    ///
    /// One check on each instruction in a `REPEAT` body, rather than a scope on every account
    /// lookup: in the loops prototype, threading the scope through `require_account`,
    /// `verify_read_bounds`, `verify_pda_seeds` and `verify_cpi` cost 56 compute units on
    /// `create template, payroll 30 rows`, and this check 24.
    ///
    /// The opcodes that read the Instructions sysvar are left out on purpose: their account must
    /// be a fixed one, so a row account there is `InvalidIntrospection` in every scope, a `REPEAT`
    /// body included.
    fn names_row(&self, instruction: &InstructionRecord) -> bool {
        let row_account = |reference: u8| reference & ITERATION_ACCOUNT_BIT != 0;
        match instruction.opcode {
            OP_LOAD_INPUT => instruction.a & ITERATION_INPUT_BIT != 0,
            OP_ACCOUNT_KEY | OP_ACCOUNT_OWNER | OP_ACCOUNT_LAMPORTS | OP_ACCOUNT_DATA_LEN
            | OP_ACCOUNT_IS_EMPTY | OP_DERIVE_PDA | OP_CREATE_PDA | OP_READ_ACCOUNT_BYTES => {
                row_account(instruction.a)
            }
            OP_INVOKE => self.cpis.get(instruction.a as usize).is_some_and(|descriptor| {
                let start = descriptor.account_start();
                row_account(descriptor.program_account)
                    || self
                        .cpi_accounts
                        .get(start..start + descriptor.account_len as usize)
                        .is_some_and(|records| records.iter().any(|record| row_account(record.account)))
            }),
            opcode => read_width(opcode) != 0 && row_account(instruction.a),
        }
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

/// The length a byte read's immediate names: 1 to `MAX_INPUT_BYTES`, the bound on every `bytes`
/// value, so the result fits wherever a `bytes` input would.
fn byte_read_len(
    instruction: &InstructionRecord,
    instruction_index: usize,
) -> Result<usize, TemplateError> {
    usize::try_from(instruction.immediate())
        .ok()
        .filter(|len| (1..=MAX_INPUT_BYTES).contains(len))
        .ok_or(TemplateError::InvalidInstruction(instruction_index))
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
        OP_READ_U32 | OP_READ_I32 => 4,
        OP_READ_U64 | OP_READ_I64 => 8,
        OP_READ_U128 => 16,
        OP_READ_PUBKEY => 32,
        _ => 0,
    }
}

/// Register type produced by each `OP_READ_*` opcode; also the result type `OP_RETURN_DATA`
/// selects when its width operand names one of them. Any other opcode falls back to `VALUE_U64`,
/// so callers must first confirm the opcode is actually a read (or that `read_width` is nonzero),
/// as every call site does.
pub const fn read_type(opcode: u8) -> u8 {
    match opcode {
        OP_READ_I64 | OP_READ_I32 => VALUE_I64,
        OP_READ_U128 => VALUE_U128,
        OP_READ_PUBKEY => VALUE_PUBKEY,
        OP_READ_BOOL => VALUE_BOOL,
        _ => VALUE_U64,
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

        // Eight row accounts times sixteen iterations is 128 runtime slots, past the 120 cap.
        let mut builder = ProgramBuilder::new();
        for _ in 0..MAX_BATCH_STRIDE {
            builder.row_account(0, None, None, 0);
        }
        builder.batch(16, 0);
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
            (OP_REM, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_REM, Some(VALUE_I64), Some(VALUE_I64), Ok(())),
            (OP_REM, Some(VALUE_U128), Some(VALUE_U128), Ok(())),
            (OP_REM, Some(VALUE_U64), Some(VALUE_I64), Err(TemplateError::TypeMismatch)),
            (OP_REM, Some(VALUE_BOOL), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_SHL, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_SHR, Some(VALUE_U128), Some(VALUE_U64), Ok(())),
            (OP_SHL, Some(VALUE_I64), Some(VALUE_U64), Err(TemplateError::TypeMismatch)),
            (OP_SHR, Some(VALUE_U64), Some(VALUE_U128), Err(TemplateError::TypeMismatch)),
            (OP_SHL, Some(VALUE_U64), None, Err(TemplateError::RegisterNotInitialized(1))),
            (OP_BIT_AND, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_BIT_OR, Some(VALUE_U128), Some(VALUE_U128), Ok(())),
            (OP_BIT_XOR, Some(VALUE_I64), Some(VALUE_I64), Err(TemplateError::TypeMismatch)),
            (OP_BIT_AND, Some(VALUE_U64), Some(VALUE_U128), Err(TemplateError::TypeMismatch)),
            (OP_BIT_OR, Some(VALUE_BOOL), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_POW10, Some(VALUE_U64), None, Ok(())),
            (OP_POW10, Some(VALUE_I64), None, Err(TemplateError::TypeMismatch)),
            (OP_POW10, Some(VALUE_U128), None, Err(TemplateError::TypeMismatch)),
            (OP_POW10, None, None, Err(TemplateError::RegisterNotInitialized(0))),
            // An output names its parts in the immediate and no register at all.
            (OP_EMIT, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),
            (
                OP_SET_RETURN_DATA,
                Some(VALUE_U64),
                Some(VALUE_U64),
                Err(TemplateError::InvalidInstruction(2)),
            ),
            (OP_BYTES_LEN, Some(VALUE_BYTES), None, Ok(())),
            (OP_BYTES_LEN, Some(VALUE_U64), None, Err(TemplateError::TypeMismatch)),
            (OP_BYTES_LEN, Some(VALUE_PUBKEY), None, Err(TemplateError::TypeMismatch)),
            (OP_BYTES_LEN, None, None, Err(TemplateError::RegisterNotInitialized(0))),
            (OP_WRITE_REGISTRY + 1, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),
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

    /// `(opcode, operand types for a/b/c, expected outcome)`, for
    /// [`multiply_divide_takes_three_matching_unsigned_operands`].
    type MulDivTypeCase = (u8, [Option<u8>; 3], Result<(), TemplateError>);

    #[test]
    fn multiply_divide_takes_three_matching_unsigned_operands() {
        let cases: &[MulDivTypeCase] = &[
            (OP_MUL_DIV, [Some(VALUE_U64); 3], Ok(())),
            (OP_MUL_DIV_CEIL, [Some(VALUE_U128); 3], Ok(())),
            (OP_MUL_DIV, [Some(VALUE_I64); 3], Err(TemplateError::TypeMismatch)),
            (
                OP_MUL_DIV,
                [Some(VALUE_U64), Some(VALUE_U64), Some(VALUE_U128)],
                Err(TemplateError::TypeMismatch),
            ),
            (
                OP_MUL_DIV_CEIL,
                [Some(VALUE_U128), Some(VALUE_U64), Some(VALUE_U128)],
                Err(TemplateError::TypeMismatch),
            ),
            (
                OP_MUL_DIV,
                [Some(VALUE_U64), Some(VALUE_U64), None],
                Err(TemplateError::RegisterNotInitialized(2)),
            ),
        ];
        for (opcode, types, expected) in cases {
            let mut builder = ProgramBuilder::new();
            let a = typed_register(&mut builder, types[0]);
            let b = typed_register(&mut builder, types[1]);
            let c = typed_register(&mut builder, types[2]);
            builder.op(*opcode, a, b, c, 0);
            let outcome = verify_builder(&builder).map(|_| ());
            assert_eq!(&outcome, expected, "opcode {opcode} with {types:?}");
        }
    }

    #[test]
    fn destination_types() {
        // Every new math opcode pins its destination's type: a witness of that type must be
        // accepted, and a witness of either other numeric type must be rejected.
        let cases: &[(u8, [u8; 3], u8)] = &[
            (OP_MUL_DIV, [VALUE_U64; 3], VALUE_U64),
            (OP_MUL_DIV_CEIL, [VALUE_U128; 3], VALUE_U128),
            (OP_REM, [VALUE_U64, VALUE_U64, VALUE_U64], VALUE_U64),
            (OP_REM, [VALUE_I64, VALUE_I64, VALUE_U64], VALUE_I64),
            (OP_SHL, [VALUE_U128, VALUE_U64, VALUE_U64], VALUE_U128),
            (OP_SHR, [VALUE_U64, VALUE_U64, VALUE_U64], VALUE_U64),
            (OP_BIT_AND, [VALUE_U64, VALUE_U64, VALUE_U64], VALUE_U64),
            (OP_BIT_XOR, [VALUE_U128, VALUE_U128, VALUE_U64], VALUE_U128),
            (OP_POW10, [VALUE_U64, VALUE_U64, VALUE_U64], VALUE_U128),
        ];
        for (opcode, types, expected) in cases {
            for witness in [VALUE_U64, VALUE_I64, VALUE_U128] {
                let mut builder = ProgramBuilder::new();
                let a = typed_register(&mut builder, Some(types[0]));
                let b = typed_register(&mut builder, Some(types[1]));
                let c = typed_register(&mut builder, Some(types[2]));
                let result = builder.op(*opcode, a, b, c, 0);
                let other = typed_register(&mut builder, Some(witness));
                builder.binary(OP_EQ, result, other);
                let outcome = verify_builder(&builder).map(|_| ());
                if witness == *expected {
                    assert_eq!(outcome, Ok(()), "opcode {opcode} dst should be {expected}");
                } else {
                    assert_eq!(
                        outcome,
                        Err(TemplateError::TypeMismatch),
                        "opcode {opcode} dst vs {witness}"
                    );
                }
            }
        }
    }

    #[test]
    fn math_ops_reject_flags() {
        // None of these are read opcodes, so the dynamic-offset flag is never theirs to claim.
        for opcode in [
            OP_MUL_DIV, OP_MUL_DIV_CEIL, OP_REM, OP_SHL, OP_SHR, OP_BIT_AND, OP_BIT_OR,
            OP_BIT_XOR, OP_POW10,
        ] {
            let mut builder = ProgramBuilder::new();
            let a = builder.const_u64(1);
            let b = builder.const_u64(1);
            let c = builder.const_u64(1);
            builder.op(opcode, a, b, c, 0);
            assert!(verify_builder(&builder).is_ok(), "opcode {opcode} unflagged");
            builder.instructions_mut()[3].flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET;
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidFlags(3)),
                "opcode {opcode}"
            );
        }
    }

    #[test]
    fn i32_reads_are_typed_i64() {
        // A fixed-offset i32 read is typed i64: it adds to an i64 and not to a u64.
        let mut builder = ProgramBuilder::new();
        let feed = builder.account(0, None, None, 8);
        let exponent = builder.read(OP_READ_I32, feed, 4);
        let one = builder.const_i64(1);
        builder.binary(OP_ADD, exponent, one);
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()));

        let mut builder = ProgramBuilder::new();
        let feed = builder.account(0, None, None, 8);
        let exponent = builder.read(OP_READ_I32, feed, 4);
        let one = builder.const_u64(1);
        builder.binary(OP_ADD, exponent, one);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(TemplateError::TypeMismatch));

        // The four bytes must fit the declared minimum data length, as for every fixed read.
        let mut builder = ProgramBuilder::new();
        let feed = builder.account(0, None, None, 8);
        builder.read(OP_READ_I32, feed, 5);
        assert!(matches!(verify_builder(&builder), Err(TemplateError::ReadOutOfBounds(_))));

        // As a `RETURN_DATA` selector this opcode picks a four-byte width; that path's typing is
        // checked by `return_data_i32_selector_is_i64`.
        assert_eq!(read_width(OP_READ_I32), 4);
    }

    #[test]
    fn dynamic_i32_read() {
        // A dynamic-offset i32 read is typed i64 too, and still needs a u64 offset register and
        // a zero immediate.
        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(4);
        let value = builder.read_dynamic(OP_READ_I32, account, offset);
        let one = builder.const_i64(1);
        builder.binary(OP_ADD, value, one);
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()));

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_i64(4);
        builder.read_dynamic(OP_READ_I32, account, offset);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(4);
        builder.read_dynamic(OP_READ_I32, account, offset);
        builder.instructions_mut()[1].immediate_le = 8u64.to_le_bytes();
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidFlags(1)));
    }

    #[test]
    fn return_data_i32_selector_is_i64() {
        for (witness, expected) in [
            (VALUE_I64, Ok(())),
            (VALUE_U64, Err(TemplateError::TypeMismatch)),
        ] {
            let mut builder = ProgramBuilder::new();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let cpi = builder.cpi(program, &[], &[]);
            builder.invoke(cpi, None);
            let value = builder.return_data(OP_READ_I32, MAX_RETURN_DATA_LEN as u64 - 4);
            let other = typed_register(&mut builder, Some(witness));
            builder.binary(OP_ADD, value, other);
            assert_eq!(verify_builder(&builder).map(|_| ()), expected, "witness {witness}");
        }

        // The four bytes must still fit inside the return-data buffer, as for any width.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        builder.invoke(cpi, None);
        builder.return_data(OP_READ_I32, MAX_RETURN_DATA_LEN as u64 - 3);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidReturnData(1))
        );
    }

    /// A builder whose fixed account 0 is pinned to the Instructions sysvar.
    fn with_sysvar() -> (ProgramBuilder, u8) {
        let mut builder = ProgramBuilder::new();
        let sysvar = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
        (builder, sysvar)
    }

    /// A valid use of `opcode`, one of `OP_INSTRUCTION_COUNT` to `OP_BYTES_LEN`, as the last
    /// instruction of its program. Returns the builder and the result register.
    fn introspection_program(opcode: u8) -> (ProgramBuilder, u8) {
        let (mut builder, sysvar) = with_sysvar();
        let zero = builder.const_u64(0);
        let result = match opcode {
            OP_READ_INSTRUCTION_DATA => {
                builder.read_instruction_data(OP_READ_U64, sysvar, zero, zero)
            }
            OP_READ_INSTRUCTION_BYTES => builder.read_instruction_bytes(sysvar, zero, zero, 8),
            OP_READ_ACCOUNT_BYTES => builder.read_account_bytes(sysvar, zero, 8),
            OP_BYTES_LEN => {
                let bytes = builder.const_bytes(&[1, 2]);
                builder.bytes_len(bytes)
            }
            _ => builder.introspect(opcode, sysvar, zero, zero),
        };
        (builder, result)
    }

    #[test]
    fn introspection_and_byte_opcodes_type_their_results() {
        let cases = [
            (OP_INSTRUCTION_COUNT, VALUE_U64),
            (OP_INSTRUCTION_INDEX, VALUE_U64),
            (OP_INSTRUCTION_PROGRAM, VALUE_PUBKEY),
            (OP_INSTRUCTION_ACCOUNT_COUNT, VALUE_U64),
            (OP_INSTRUCTION_ACCOUNT, VALUE_PUBKEY),
            (OP_INSTRUCTION_ACCOUNT_FLAGS, VALUE_U64),
            (OP_INSTRUCTION_DATA_LEN, VALUE_U64),
            (OP_READ_INSTRUCTION_DATA, VALUE_U64),
            (OP_READ_INSTRUCTION_BYTES, VALUE_BYTES),
            (OP_READ_ACCOUNT_BYTES, VALUE_BYTES),
            (OP_BYTES_LEN, VALUE_U64),
        ];
        for (opcode, expected) in cases {
            for witness in [VALUE_U64, VALUE_PUBKEY, VALUE_BYTES] {
                let (mut builder, result) = introspection_program(opcode);
                let other = typed_register(&mut builder, Some(witness));
                builder.binary(OP_EQ, result, other);
                let outcome = verify_builder(&builder).map(|_| ());
                if witness == expected {
                    assert_eq!(outcome, Ok(()), "opcode {opcode}");
                } else {
                    assert_eq!(
                        outcome,
                        Err(TemplateError::TypeMismatch),
                        "opcode {opcode} vs {witness}"
                    );
                }
            }
        }
    }

    #[test]
    fn introspection_needs_a_fixed_account_pinned_to_the_instructions_sysvar() {
        for opcode in OP_INSTRUCTION_COUNT..=OP_READ_INSTRUCTION_BYTES {
            let immediate = match opcode {
                OP_READ_INSTRUCTION_DATA => OP_READ_U64 as u64,
                OP_READ_INSTRUCTION_BYTES => 8,
                _ => 0,
            };
            // Unpinned but owned by the sysvar's address, and pinned to another address.
            for address in [None, Some([7; 32])] {
                let mut builder = ProgramBuilder::new();
                let account = builder.account(0, address, Some(INSTRUCTIONS_SYSVAR_ID), 0);
                let zero = builder.const_u64(0);
                builder.op(opcode, account, zero, zero, immediate);
                assert_eq!(
                    verify_builder(&builder),
                    Err(TemplateError::InvalidIntrospection(1)),
                    "opcode {opcode} with address {address:?}"
                );
            }
            // An account the schema does not declare.
            let (mut builder, _) = with_sysvar();
            let zero = builder.const_u64(0);
            builder.op(opcode, 1, zero, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidIntrospection(1)),
                "opcode {opcode} with an undeclared account"
            );
            // A row account pinned to the sysvar is still not a fixed account.
            let mut builder = ProgramBuilder::new();
            let row = builder.row_account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
            builder.batch(1, 0);
            let zero = builder.const_u64(0);
            builder.for_each(0, |body| {
                body.op(opcode, row, zero, zero, immediate);
            });
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidIntrospection(2)),
                "opcode {opcode} on a row account"
            );
            // In a REPEAT body, which has no rows, it is the same error: the verifier's row check
            // for count loops leaves these opcodes to their own.
            let mut builder = ProgramBuilder::new();
            let row = builder.row_account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
            builder.batch(1, 0);
            let zero = builder.const_u64(0);
            builder.repeat(zero, 1, 0, |body| {
                body.op(opcode, row, zero, zero, immediate);
            });
            builder.for_each(0, |body| {
                body.loop_index();
            });
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidIntrospection(2)),
                "opcode {opcode} on a row account in a REPEAT"
            );
            // The pinned fixed account works inside a loop body too.
            let mut builder = ProgramBuilder::new();
            let sysvar = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
            builder.row_account(0, None, None, 0);
            builder.batch(1, 0);
            let zero = builder.const_u64(0);
            builder.for_each(0, |body| {
                body.op(opcode, sysvar, zero, zero, immediate);
            });
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "opcode {opcode} in a loop");
        }
    }

    #[test]
    fn introspection_indexes_positions_and_offsets_are_u64_registers() {
        let indexed = [
            (OP_INSTRUCTION_PROGRAM, 0),
            (OP_INSTRUCTION_ACCOUNT_COUNT, 0),
            (OP_INSTRUCTION_ACCOUNT, 0),
            (OP_INSTRUCTION_ACCOUNT_FLAGS, 0),
            (OP_INSTRUCTION_DATA_LEN, 0),
            (OP_READ_INSTRUCTION_DATA, OP_READ_U8 as u64),
            (OP_READ_INSTRUCTION_BYTES, 1),
        ];
        for (opcode, immediate) in indexed {
            let (mut builder, sysvar) = with_sysvar();
            let index = builder.const_i64(0);
            let zero = builder.const_u64(0);
            builder.op(opcode, sysvar, index, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::TypeMismatch),
                "opcode {opcode} index"
            );

            let (mut builder, sysvar) = with_sysvar();
            let index = builder.register();
            let zero = builder.const_u64(0);
            builder.op(opcode, sysvar, index, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::RegisterNotInitialized(index)),
                "opcode {opcode} unset index"
            );
        }
        // The account opcodes take a position in `c`, the data reads an offset.
        let positioned = [
            (OP_INSTRUCTION_ACCOUNT, 0),
            (OP_INSTRUCTION_ACCOUNT_FLAGS, 0),
            (OP_READ_INSTRUCTION_DATA, OP_READ_U8 as u64),
            (OP_READ_INSTRUCTION_BYTES, 1),
        ];
        for (opcode, immediate) in positioned {
            let (mut builder, sysvar) = with_sysvar();
            let zero = builder.const_u64(0);
            let position = builder.const_u128(0);
            builder.op(opcode, sysvar, zero, position, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::TypeMismatch),
                "opcode {opcode} c"
            );
        }
    }

    #[test]
    fn instruction_data_reads_take_a_read_opcode_as_their_width() {
        let selectors = [
            (OP_READ_U8, VALUE_U64),
            (OP_READ_U16, VALUE_U64),
            (OP_READ_I32, VALUE_I64),
            (OP_READ_I64, VALUE_I64),
            (OP_READ_BOOL, VALUE_BOOL),
            (OP_READ_U128, VALUE_U128),
            (OP_READ_PUBKEY, VALUE_PUBKEY),
        ];
        for (selector, witness) in selectors {
            let (mut builder, sysvar) = with_sysvar();
            let zero = builder.const_u64(0);
            let value = builder.read_instruction_data(selector, sysvar, zero, zero);
            let other = typed_register(&mut builder, Some(witness));
            builder.binary(OP_EQ, value, other);
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "selector {selector}");
        }
        // Anything but a read opcode is refused, including a read opcode above the low byte.
        let immediates = [
            0,
            OP_ADD as u64,
            OP_READ_INSTRUCTION_DATA as u64,
            0x100 | OP_READ_U64 as u64,
            u64::MAX,
        ];
        for immediate in immediates {
            let (mut builder, sysvar) = with_sysvar();
            let zero = builder.const_u64(0);
            builder.op(OP_READ_INSTRUCTION_DATA, sysvar, zero, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidInstruction(1)),
                "immediate {immediate:#x}"
            );
        }
    }

    #[test]
    fn byte_reads_take_one_to_1024_bytes_and_are_typed_that_long() {
        for opcode in [OP_READ_INSTRUCTION_BYTES, OP_READ_ACCOUNT_BYTES] {
            let lengths = [
                (0, Err(TemplateError::InvalidInstruction(1))),
                (1, Ok(())),
                (MAX_INPUT_BYTES as u64, Ok(())),
                (MAX_INPUT_BYTES as u64 + 1, Err(TemplateError::InvalidInstruction(1))),
                (u64::MAX, Err(TemplateError::InvalidInstruction(1))),
            ];
            for (len, expected) in lengths {
                let (mut builder, sysvar) = with_sysvar();
                let zero = builder.const_u64(0);
                builder.op(opcode, sysvar, zero, zero, len);
                assert_eq!(
                    verify_builder(&builder).map(|_| ()),
                    expected,
                    "opcode {opcode} len {len}"
                );
            }
            // A CPI that forwards the bytes must declare exactly their length.
            let (mut builder, sysvar) = with_sysvar();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let zero = builder.const_u64(0);
            let bytes = builder.op(opcode, sysvar, zero, zero, 40);
            let cpi = builder.cpi(program, &[], &[Segment::Register(DATA_REG_BYTES, bytes)]);
            builder.set_cpi_max_data_len(cpi, 40);
            builder.invoke(cpi, None);
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "opcode {opcode}");
            builder.set_cpi_max_data_len(cpi, 41);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidCpi(0)),
                "opcode {opcode}"
            );
        }
    }

    #[test]
    fn account_byte_reads_take_a_declared_account_and_a_u64_offset() {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(0);
        builder.read_account_bytes(account, offset, 8);
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "a fixed account, unpinned");

        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let offset = builder.const_u64(0);
        builder.for_each(0, |body| {
            body.read_account_bytes(row, offset, 8);
        });
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "a row account in its loop");

        let mut builder = ProgramBuilder::new();
        let offset = builder.const_u64(0);
        builder.read_account_bytes(3, offset, 8);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidAccountConstraint(3)));

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_i64(0);
        builder.read_account_bytes(account, offset, 8);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));
    }

    /// The run refuses a byte read of an account its instruction can write, so a byte read of a
    /// fixed account declared writable, which every run passes writable, could never succeed and
    /// is refused as `InvalidInstruction`, the error of a bad byte-read length. A row account is
    /// left to the run.
    #[test]
    fn account_byte_reads_refuse_a_fixed_account_declared_writable() {
        for flags in [ACCOUNT_WRITABLE, ACCOUNT_SIGNER | ACCOUNT_WRITABLE] {
            let mut builder = ProgramBuilder::new();
            let account = builder.account(flags, None, None, 0);
            let offset = builder.const_u64(0);
            let pc = builder.instructions_mut().len();
            builder.read_account_bytes(account, offset, 8);
            assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidInstruction(pc)), "{flags}");
        }

        let mut builder = ProgramBuilder::new();
        let signer = builder.account(ACCOUNT_SIGNER, None, None, 0);
        let offset = builder.const_u64(0);
        builder.read_account_bytes(signer, offset, 8);
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "a read-only signer");

        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(2, 0);
        let offset = builder.const_u64(0);
        builder.for_each(0, |body| {
            body.read_account_bytes(row, offset, 8);
        });
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "a writable row account");
    }

    #[test]
    fn introspection_and_byte_opcodes_reject_flags() {
        // None of them is a read opcode, so the dynamic-offset flag is never theirs to claim.
        for opcode in OP_INSTRUCTION_COUNT..=OP_BYTES_LEN {
            let (mut builder, _) = introspection_program(opcode);
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "opcode {opcode} unflagged");
            let last = builder.instructions_mut().len() - 1;
            builder.instructions_mut()[last].flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET;
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidFlags(last)),
                "opcode {opcode}"
            );
        }
    }

    #[test]
    fn row_inputs_and_account_groups_are_verified() {
        // A row input loaded inside the loop takes the row descriptor's type.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let amount = builder.row_input(VALUE_U64, 0);
        builder.for_each(0, |body| {
            let value = body.load_input(amount);
            let floor = body.const_u64(0);
            let non_negative = body.binary(OP_LTE, floor, value);
            body.require(non_negative);
        });
        assert!(verify_builder(&builder).is_ok(), "row input inside the loop verifies");

        // Outside the loop the same reference is rejected.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        let amount = builder.row_input(VALUE_U64, 0);
        builder.load_input(amount);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(0))
        );

        // An offset past the declared row is rejected.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(3, 0);
        builder.row_input(VALUE_U64, 0);
        builder.for_each(0, |body| {
            body.load_input(ITERATION_INPUT_BIT | 1);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(1))
        );

        // Row inputs need a batch to iterate over.
        let mut builder = ProgramBuilder::new();
        builder.row_input(VALUE_U64, 0);
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // More than eight row inputs, or more than 32 descriptors in total, are rejected.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(1, 0);
        for _ in 0..MAX_ROW_INPUTS + 1 {
            builder.row_input(VALUE_BOOL, 0);
        }
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::TooManyInputs));

        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(1, 0);
        for _ in 0..MAX_INPUTS - 4 {
            builder.input(VALUE_BOOL, 0);
        }
        for _ in 0..5 {
            builder.row_input(VALUE_BOOL, 0);
        }
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(verify_builder(&builder), Err(TemplateError::TooManyInputs));

        // Fixed values plus iterations times row values are capped at 256.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(60, 0);
        for _ in 0..4 {
            builder.input(VALUE_BOOL, 0);
        }
        for _ in 0..MAX_ROW_INPUTS {
            builder.row_input(VALUE_BOOL, 0);
        }
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::TooManyInputs),
            "4 + 60 * 8 values"
        );
        builder.batch(31, 0);
        assert!(verify_builder(&builder).is_ok(), "4 + 31 * 8 values fit");

        // Nine account groups are too many.
        let mut builder = ProgramBuilder::new();
        builder.account_groups(MAX_ACCOUNT_GROUPS as u8 + 1);
        builder.const_bool(true);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::TooManyAccountGroups)
        );

        // A CPI may forward a declared group and nothing beyond it.
        let mut builder = ProgramBuilder::new();
        builder.account_groups(1);
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([9; 32]), None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let cpi = builder.cpi_with_group(
            program,
            &[(payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)],
            &[],
            0,
        );
        builder.invoke(cpi, None);
        assert!(verify_builder(&builder).is_ok(), "group 0 of 1 is valid");
        builder.cpis_mut()[0].account_group = 1;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCpi(0)));
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

        // Two loops over the same rows.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        builder.for_each(0, |body| body.require(condition));
        builder.for_each(0, |body| body.require(condition));
        assert!(verify_builder(&builder).is_ok());

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
    fn count_loops_take_a_u64_count_a_body_and_a_maximum() {
        // A count loop with the index, and a carried total that leaves the loop.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(3);
        let total = builder.const_u64(0);
        builder.repeat(count, 4, 1 << total, |body| {
            let index = body.loop_index();
            let sum = body.binary(OP_ADD, total, index);
            body.mov(total, sum);
        });
        let same = builder.binary(OP_EQ, total, total);
        builder.require(same);
        assert!(verify_builder(&builder).is_ok());

        // An empty body or a zero maximum is not a loop.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let repeat = builder.repeat(count, 1, 0, |_| {});
        builder.const_bool(true);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidLoop(repeat)));
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let repeat = builder.repeat(count, 0, 0, |body| {
            body.loop_index();
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidLoop(repeat)));

        // The count is read when the loop starts, so it must already hold a u64.
        let mut builder = ProgramBuilder::new();
        let count = builder.register();
        builder.repeat(count, 1, 0, |body| {
            body.loop_index();
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::RegisterNotInitialized(count)));
        let mut builder = ProgramBuilder::new();
        let count = builder.const_i64(1);
        builder.repeat(count, 1, 0, |body| {
            body.loop_index();
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));
        let mut builder = ProgramBuilder::new();
        builder.repeat(9, 1, 0, |body| {
            body.loop_index();
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidRegister(9)));

        // A body past the end of the program, and flags, which only reads take.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let repeat = builder.repeat(count, 1, 0, |body| {
            body.loop_index();
        });
        builder.instructions_mut()[repeat].a = 200;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidInstruction(repeat)));
        builder.instructions_mut()[repeat].a = 1;
        builder.instructions_mut()[repeat].flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET;
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidFlags(repeat)));

        // A REPEAT writes no register, so its destination is reserved, whatever register it names.
        builder.instructions_mut()[repeat].flags = 0;
        assert!(verify_builder(&builder).is_ok());
        for dst in [0, 1, MAX_REGISTERS as u8, NO_INDEX - 1] {
            builder.instructions_mut()[repeat].dst = dst;
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidLoop(repeat)),
                "destination {dst}"
            );
        }
        // FOREACH shipped accepting any destination, and templates that verify today must keep
        // verifying, so its destination stays unchecked.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let condition = builder.const_bool(true);
        let foreach = builder.for_each(0, |body| body.require(condition));
        builder.instructions_mut()[foreach].dst = 0;
        assert!(verify_builder(&builder).is_ok());

        // Carried registers follow the FOREACH rules: set before the loop, and the same type after.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let total = builder.register();
        builder.repeat(count, 1, 1 << total, |body| {
            let index = body.loop_index();
            body.mov(total, index);
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCarry(total)));
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let total = builder.const_u64(0);
        builder.repeat(count, 1, 1 << total, |body| {
            let flag = body.const_bool(true);
            body.mov(total, flag);
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidCarry(total)));

        // Registers written in the body are gone after it.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let mut inner = NO_INDEX;
        builder.repeat(count, 1, 0, |body| {
            inner = body.loop_index();
        });
        let same = builder.binary(OP_EQ, inner, inner);
        builder.require(same);
        assert_eq!(verify_builder(&builder), Err(TemplateError::RegisterNotInitialized(inner)));

        // One instruction at a time, a loop is never accepted, in any scope, with the error a
        // nested one gets.
        let mut builder = ProgramBuilder::new();
        builder.const_bool(true);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut registers = [None; MAX_REGISTERS];
        let repeat = record(OP_REPEAT, NO_INDEX, 1, 0, 1, 0, 0);
        let foreach = record(OP_FOREACH, NO_INDEX, 1, NO_INDEX, NO_INDEX, 0, 0);
        for scope in [LoopScope::Root, LoopScope::Rows, LoopScope::Count] {
            assert_eq!(
                program.verify_single_instruction(&repeat, 3, scope, None, &mut registers),
                Err(TemplateError::InvalidLoop(3)),
                "{scope:?}"
            );
            assert_eq!(
                program.verify_single_instruction(&foreach, 3, scope, None, &mut registers),
                Err(TemplateError::InvalidBatch),
                "{scope:?}"
            );
        }
    }

    /// An executable program, a writable row account with a pinned owner and eight bytes of data,
    /// a batch of two rows, and one `u64` row input.
    fn declare_rows(builder: &mut ProgramBuilder) -> (u8, u8, u8) {
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let row = builder.row_account(ACCOUNT_WRITABLE, None, Some([2; 32]), 8);
        builder.batch(2, 0);
        let amount = builder.row_input(VALUE_U64, 0);
        (program, row, amount)
    }

    /// `(what, emit)`: a body that names a row, for
    /// [`count_loop_bodies_have_an_index_but_no_rows`]. `emit` takes the program, row account and
    /// row input from [`declare_rows`], and names the row in the last instruction it emits.
    type RowCase = (&'static str, fn(&mut ProgramBuilder, u8, u8, u8));

    #[test]
    fn count_loop_bodies_have_an_index_but_no_rows() {
        let cases: [RowCase; 6] = [
            ("row input", |body, _, _, amount| {
                body.load_input(amount);
            }),
            ("row account field", |body, _, row, _| {
                body.account_key(row);
            }),
            ("row account read", |body, _, row, _| {
                body.read(OP_READ_U64, row, 0);
            }),
            ("row account read at a register offset", |body, _, row, _| {
                let offset = body.const_u64(0);
                body.read_dynamic(OP_READ_U64, row, offset);
            }),
            ("row account bytes", |body, _, row, _| {
                let offset = body.const_u64(0);
                body.read_account_bytes(row, offset, 8);
            }),
            ("invoke passing a row account", |body, program, row, _| {
                let cpi = body.cpi(program, &[(row, ACCOUNT_WRITABLE)], &[]);
                body.invoke(cpi, None);
            }),
        ];
        for (what, emit) in cases {
            // Inside a FOREACH the row is there.
            let mut builder = ProgramBuilder::new();
            let (program, row, amount) = declare_rows(&mut builder);
            builder.for_each(0, |body| emit(body, program, row, amount));
            assert!(verify_builder(&builder).is_ok(), "{what} in a FOREACH");
            // Inside a REPEAT it is not, though the FOREACH before it iterates the rows.
            let count = builder.const_u64(1);
            builder.repeat(count, 1, 0, |body| emit(body, program, row, amount));
            let last = builder.instructions_mut().len() - 1;
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidLoop(last)),
                "{what} in a REPEAT"
            );
        }
    }

    #[test]
    fn a_template_holds_up_to_eight_loops_in_sequence_and_none_nested() {
        // Two FOREACH loops over the same rows, with a REPEAT between them.
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let count = builder.const_u64(2);
        builder.for_each(0, |body| {
            body.account_lamports(row);
        });
        builder.repeat(count, 2, 0, |body| {
            body.loop_index();
        });
        builder.for_each(0, |body| {
            body.account_key(row);
        });
        assert!(verify_builder(&builder).is_ok());

        // A count loop needs no batch, but a batch still needs a FOREACH.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(2);
        builder.repeat(count, 2, 0, |body| {
            body.loop_index();
        });
        assert!(verify_builder(&builder).is_ok());
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Eight loops, and not a ninth.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        for _ in 0..MAX_LOOPS {
            builder.repeat(count, 1, 0, |body| {
                body.loop_index();
            });
        }
        assert!(verify_builder(&builder).is_ok());
        let ninth = builder.repeat(count, 1, 0, |body| {
            body.loop_index();
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidLoop(ninth)));

        // No nesting: a REPEAT inside any body is InvalidLoop; a FOREACH is InvalidBatch, as before.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let count = builder.const_u64(1);
        let mut inner = 0;
        builder.for_each(0, |body| {
            inner = body.repeat(count, 1, 0, |inner_body| {
                inner_body.loop_index();
            });
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidLoop(inner)));
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let mut inner = 0;
        builder.repeat(count, 1, 0, |body| {
            inner = body.repeat(count, 1, 0, |inner_body| {
                inner_body.loop_index();
            });
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidLoop(inner)));
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let count = builder.const_u64(1);
        builder.repeat(count, 1, 0, |body| {
            body.for_each(0, |inner_body| {
                inner_body.loop_index();
            });
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidBatch));

        // Return data never follows a loop: the invoke that set it ran in the body.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(program, &[], &[]);
        let count = builder.const_u64(1);
        builder.repeat(count, 1, 0, |body| body.invoke(cpi, None));
        let read = builder.instructions_mut().len();
        builder.return_data(OP_READ_U64, 0);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidReturnData(read)));
    }

    #[test]
    fn worst_case_invocations_add_up_over_every_loop() {
        // One invoke at the root, one per batch row, and two per REPEAT pass.
        let build = |rows: u8, max: u8| {
            let mut builder = ProgramBuilder::new();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            builder.row_account(0, None, None, 0);
            builder.batch(rows, 0);
            let cpi = builder.cpi(program, &[], &[]);
            let count = builder.const_u64(1);
            builder.invoke(cpi, None);
            builder.for_each(0, |body| body.invoke(cpi, None));
            builder.repeat(count, max, 0, |body| {
                body.invoke(cpi, None);
                body.invoke(cpi, None);
            });
            verify_builder(&builder)
        };
        // 1 + 21 × 1 + 21 × 2 = 64.
        assert_eq!(build(21, 21).unwrap().max_expanded_cpis, 64);
        assert_eq!(build(22, 21), Err(TemplateError::ExcessiveCpiExpansion));
        assert_eq!(build(21, 22), Err(TemplateError::ExcessiveCpiExpansion));
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
        builder.cpis_mut()[0].account_group = 0;
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
    fn create_pda_requires_a_u64_bump() {
        // A u64 bump and one seed verify.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let owner = builder.account(0, None, None, 0);
        let key = builder.account_key(owner);
        let bump = builder.const_u64(255);
        let literal = builder.blob(b"position");
        let created = builder.create_pda(
            program,
            bump,
            &[Segment::Literal(literal), Segment::Register(DATA_REG_PUBKEY, key)],
        );
        let matches = builder.binary(OP_EQ, created, key);
        builder.require(matches);
        assert!(verify_builder(&builder).is_ok());

        // A pubkey bump register is rejected by typing.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let owner = builder.account(0, None, None, 0);
        let key = builder.account_key(owner);
        let literal = builder.blob(b"position");
        builder.create_pda(program, key, &[Segment::Literal(literal)]);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));

        // A non-executable program account is rejected, as for DERIVE_PDA.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(0, Some([1; 32]), None, 0);
        let bump = builder.const_u64(254);
        let literal = builder.blob(b"position");
        let created = builder.create_pda(program, bump, &[Segment::Literal(literal)]);
        let matches = builder.binary(OP_EQ, created, created);
        builder.require(matches);
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::InvalidInstruction(1))
        );
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
            (OP_READ_I32, 4),
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
            (OP_READ_I32, 4),
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
    fn outputs_are_encoded_like_invocation_data_up_to_the_return_data_limit() {
        // Literal bytes, a u64 narrowed to two bytes, a pubkey, and a bytes input counted at its
        // maximum length. Neither output is an invocation.
        let mut builder = ProgramBuilder::new();
        let owner = builder.account(0, None, None, 0);
        let memo_input = builder.input(VALUE_BYTES, 16);
        let memo = builder.load_input(memo_input);
        let amount = builder.const_u64(7);
        let key = builder.account_key(owner);
        let tag = builder.blob(b"TAG1");
        builder.emit_data(&[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_U16, amount),
            Segment::Register(DATA_REG_PUBKEY, key),
            Segment::Register(DATA_REG_BYTES, memo),
        ]);
        builder.set_return_data(&[Segment::Register(DATA_REG_U64, amount)]);
        let stats = verify_builder(&builder).unwrap();
        assert_eq!((stats.cpis, stats.max_expanded_cpis, stats.max_cpi_data_len), (0, 0, 0));

        // A 1,024-byte bytes input fills the limit; one more literal byte passes it.
        let at_limit = |extra: usize| {
            let mut builder = ProgramBuilder::new();
            let input = builder.input(VALUE_BYTES, MAX_INPUT_BYTES as u16);
            let value = builder.load_input(input);
            let literal = builder.blob(&vec![0; extra]);
            let at = builder.set_return_data(&[
                Segment::Register(DATA_REG_BYTES, value),
                Segment::Literal(literal),
            ]);
            (verify_builder(&builder).map(|_| ()), at)
        };
        assert_eq!(at_limit(0).0, Ok(()));
        let (outcome, at) = at_limit(1);
        assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));

        // EMIT has the same bound, and a select counts its longer branch: 1,000 bytes, after a tag
        // of padding.
        let emit_with_padding = |padding: usize| {
            let mut builder = ProgramBuilder::new();
            let condition = builder.const_bool(true);
            let short = builder.const_bytes(&[1; 4]);
            let long = builder.const_bytes(&[2; 1_000]);
            let selected = builder.select(condition, short, long);
            let literal = builder.blob(&vec![0; padding]);
            let at = builder.emit_data(&[
                Segment::Literal(literal),
                Segment::Register(DATA_REG_BYTES, selected),
            ]);
            (verify_builder(&builder).map(|_| ()), at)
        };
        assert_eq!(emit_with_padding(24).0, Ok(()));
        let (outcome, at) = emit_with_padding(25);
        assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));
    }

    #[test]
    fn output_segments_follow_the_segment_rules() {
        // Each case builds a CPI with two segments first, so the output's own segment is at
        // index 2: errors name the segment's place in the whole table. The segment rules come
        // before an EMIT's tag, so these untagged logs fail on their segment.
        let check = |mutate: &dyn Fn(&mut ProgramBuilder, u8), expected: TemplateError| {
            let mut builder = ProgramBuilder::new();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let amount = builder.const_u64(1);
            let literal = builder.blob(&[2, 0, 0, 0]);
            let cpi = builder.cpi(
                program,
                &[],
                &[Segment::Literal(literal), Segment::Register(DATA_REG_U64, amount)],
            );
            builder.invoke(cpi, None);
            mutate(&mut builder, amount);
            assert_eq!(verify_builder(&builder), Err(expected), "{expected:?}");
        };
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_I64, amount)]);
            },
            TemplateError::TypeMismatch,
        );
        check(
            &|builder, _| {
                let unset = builder.register();
                builder.emit_data(&[Segment::Register(DATA_REG_U64, unset)]);
            },
            TemplateError::RegisterNotInitialized(1),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(0xfe, amount)]);
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, _| {
                builder.emit_data(&[Segment::Literal((2, 3))]);
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, _| {
                builder.emit_data(&[Segment::Literal((0, 1))]);
                builder.segments_mut()[2].register = 0;
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_U64, amount)]);
                builder.segments_mut()[2].len_le = [1, 0];
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_U64, amount)]);
                builder.segments_mut()[2].offset_le = [1, 0];
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_U64, amount)]);
                builder.segments_mut()[2].reserved = [0, 1];
            },
            TemplateError::InvalidDataSegment(2),
        );
    }

    #[test]
    fn output_records_name_a_non_empty_range_and_no_register() {
        // Either output names both segments in the table: a tag, then the amount.
        let with_output = |opcode: u8, mutate: &dyn Fn(&mut InstructionRecord)| {
            let mut builder = ProgramBuilder::new();
            let amount = builder.const_u64(1);
            let tag = builder.blob(b"TAG1");
            let parts = [Segment::Literal(tag), Segment::Register(DATA_REG_U64, amount)];
            let at = if opcode == OP_EMIT {
                builder.emit_data(&parts)
            } else {
                builder.set_return_data(&parts)
            };
            mutate(&mut builder.instructions_mut()[at]);
            (verify_builder(&builder).map(|_| ()), at)
        };
        for opcode in [OP_EMIT, OP_SET_RETURN_DATA] {
            assert_eq!(with_output(opcode, &|_| {}).0, Ok(()), "opcode {opcode}");
            for mutate in [
                (|record: &mut InstructionRecord| record.dst = 0) as fn(&mut InstructionRecord),
                |record| record.a = 0,
                |record| record.b = 0,
                |record| record.c = 0,
                |record| record.immediate_le = range_immediate(0, 0).to_le_bytes(),
                |record| record.immediate_le = range_immediate(2, 1).to_le_bytes(),
                |record| record.immediate_le = range_immediate(0, 3).to_le_bytes(),
            ] {
                let (outcome, at) = with_output(opcode, &mutate);
                assert_eq!(
                    outcome,
                    Err(TemplateError::InvalidInstruction(at)),
                    "opcode {opcode}"
                );
            }
            let (outcome, at) =
                with_output(opcode, &|record| record.flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET);
            assert_eq!(outcome, Err(TemplateError::InvalidFlags(at)), "opcode {opcode}");
        }
    }

    /// A log line names the program that wrote it, not the template, so an `EMIT` starts with a
    /// literal tag of four bytes or more, and never with the run event's family: otherwise any
    /// template could log a byte-exact run event for any template address.
    #[test]
    fn an_emit_starts_with_a_tag_outside_the_run_event_family() {
        let emit = |parts: &dyn Fn(&mut ProgramBuilder, u8) -> Vec<Segment>| {
            let mut builder = ProgramBuilder::new();
            let amount = builder.const_u64(1);
            let parts = parts(&mut builder, amount);
            let at = builder.emit_data(&parts);
            (verify_builder(&builder).map(|_| ()), at)
        };

        // A tag of four bytes or more, alone or before the data, including tags that only
        // resemble the family.
        for tag in [&b"TAG1"[..], b"a longer tag", b"BEU1", b"bev1", b"XBEV"] {
            let tagged = emit(&|builder, amount| {
                vec![
                    Segment::Literal(builder.blob(tag)),
                    Segment::Register(DATA_REG_U64, amount),
                ]
            });
            assert_eq!(tagged.0, Ok(()), "{tag:?}");
            assert_eq!(
                emit(&|builder, _| vec![Segment::Literal(builder.blob(tag))]).0,
                Ok(())
            );
        }

        // No tag, a register before the tag, and a three-byte tag, even with a fourth literal
        // byte after it: the tag is the first segment alone.
        for parts in [
            (|_: &mut ProgramBuilder, amount: u8| vec![Segment::Register(DATA_REG_U64, amount)])
                as fn(&mut ProgramBuilder, u8) -> Vec<Segment>,
            |builder, amount| {
                vec![
                    Segment::Register(DATA_REG_U64, amount),
                    Segment::Literal(builder.blob(b"TAG1")),
                ]
            },
            |builder, amount| {
                vec![
                    Segment::Literal(builder.blob(b"TAG")),
                    Segment::Register(DATA_REG_U64, amount),
                ]
            },
            |builder, _| {
                vec![
                    Segment::Literal(builder.blob(b"TAG")),
                    Segment::Literal(builder.blob(b"1")),
                ]
            },
        ] {
            let (outcome, at) = emit(&parts);
            assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));
        }

        // The run event's magic, any other version of it, and a byte-exact event: magic, bytecode
        // version, iterations, invokes reached, the mask of those that ran, template address.
        let mut event = b"BEV1".to_vec();
        event.extend_from_slice(&[TEMPLATE_PROGRAM_VERSION, 0, 1]);
        event.extend_from_slice(&1u64.to_le_bytes());
        event.extend_from_slice(&[9; 32]);
        for tag in [&b"BEV1"[..], b"BEV2", b"BEV\0", b"BEVERAGE", &event] {
            let (outcome, at) = emit(&|builder, _| vec![Segment::Literal(builder.blob(tag))]);
            assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)), "{tag:?}");
        }

        // Return data is read by the caller that invoked the run, never mistaken for a log, so it
        // needs no tag.
        let mut builder = ProgramBuilder::new();
        let amount = builder.const_u64(1);
        builder.set_return_data(&[Segment::Register(DATA_REG_U64, amount)]);
        assert!(verify_builder(&builder).is_ok());
    }

    #[test]
    fn return_data_is_set_once_outside_every_loop_after_the_last_invoke() {
        let transfer = |builder: &mut ProgramBuilder| {
            let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            builder.cpi(system, &[], &[])
        };
        let result = |builder: &mut ProgramBuilder| {
            let value = builder.const_u64(7);
            builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)])
        };

        // After the last invoke, with outputs logged anywhere else, it verifies.
        let mut builder = ProgramBuilder::new();
        let cpi = transfer(&mut builder);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let count = builder.const_u64(3);
        let tag = builder.blob(b"TAG1");
        builder.emit_data(&[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_U64, count),
        ]);
        builder.for_each(0, |body| {
            let row = body.loop_index();
            body.emit_data(&[Segment::Literal(tag), Segment::Register(DATA_REG_U8, row)]);
            body.invoke(cpi, None);
        });
        builder.invoke(cpi, None);
        result(&mut builder);
        assert!(verify_builder(&builder).is_ok());

        // Before an invoke.
        let mut builder = ProgramBuilder::new();
        let cpi = transfer(&mut builder);
        let at = result(&mut builder);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // Before a loop whose body invokes.
        let mut builder = ProgramBuilder::new();
        let cpi = transfer(&mut builder);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let at = result(&mut builder);
        builder.for_each(0, |body| body.invoke(cpi, None));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // Inside a loop.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let value = builder.const_u64(7);
        let mut at = 0;
        builder.for_each(0, |body| {
            at = body.set_return_data(&[Segment::Register(DATA_REG_U64, value)]);
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // Twice: the first names the error.
        let mut builder = ProgramBuilder::new();
        let at = result(&mut builder);
        result(&mut builder);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // The single-instruction entry point the specifications use never indexes out of range.
        let mut builder = ProgramBuilder::new();
        result(&mut builder);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut registers = [None; MAX_REGISTERS];
        registers[0] = Some(RegisterInfo::scalar(VALUE_U64));
        let record = program.instructions[1];
        assert_eq!(
            program.verify_single_instruction(
                &record,
                usize::MAX,
                LoopScope::Root,
                None,
                &mut registers
            ),
            Ok((0, 0))
        );
    }

    /// A count loop, the loops phase's `REPEAT`, is a loop like any other.
    #[test]
    fn return_data_is_not_set_inside_a_count_loop() {
        // REPEAT: `a` is the body length, `b` the u64 count register, `c` the static maximum.
        let with_body = |body: &dyn Fn(&mut ProgramBuilder, u8) -> usize| {
            let mut builder = ProgramBuilder::new();
            let count = builder.const_u64(2);
            let value = builder.const_u64(7);
            builder.emit(record(OP_REPEAT, NO_INDEX, 1, count, 2, 0, 0));
            let at = body(&mut builder, value);
            (verify_builder(&builder).map(|_| ()), at)
        };
        let (outcome, at) = with_body(&|builder, value| {
            builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)])
        });
        assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));
        let (outcome, _) = with_body(&|builder, value| {
            let tag = builder.blob(b"TAG1");
            builder.emit_data(&[
                Segment::Literal(tag),
                Segment::Register(DATA_REG_U64, value),
            ])
        });
        assert_eq!(outcome, Ok(()));
    }

    /// As an `EMIT` checks its segments before its tag, a `SET_RETURN_DATA` checks them before
    /// where it sits, so a misplaced output with a bad segment reports the segment.
    #[test]
    fn return_data_reports_a_bad_segment_before_its_placement() {
        // Inside a loop, naming a register that holds no value.
        let mut builder = ProgramBuilder::new();
        let count = builder.const_u64(1);
        let unset = builder.register();
        builder.repeat(count, 1, 0, |body| {
            body.set_return_data(&[Segment::Register(DATA_REG_U64, unset)]);
        });
        assert_eq!(
            verify_builder(&builder),
            Err(TemplateError::RegisterNotInitialized(unset))
        );

        // Before an invoke, with a segment of no known kind. The invocation has no data, so the
        // output's segment is the table's first.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(system, &[], &[]);
        let value = builder.const_u64(7);
        builder.set_return_data(&[Segment::Register(0xfe, value)]);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidDataSegment(0)));

        // With good segments, the placement still decides.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
        let cpi = builder.cpi(system, &[], &[]);
        let value = builder.const_u64(7);
        let at = builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)]);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));
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

    /// Compiled by the TypeScript SDK; regenerate with `pnpm fixtures`.
    const SYSTEM_TRANSFER_HEX: &str = include_str!("../../../fixtures/system-transfer.hex");
    const ATA_ASSERTION_HEX: &str = include_str!("../../../fixtures/assert-ata.hex");

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

    /// Every compiler fixture must parse and verify; the TypeScript suite keeps the files current.
    #[test]
    fn every_shared_fixture_parses_and_verifies() {
        let fixtures: [(&str, &str); 20] = [
            ("system-transfer", include_str!("../../../fixtures/system-transfer.hex")),
            ("batch-transfer-30", include_str!("../../../fixtures/batch-transfer-30.hex")),
            ("ensure-ata", include_str!("../../../fixtures/ensure-ata.hex")),
            ("assert-ata", include_str!("../../../fixtures/assert-ata.hex")),
            (
                "assert-ata-with-bump",
                include_str!("../../../fixtures/assert-ata-with-bump.hex"),
            ),
            (
                "checked-transfer-snapshot",
                include_str!("../../../fixtures/checked-transfer-snapshot.hex"),
            ),
            ("carry-sum", include_str!("../../../fixtures/carry-sum.hex")),
            ("waterfall-payout", include_str!("../../../fixtures/waterfall-payout.hex")),
            ("dynamic-read", include_str!("../../../fixtures/dynamic-read.hex")),
            ("return-data", include_str!("../../../fixtures/return-data.hex")),
            ("event-flag", include_str!("../../../fixtures/event-flag.hex")),
            ("pinned-mint-read", include_str!("../../../fixtures/pinned-mint-read.hex")),
            ("payroll-row-amounts", include_str!("../../../fixtures/payroll-row-amounts.hex")),
            ("group-forward-transfer", include_str!("../../../fixtures/group-forward-transfer.hex")),
            ("math-ops", include_str!("../../../fixtures/math-ops.hex")),
            ("output", include_str!("../../../fixtures/output.hex")),
            ("loops", include_str!("../../../fixtures/loops.hex")),
            ("introspection", include_str!("../../../fixtures/introspection.hex")),
            (
                "signed-quote-settlement",
                include_str!("../../../fixtures/signed-quote-settlement.hex"),
            ),
            (
                "rate-limited-transfer",
                include_str!("../../../fixtures/rate-limited-transfer.hex"),
            ),
        ];
        for (name, hex) in fixtures {
            let bytes = decode_hex(hex);
            let program =
                ProgramView::parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
            program
                .verify()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(program.header.version(), TEMPLATE_PROGRAM_VERSION, "{name}");
        }

        // The live-protocol examples in `clients/js/examples/protocols` are compiled by the
        // TypeScript suite into this file, alongside the fixed-account, input and group order the
        // LiteSVM harness uses to build each run instruction by name. They target programs no
        // test can invoke, but the verifier is exactly the check that matters for them: it is
        // what the chain runs before a template is allowed to be stored.
        let protocols = include_str!("../../../fixtures/protocol-examples.json");
        let mut examples = 0usize;
        for entry in payload_values(protocols) {
            let bytes = decode_hex(entry);
            let program = ProgramView::parse(&bytes)
                .unwrap_or_else(|error| panic!("protocol example {examples}: {error}"));
            program
                .verify()
                .unwrap_or_else(|error| panic!("protocol example {examples}: {error}"));
            examples += 1;
        }
        assert_eq!(examples, 13, "every protocol example is verified");
        // The test-only runtime scenarios in `clients/js/examples/scenarios`, recorded the same way
        // in their own file. The LiteSVM suite uploads them, but it needs the snapshot; this does
        // not.
        let scenarios = include_str!("../../../fixtures/protocol-scenarios.json");
        let mut verified = 0usize;
        for entry in payload_values(scenarios) {
            let bytes = decode_hex(entry);
            let program = ProgramView::parse(&bytes)
                .unwrap_or_else(|error| panic!("runtime scenario {verified}: {error}"));
            program
                .verify()
                .unwrap_or_else(|error| panic!("runtime scenario {verified}: {error}"));
            verified += 1;
        }
        assert_eq!(verified, 5, "every runtime scenario is verified");
        let carry = decode_hex(include_str!("../../../fixtures/carry-sum.hex"));
        let program = ProgramView::parse(&carry).unwrap();
        assert_eq!(program.header.batch_min_iterations(), 1);
        let event = decode_hex(include_str!("../../../fixtures/event-flag.hex"));
        let program = ProgramView::parse(&event).unwrap();
        assert_eq!(program.header.flags(), PROGRAM_FLAG_EMIT_EVENT);
    }

    /// Each `"payload": "<hex>"` value in `protocol-examples.json`, in file order. The fixture
    /// now also carries account and input names alongside each payload, so picking out quoted
    /// strings over some length is no longer safe; reading the `payload` key explicitly is.
    fn payload_values(json: &str) -> impl Iterator<Item = &str> {
        const KEY: &str = "\"payload\":";
        let mut rest = json;
        std::iter::from_fn(move || {
            let after_key = &rest[rest.find(KEY)? + KEY.len()..];
            let open = after_key.find('"')? + 1;
            let close = open + after_key[open..].find('"')?;
            rest = &after_key[close + 1..];
            Some(&after_key[open..close])
        })
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).unwrap();
                let low = (pair[1] as char).to_digit(16).unwrap();
                ((high << 4) | low) as u8
            })
            .collect()
    }

    /// System program, entry and payer, then an open of registry 2 (16 bytes) keyed by the
    /// payer's address. Returns the builder, the three accounts and the open's pc.
    fn registry_program() -> (ProgramBuilder, u8, u8, u8, usize) {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let key = builder.account_key(payer);
        let pc = builder.open_registry(entry, Some(key), payer, 2, 16, system);
        (builder, system, entry, payer, pc)
    }

    fn next_pc(builder: &mut ProgramBuilder) -> usize {
        builder.instructions_mut().len()
    }

    #[test]
    fn registry_rules() {
        let invalid = TemplateError::InvalidRegistry;

        // A valid open, a read of each width and a write of each writable width; an open counts
        // three CPIs.
        let (mut builder, _, entry, _, _) = registry_program();
        let spent = builder.read_registry(entry, 0, OP_READ_U64);
        builder.read_registry(entry, 15, OP_READ_U8);
        builder.read_registry(entry, 12, OP_READ_I32);
        builder.write_registry(entry, 8, OP_READ_U64, spent);
        let flag = builder.const_bool(true);
        builder.write_registry(entry, 15, OP_READ_BOOL, flag);
        let stats = verify_builder(&builder).unwrap();
        assert_eq!(stats.max_expanded_cpis, 3);

        // A pubkey field takes 32 bytes: not in a 16-byte registry, and fine in a 32-byte one.
        let (mut shorter, _, entry, _, _) = registry_program();
        let pc = next_pc(&mut shorter);
        shorter.read_registry(entry, 0, OP_READ_PUBKEY);
        assert_eq!(verify_builder(&shorter).map(|_| ()), Err(invalid(pc)));
        let (mut wider, _, entry, _, open) = registry_program();
        wider.instructions_mut()[open].immediate_le =
            RegistryOpen { index: 2, size: 32, system_program: 0 }.encode().to_le_bytes();
        wider.read_registry(entry, 0, OP_READ_PUBKEY);
        assert!(verify_builder(&wider).is_ok());

        // No key: the zero key.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.open_registry(entry, None, payer, 0, 1, system);
        assert!(verify_builder(&builder).is_ok());

        // Each broken open, one at a time. `(entry flags, entry pinned, payer flags, system pinned
        // to, key type, index, size)`.
        let base = (ACCOUNT_WRITABLE, false, ACCOUNT_SIGNER | ACCOUNT_WRITABLE, SYSTEM_PROGRAM_ADDRESS, Some(VALUE_PUBKEY), 0u8, 16u16);
        let cases = [
            ("entry read-only", (0, false, base.2, base.3, base.4, 0, 16)),
            ("entry signer and writable", (ACCOUNT_SIGNER | ACCOUNT_WRITABLE, false, base.2, base.3, base.4, 0, 16)),
            ("entry executable and writable", (ACCOUNT_EXECUTABLE | ACCOUNT_WRITABLE, false, base.2, base.3, base.4, 0, 16)),
            ("entry pinned", (ACCOUNT_WRITABLE, true, base.2, base.3, base.4, 0, 16)),
            ("payer not a signer", (base.0, false, ACCOUNT_WRITABLE, base.3, base.4, 0, 16)),
            ("payer read-only", (base.0, false, ACCOUNT_SIGNER, base.3, base.4, 0, 16)),
            ("system program elsewhere", (base.0, false, base.2, [1; 32], base.4, 0, 16)),
            ("key a u64", (base.0, false, base.2, base.3, Some(VALUE_U64), 0, 16)),
            ("key unset", (base.0, false, base.2, base.3, None, 0, 16)),
            ("index 8", (base.0, false, base.2, base.3, base.4, 8, 16)),
            ("size 0", (base.0, false, base.2, base.3, base.4, 0, 0)),
            ("size 513", (base.0, false, base.2, base.3, base.4, 0, 513)),
            ("size 512 is fine", (base.0, false, base.2, base.3, base.4, 0, 512)),
        ];
        for (name, (entry_flags, pinned, payer_flags, system_address, key_type, index, size)) in cases {
            let mut builder = ProgramBuilder::new();
            let system = builder.account(0, Some(system_address), None, 0);
            let entry = builder.account(entry_flags, pinned.then_some([5; 32]), None, 0);
            let payer = builder.account(payer_flags, None, None, 0);
            let key = typed_register(&mut builder, key_type);
            let pc = builder.open_registry(entry, Some(key), payer, index, size, system);
            let expected = if name.ends_with("is fine") { Ok(()) } else { Err(invalid(pc)) };
            assert_eq!(verify_builder(&builder).map(|_| ()), expected, "{name}");
        }

        // An entry's owner is the System program until its open creates it and Ballista after,
        // and it has no data until then: an owner pin fails every run on one side of creation, and
        // a data-length floor fails creation itself.
        let cases = [
            ("owner pinned to Ballista", Some([7; 32]), 0),
            ("owner pinned to the System program", Some(SYSTEM_PROGRAM_ADDRESS), 0),
            ("a data-length floor", None, 72 + 16),
            ("a one-byte data-length floor", None, 1),
        ];
        for (name, owner, min_data_len) in cases {
            let mut builder = ProgramBuilder::new();
            let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
            let entry = builder.account(ACCOUNT_WRITABLE, None, owner, min_data_len);
            let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
            let pc = builder.open_registry(entry, None, payer, 0, 16, system);
            assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "{name}");
        }

        // The accounts must be fixed ones: a row account, or one past the declared accounts.
        for (entry, payer, system) in [(ITERATION_ACCOUNT_BIT, 2, 0), (1, 9, 0), (1, 2, ITERATION_ACCOUNT_BIT)] {
            let mut builder = ProgramBuilder::new();
            builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
            builder.account(ACCOUNT_WRITABLE, None, None, 0);
            builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
            let pc = builder.open_registry(entry, None, payer, 0, 8, system);
            assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "{entry} {payer} {system}");
        }

        // The System program account must be pinned to it; declaring it executable is not enough.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let pc = builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "system program unpinned");

        // A destination, or a spare immediate byte.
        let (mut builder, _, _, _, pc) = registry_program();
        builder.instructions_mut()[pc].dst = 0;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));
        let (mut builder, _, _, _, pc) = registry_program();
        builder.instructions_mut()[pc].immediate_le[4] = 1;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Only at the root.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let count = builder.const_u64(1);
        let mut pc = 0;
        builder.repeat(count, 1, 0, |body| pc = body.open_registry(entry, None, payer, 0, 8, system));
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let mut pc = 0;
        builder.for_each(0, |body| pc = body.open_registry(entry, None, payer, 0, 8, system));
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "in a FOREACH body");

        // Each entry account once; one size per registry index; two entries of one registry.
        let (mut builder, system, entry, payer, _) = registry_program();
        let pc = builder.open_registry(entry, None, payer, 3, 16, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "entry opened twice");
        let (mut builder, system, _, payer, _) = registry_program();
        let other = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let pc = builder.open_registry(other, None, payer, 2, 24, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "registry 2 is 16 bytes");
        let (mut builder, system, _, payer, _) = registry_program();
        let other = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        builder.open_registry(other, None, payer, 2, 16, system);
        assert_eq!(verify_builder(&builder).unwrap().max_expanded_cpis, 6);

        // At most eight opens.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let entries: Vec<u8> = (0..9).map(|_| builder.account(ACCOUNT_WRITABLE, None, None, 0)).collect();
        let pcs: Vec<usize> = entries
            .iter()
            .map(|entry| builder.open_registry(*entry, None, payer, 0, 8, system))
            .collect();
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pcs[8])));

        // Never after SET_RETURN_DATA.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let tag = builder.blob(&[1]);
        builder.set_return_data(&[Segment::Literal(tag)]);
        let pc = builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Reads and writes need an open of their account before them, and a field inside it.
        for (name, on_payer, offset, selector, write, fine) in [
            ("read past the end", false, 9, OP_READ_U64, false, false),
            ("read the last byte", false, 15, OP_READ_U8, false, true),
            ("read an account never opened", true, 0, OP_READ_U64, false, false),
            ("read with a selector that is not a read", false, 0, OP_ADD, false, false),
            ("write an account never opened", true, 0, OP_READ_U64, true, false),
            ("write a u8", false, 0, OP_READ_U8, true, false),
            ("write a u64", false, 8, OP_READ_U64, true, true),
            ("write an i64 from a u64", false, 8, OP_READ_I64, true, false),
        ] {
            let (mut program, _, entry, payer, _) = registry_program();
            let value = program.const_u64(1);
            let account = if on_payer { payer } else { entry };
            let pc = next_pc(&mut program);
            if write {
                program.write_registry(account, offset, selector, value);
            } else {
                program.read_registry(account, offset, selector);
            }
            let expected = if fine { Ok(()) } else { Err(invalid(pc)) };
            assert_eq!(verify_builder(&program).map(|_| ()), expected, "{name}");
        }

        // A read before its open.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.read_registry(entry, 0, OP_READ_U64);
        builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(0)));

        // Spare operands, and a write's destination.
        let (mut builder, _, entry, _, _) = registry_program();
        let pc = next_pc(&mut builder);
        builder.read_registry(entry, 0, OP_READ_U64);
        builder.instructions_mut()[pc].b = 0;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));
        let (mut builder, _, entry, _, _) = registry_program();
        let value = builder.const_u64(1);
        let pc = builder.write_registry(entry, 0, OP_READ_U64, value);
        builder.instructions_mut()[pc].dst = 0;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Reads and writes work inside a loop body; the open stays at the root.
        let (mut builder, _, entry, _, _) = registry_program();
        let count = builder.const_u64(2);
        builder.repeat(count, 2, 0, |body| {
            let spent = body.read_registry(entry, 0, OP_READ_U64);
            body.write_registry(entry, 0, OP_READ_U64, spent);
        });
        assert!(verify_builder(&builder).is_ok());
    }

    /// An entry's data is read only through its fields: any other account-data read of an entry
    /// account is refused, after the open, before it, where a CPI could still change the entry,
    /// or in a loop body. The entry's key, owner, lamports and data length stay readable.
    #[test]
    fn entry_data_is_read_only_through_its_fields() {
        let invalid = TemplateError::InvalidRegistry;
        let read_opcodes = [
            OP_READ_U8, OP_READ_U16, OP_READ_U32, OP_READ_U64, OP_READ_I64, OP_READ_U128,
            OP_READ_PUBKEY, OP_READ_BOOL, OP_READ_I32,
        ];
        for opcode in read_opcodes {
            let (mut builder, _, entry, _, _) = registry_program();
            let pc = next_pc(&mut builder);
            builder.read(opcode, entry, 72);
            assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "static {opcode}");

            let (mut builder, _, entry, _, _) = registry_program();
            let offset = builder.const_u64(72);
            let pc = next_pc(&mut builder);
            builder.read_dynamic(opcode, entry, offset);
            assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "dynamic {opcode}");
        }
        let (mut builder, _, entry, _, _) = registry_program();
        let offset = builder.const_u64(72);
        let pc = next_pc(&mut builder);
        builder.read_account_bytes(entry, offset, 8);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "bytes");

        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let offset = builder.const_u64(72);
        let pc = next_pc(&mut builder);
        builder.read_dynamic(OP_READ_U64, entry, offset);
        builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "before the open");

        let (mut builder, _, entry, _, _) = registry_program();
        let offset = builder.const_u64(72);
        let count = builder.const_u64(1);
        let mut pc = 0;
        builder.repeat(count, 1, 0, |body| {
            pc = next_pc(body);
            body.read_dynamic(OP_READ_U64, entry, offset);
        });
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "in a loop body");

        // Not fields: the entry's header reads, and another account's data.
        let (mut builder, _, entry, payer, _) = registry_program();
        builder.account_key(entry);
        builder.account_owner(entry);
        builder.account_lamports(entry);
        builder.account_data_len(entry);
        builder.account_is_empty(entry);
        let offset = builder.const_u64(0);
        builder.read_dynamic(OP_READ_U64, payer, offset);
        assert!(verify_builder(&builder).is_ok());
    }

    /// No CPI lists a fixed account an open names writable, wherever its invoke sits: after the
    /// open, the entry's borrow mark fails that CPI with `RegistryReentry` every time. The open
    /// reports it. Passing the entry read-only is fine, and so is a writable row account or group
    /// member, which only the run can tell apart from the entry.
    #[test]
    fn no_cpi_passes_an_entry_account_writable() {
        let invalid = TemplateError::InvalidRegistry;
        // A CPI to an executable account, passing `accounts`, forwarding `group`.
        let cpi = |builder: &mut ProgramBuilder, accounts: &[(u8, u8)], group: u8| {
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([9; 32]), None, 0);
            let data = builder.blob(&[1]);
            builder.cpi_with_group(program, accounts, &[Segment::Literal(data)], group)
        };

        // After the open, at the root and in a loop body.
        let (mut builder, _, entry, _, open) = registry_program();
        let passes = cpi(&mut builder, &[(entry, ACCOUNT_WRITABLE)], NO_INDEX);
        builder.invoke(passes, None);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(open)), "after the open");
        let (mut builder, _, entry, payer, open) = registry_program();
        let passes = cpi(&mut builder, &[(payer, ACCOUNT_WRITABLE), (entry, ACCOUNT_WRITABLE)], NO_INDEX);
        let count = builder.const_u64(1);
        builder.repeat(count, 1, 0, |body| body.invoke(passes, None));
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(open)), "in a loop body");

        // Before the open.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let passes = cpi(&mut builder, &[(entry, ACCOUNT_WRITABLE)], NO_INDEX);
        builder.invoke(passes, None);
        let open = builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(open)), "before the open");

        // A CPI that nothing invokes is refused as well: the open scans the CPI account records.
        let (mut builder, _, entry, _, open) = registry_program();
        cpi(&mut builder, &[(entry, ACCOUNT_WRITABLE)], NO_INDEX);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(open)), "never invoked");

        // Read-only, and another fixed account writable.
        let (mut builder, _, entry, payer, _) = registry_program();
        let read_only = cpi(&mut builder, &[(entry, 0), (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)], NO_INDEX);
        builder.invoke(read_only, None);
        assert!(verify_builder(&builder).is_ok());

        // A writable row account, and a forwarded group, may turn out to be the entry at run time.
        let (mut builder, _, _, _, _) = registry_program();
        builder.account_groups(1);
        let row = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(2, 0);
        let rows = cpi(&mut builder, &[(row, ACCOUNT_WRITABLE)], 0);
        builder.for_each(0, |body| body.invoke(rows, None));
        assert!(verify_builder(&builder).is_ok());
    }
}
