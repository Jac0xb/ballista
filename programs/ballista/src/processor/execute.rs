use ballista_common::template::*;
use pinocchio::{
    cpi::invoke_with_bounds,
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{clock::Clock, Sysvar},
    AccountView, ProgramResult,
};
use solana_address::Address;

use crate::error::BallistaError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeValue<'data> {
    Unset,
    Bool(bool),
    U64(u64),
    I64(i64),
    U128([u8; 16]),
    Pubkey([u8; 32]),
    Bytes(&'data [u8]),
}

pub fn run<'data>(
    program: &ProgramView<'data>,
    input_bytes: &'data [u8],
    runtime_accounts: &'data mut [AccountView],
) -> ProgramResult {
    let inputs = parse_inputs(program.inputs, input_bytes)?;
    let iterations = validate_runtime_accounts(program, runtime_accounts)?;
    let mut registers = vec![RuntimeValue::Unset; MAX_REGISTERS];
    execute_root(
        program,
        &inputs,
        runtime_accounts,
        iterations,
        &mut registers,
    )
}

fn parse_inputs<'data>(
    descriptors: &[InputDescriptor],
    mut data: &'data [u8],
) -> Result<Vec<RuntimeValue<'data>>, ProgramError> {
    let mut inputs = vec![RuntimeValue::Unset; descriptors.len()];
    for (index, descriptor) in descriptors.iter().enumerate() {
        let value = match descriptor.value_type {
            VALUE_BOOL => {
                let (value, remaining) = take::<1>(data)?;
                data = remaining;
                match value[0] {
                    0 => RuntimeValue::Bool(false),
                    1 => RuntimeValue::Bool(true),
                    _ => return Err(BallistaError::InvalidRunInputs.into()),
                }
            }
            VALUE_U64 => {
                let (value, remaining) = take::<8>(data)?;
                data = remaining;
                RuntimeValue::U64(u64::from_le_bytes(*value))
            }
            VALUE_I64 => {
                let (value, remaining) = take::<8>(data)?;
                data = remaining;
                RuntimeValue::I64(i64::from_le_bytes(*value))
            }
            VALUE_U128 => {
                let (value, remaining) = take::<16>(data)?;
                data = remaining;
                RuntimeValue::U128(*value)
            }
            VALUE_PUBKEY => {
                let (value, remaining) = take::<32>(data)?;
                data = remaining;
                RuntimeValue::Pubkey(*value)
            }
            VALUE_BYTES => {
                let (len, remaining) = take::<2>(data)?;
                let len = u16::from_le_bytes(*len) as usize;
                let (bytes, remaining) = remaining
                    .split_at_checked(len)
                    .ok_or(BallistaError::InvalidRunInputs)?;
                if len > descriptor.max_len() {
                    return Err(BallistaError::InvalidRunInputs.into());
                }
                data = remaining;
                RuntimeValue::Bytes(bytes)
            }
            _ => return Err(BallistaError::InvalidRunInputs.into()),
        };
        inputs[index] = value;
    }
    if !data.is_empty() {
        return Err(BallistaError::InvalidRunInputs.into());
    }
    Ok(inputs)
}

fn validate_runtime_accounts(
    program: &ProgramView<'_>,
    accounts: &[AccountView],
) -> Result<usize, ProgramError> {
    if accounts.len() > MAX_RUNTIME_ACCOUNTS {
        return Err(BallistaError::InvalidAccountRange.into());
    }
    let fixed = program.header.fixed_account_count();
    let stride = program.header.batch_stride();
    let iterations = if stride == 0 {
        if accounts.len() != fixed {
            return Err(BallistaError::InvalidAccountRange.into());
        }
        0
    } else {
        let tail = accounts
            .len()
            .checked_sub(fixed)
            .ok_or(BallistaError::InvalidAccountRange)?;
        if tail % stride != 0 {
            return Err(BallistaError::InvalidAccountRange.into());
        }
        let iterations = tail / stride;
        if iterations > program.header.batch_max_iterations() {
            return Err(BallistaError::InvalidAccountRange.into());
        }
        iterations
    };

    for (index, account) in accounts.iter().take(fixed).enumerate() {
        validate_account(program, account, &program.accounts[index])?;
    }
    for iteration in 0..iterations {
        for offset in 0..stride {
            let account_index = fixed + iteration * stride + offset;
            validate_account(
                program,
                &accounts[account_index],
                &program.accounts[fixed + offset],
            )?;
        }
    }
    Ok(iterations)
}

fn validate_account(
    program: &ProgramView<'_>,
    account: &AccountView,
    constraint: &AccountConstraint,
) -> ProgramResult {
    if constraint.flags & ACCOUNT_SIGNER != 0 && !account.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if constraint.flags & ACCOUNT_WRITABLE != 0 && !account.is_writable() {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    if constraint.flags & ACCOUNT_EXECUTABLE != 0 && !account.executable() {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    if constraint.address_index != NO_INDEX
        && account.address().as_ref()
            != program.pubkeys[constraint.address_index as usize]
                .bytes
                .as_slice()
    {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    if constraint.owner_index != NO_INDEX
        && account.owner().as_ref()
            != program.pubkeys[constraint.owner_index as usize]
                .bytes
                .as_slice()
    {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    if account.data_len() < constraint.min_data_len() {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    Ok(())
}

fn execute_root<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    iterations: usize,
    registers: &mut [RuntimeValue<'data>],
) -> ProgramResult {
    let mut pc = 0usize;
    while pc < program.instructions.len() {
        let instruction = &program.instructions[pc];
        if instruction.opcode == OP_FOREACH {
            let body_start = pc + 1;
            let body_end = body_start + instruction.a as usize;
            let base_registers = registers.to_vec();
            for iteration in 0..iterations {
                registers.copy_from_slice(&base_registers);
                let row_base = program.header.fixed_account_count()
                    + iteration * program.header.batch_stride();
                execute_range(
                    program,
                    inputs,
                    accounts,
                    registers,
                    body_start,
                    body_end,
                    Some((iteration, row_base)),
                )?;
            }
            registers.copy_from_slice(&base_registers);
            pc = body_end;
            continue;
        }
        execute_instruction(program, inputs, accounts, registers, instruction, None)?;
        pc += 1;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_range<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    registers: &mut [RuntimeValue<'data>],
    start: usize,
    end: usize,
    loop_context: Option<(usize, usize)>,
) -> ProgramResult {
    for instruction in &program.instructions[start..end] {
        if instruction.opcode == OP_FOREACH {
            return Err(BallistaError::InvalidTemplateProgram.into());
        }
        execute_instruction(
            program,
            inputs,
            accounts,
            registers,
            instruction,
            loop_context,
        )?;
    }
    Ok(())
}

fn execute_instruction<'data>(
    program: &ProgramView<'data>,
    inputs: &[RuntimeValue<'data>],
    accounts: &'data [AccountView],
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
    loop_context: Option<(usize, usize)>,
) -> ProgramResult {
    let dst = instruction.dst as usize;
    match instruction.opcode {
        OP_LOAD_INPUT => set(registers, dst, inputs[instruction.a as usize])?,
        OP_CONST_BOOL => set(registers, dst, RuntimeValue::Bool(instruction.a != 0))?,
        OP_CONST_U64 => set(registers, dst, RuntimeValue::U64(instruction.immediate()))?,
        OP_CONST_I64 => set(
            registers,
            dst,
            RuntimeValue::I64(i64::from_le_bytes(instruction.immediate_le)),
        )?,
        OP_CONST_U128 => {
            let (offset, len) = instruction.blob_range();
            let bytes: &[u8; 16] = program.blob[offset..offset + len]
                .try_into()
                .map_err(|_| BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, RuntimeValue::U128(*bytes))?;
        }
        OP_CONST_PUBKEY => set(
            registers,
            dst,
            RuntimeValue::Pubkey(program.pubkeys[instruction.a as usize].bytes),
        )?,
        OP_CONST_BYTES => {
            let (offset, len) = instruction.blob_range();
            set(
                registers,
                dst,
                RuntimeValue::Bytes(&program.blob[offset..offset + len]),
            )?;
        }
        OP_ACCOUNT_KEY => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(
                registers,
                dst,
                RuntimeValue::Pubkey(account.address().to_bytes()),
            )?;
        }
        OP_ACCOUNT_OWNER => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(
                registers,
                dst,
                RuntimeValue::Pubkey(account.owner().to_bytes()),
            )?;
        }
        OP_ACCOUNT_LAMPORTS => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::U64(account.lamports()))?;
        }
        OP_ACCOUNT_DATA_LEN => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::U64(account.data_len() as u64))?;
        }
        OP_ACCOUNT_IS_EMPTY => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            set(registers, dst, RuntimeValue::Bool(account.is_data_empty()))?;
        }
        OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_U128
        | OP_READ_PUBKEY | OP_READ_BOOL => {
            let account = resolve_account(program, accounts, instruction.a, loop_context)?;
            let data = account.try_borrow()?;
            let offset = instruction.immediate() as usize;
            let value = match instruction.opcode {
                OP_READ_U8 => RuntimeValue::U64(read_array::<1>(&data, offset)?[0] as u64),
                OP_READ_U16 => {
                    RuntimeValue::U64(u16::from_le_bytes(*read_array(&data, offset)?) as u64)
                }
                OP_READ_U32 => {
                    RuntimeValue::U64(u32::from_le_bytes(*read_array(&data, offset)?) as u64)
                }
                OP_READ_U64 => RuntimeValue::U64(u64::from_le_bytes(*read_array(&data, offset)?)),
                OP_READ_I64 => RuntimeValue::I64(i64::from_le_bytes(*read_array(&data, offset)?)),
                OP_READ_U128 => RuntimeValue::U128(*read_array(&data, offset)?),
                OP_READ_PUBKEY => RuntimeValue::Pubkey(*read_array(&data, offset)?),
                OP_READ_BOOL => match read_array::<1>(&data, offset)?[0] {
                    0 => RuntimeValue::Bool(false),
                    1 => RuntimeValue::Bool(true),
                    _ => return Err(BallistaError::TypeMismatch.into()),
                },
                _ => unreachable!(),
            };
            set(registers, dst, value)?;
        }
        OP_CLOCK_SLOT => set(registers, dst, RuntimeValue::U64(Clock::get()?.slot))?,
        OP_CLOCK_TIMESTAMP => set(
            registers,
            dst,
            RuntimeValue::I64(Clock::get()?.unix_timestamp),
        )?,
        OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX => {
            let left = get(registers, instruction.a)?;
            let right = get(registers, instruction.b)?;
            set(registers, dst, arithmetic(instruction.opcode, left, right)?)?;
        }
        OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => {
            let left = get(registers, instruction.a)?;
            let right = get(registers, instruction.b)?;
            let result = compare(instruction.opcode, left, right)?;
            set(registers, dst, RuntimeValue::Bool(result))?;
        }
        OP_AND | OP_OR => {
            let left = as_bool(get(registers, instruction.a)?)?;
            let right = as_bool(get(registers, instruction.b)?)?;
            set(
                registers,
                dst,
                RuntimeValue::Bool(if instruction.opcode == OP_AND {
                    left && right
                } else {
                    left || right
                }),
            )?;
        }
        OP_NOT => set(
            registers,
            dst,
            RuntimeValue::Bool(!as_bool(get(registers, instruction.a)?)?),
        )?,
        OP_SELECT => {
            let condition = as_bool(get(registers, instruction.a)?)?;
            let selected = if condition {
                get(registers, instruction.b)?
            } else {
                get(registers, instruction.c)?
            };
            set(registers, dst, selected)?;
        }
        OP_CAST_U64 | OP_CAST_I64 | OP_CAST_U128 => {
            let value = cast(instruction.opcode, get(registers, instruction.a)?)?;
            set(registers, dst, value)?;
        }
        OP_LOOP_INDEX => {
            let (iteration, _) = loop_context.ok_or(BallistaError::InvalidTemplateProgram)?;
            set(registers, dst, RuntimeValue::U64(iteration as u64))?;
        }
        OP_DERIVE_PDA => {
            let program_account = resolve_account(program, accounts, instruction.a, loop_context)?;
            let (segment_start, segment_len) = instruction.blob_range();
            let mut seeds = Vec::with_capacity(segment_len);
            for segment in &program.data_segments[segment_start..segment_start + segment_len] {
                let mut seed = Vec::with_capacity(MAX_PDA_SEED_LEN);
                append_segment(program, registers, segment, &mut seed)?;
                if seed.len() > MAX_PDA_SEED_LEN {
                    return Err(BallistaError::InvalidPdaDerivation.into());
                }
                seeds.push(seed);
            }
            let seed_slices: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
            let (derived, _) = Address::try_find_program_address(
                seed_slices.as_slice(),
                program_account.address(),
            )
            .ok_or(BallistaError::InvalidPdaDerivation)?;
            set(registers, dst, RuntimeValue::Pubkey(derived.to_bytes()))?;
        }
        OP_REQUIRE => {
            if !as_bool(get(registers, instruction.a)?)? {
                return Err(BallistaError::RequirementFailed.into());
            }
        }
        OP_INVOKE => {
            if instruction.b != NO_INDEX && !as_bool(get(registers, instruction.b)?)? {
                return Ok(());
            }
            invoke_cpi(
                program,
                accounts,
                registers,
                instruction.a as usize,
                loop_context,
            )?;
        }
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    }
    Ok(())
}

fn invoke_cpi<'data>(
    program: &ProgramView<'data>,
    accounts: &'data [AccountView],
    registers: &[RuntimeValue<'data>],
    cpi_index: usize,
    loop_context: Option<(usize, usize)>,
) -> ProgramResult {
    let descriptor = &program.cpis[cpi_index];
    let account_end = descriptor.account_start() + descriptor.account_len as usize;
    let segment_end = descriptor.segment_start() + descriptor.segment_len as usize;
    let mut instruction_accounts = Vec::with_capacity(descriptor.account_len as usize);
    let mut account_views = Vec::with_capacity(descriptor.account_len as usize);
    for record in &program.cpi_accounts[descriptor.account_start()..account_end] {
        let account = resolve_account(program, accounts, record.account, loop_context)?;
        instruction_accounts.push(InstructionAccount::new(
            account.address(),
            record.flags & ACCOUNT_WRITABLE != 0,
            record.flags & ACCOUNT_SIGNER != 0,
        ));
        account_views.push(account);
    }

    let mut data = Vec::with_capacity(descriptor.max_data_len());
    for segment in &program.data_segments[descriptor.segment_start()..segment_end] {
        append_segment(program, registers, segment, &mut data)?;
    }
    if data.len() > descriptor.max_data_len() || data.len() > MAX_CPI_DATA_LEN {
        return Err(BallistaError::CpiDataTooLarge.into());
    }

    let program_account =
        resolve_account(program, accounts, descriptor.program_account, loop_context)?;
    let instruction = InstructionView {
        program_id: program_account.address(),
        accounts: instruction_accounts.as_slice(),
        data: data.as_slice(),
    };
    invoke_with_bounds::<64, _>(&instruction, account_views.as_slice())
}

fn append_segment<'data>(
    program: &ProgramView<'data>,
    registers: &[RuntimeValue<'data>],
    segment: &DataSegment,
    output: &mut Vec<u8>,
) -> ProgramResult {
    match segment.kind {
        DATA_LITERAL => output
            .extend_from_slice(&program.blob[segment.offset()..segment.offset() + segment.len()]),
        DATA_REG_U8 => {
            let value = as_u128(get(registers, segment.register)?)?;
            output.push(u8::try_from(value).map_err(|_| BallistaError::ArithmeticOverflow)?);
        }
        DATA_REG_U16 => {
            let value = u16::try_from(as_u128(get(registers, segment.register)?)?)
                .map_err(|_| BallistaError::ArithmeticOverflow)?;
            output.extend_from_slice(&value.to_le_bytes());
        }
        DATA_REG_U32 => {
            let value = u32::try_from(as_u128(get(registers, segment.register)?)?)
                .map_err(|_| BallistaError::ArithmeticOverflow)?;
            output.extend_from_slice(&value.to_le_bytes());
        }
        DATA_REG_U64 => {
            let value = u64::try_from(as_u128(get(registers, segment.register)?)?)
                .map_err(|_| BallistaError::ArithmeticOverflow)?;
            output.extend_from_slice(&value.to_le_bytes());
        }
        DATA_REG_I64 => match get(registers, segment.register)? {
            RuntimeValue::I64(value) => output.extend_from_slice(&value.to_le_bytes()),
            _ => return Err(BallistaError::TypeMismatch.into()),
        },
        DATA_REG_U128 => match get(registers, segment.register)? {
            RuntimeValue::U128(value) => output.extend_from_slice(&value),
            _ => return Err(BallistaError::TypeMismatch.into()),
        },
        DATA_REG_PUBKEY => match get(registers, segment.register)? {
            RuntimeValue::Pubkey(value) => output.extend_from_slice(&value),
            _ => return Err(BallistaError::TypeMismatch.into()),
        },
        DATA_REG_BOOL => output.push(u8::from(as_bool(get(registers, segment.register)?)?)),
        DATA_REG_BYTES => match get(registers, segment.register)? {
            RuntimeValue::Bytes(value) => output.extend_from_slice(value),
            _ => return Err(BallistaError::TypeMismatch.into()),
        },
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    }
    Ok(())
}

fn resolve_account<'data>(
    program: &ProgramView<'_>,
    accounts: &'data [AccountView],
    reference: u8,
    loop_context: Option<(usize, usize)>,
) -> Result<&'data AccountView, ProgramError> {
    let index = if reference & ITERATION_ACCOUNT_BIT == 0 {
        let index = reference as usize;
        if index >= program.header.fixed_account_count() {
            return Err(BallistaError::InvalidRuntimeAccount.into());
        }
        index
    } else {
        let (_, row_base) = loop_context.ok_or(BallistaError::InvalidRuntimeAccount)?;
        let offset = (reference & !ITERATION_ACCOUNT_BIT) as usize;
        if offset >= program.header.batch_stride() {
            return Err(BallistaError::InvalidRuntimeAccount.into());
        }
        row_base + offset
    };
    accounts
        .get(index)
        .ok_or_else(|| BallistaError::InvalidRuntimeAccount.into())
}

fn arithmetic<'data>(
    opcode: u8,
    left: RuntimeValue<'data>,
    right: RuntimeValue<'data>,
) -> Result<RuntimeValue<'data>, ProgramError> {
    macro_rules! checked {
        ($left:expr, $right:expr) => {{
            let value = match opcode {
                OP_ADD => $left.checked_add($right),
                OP_SUB => $left.checked_sub($right),
                OP_MUL => $left.checked_mul($right),
                OP_DIV => $left.checked_div($right),
                OP_MIN => Some($left.min($right)),
                OP_MAX => Some($left.max($right)),
                _ => None,
            };
            value.ok_or_else(|| {
                ProgramError::from(if opcode == OP_DIV && $right == 0 {
                    BallistaError::DivisionByZero
                } else {
                    BallistaError::ArithmeticOverflow
                })
            })?
        }};
    }
    Ok(match (left, right) {
        (RuntimeValue::U64(left), RuntimeValue::U64(right)) => {
            RuntimeValue::U64(checked!(left, right))
        }
        (RuntimeValue::I64(left), RuntimeValue::I64(right)) => {
            RuntimeValue::I64(checked!(left, right))
        }
        (RuntimeValue::U128(left), RuntimeValue::U128(right)) => {
            let left = u128::from_le_bytes(left);
            let right = u128::from_le_bytes(right);
            RuntimeValue::U128(checked!(left, right).to_le_bytes())
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

fn compare(
    opcode: u8,
    left: RuntimeValue<'_>,
    right: RuntimeValue<'_>,
) -> Result<bool, ProgramError> {
    macro_rules! compare_values {
        ($left:expr, $right:expr) => {
            match opcode {
                OP_EQ => $left == $right,
                OP_NE => $left != $right,
                OP_LT => $left < $right,
                OP_LTE => $left <= $right,
                OP_GT => $left > $right,
                OP_GTE => $left >= $right,
                _ => return Err(BallistaError::InvalidTemplateProgram.into()),
            }
        };
    }
    Ok(match (left, right) {
        (RuntimeValue::Bool(left), RuntimeValue::Bool(right)) => {
            if !matches!(opcode, OP_EQ | OP_NE) {
                return Err(BallistaError::TypeMismatch.into());
            }
            compare_values!(left, right)
        }
        (RuntimeValue::U64(left), RuntimeValue::U64(right)) => compare_values!(left, right),
        (RuntimeValue::I64(left), RuntimeValue::I64(right)) => compare_values!(left, right),
        (RuntimeValue::U128(left), RuntimeValue::U128(right)) => {
            compare_values!(u128::from_le_bytes(left), u128::from_le_bytes(right))
        }
        (RuntimeValue::Pubkey(left), RuntimeValue::Pubkey(right)) => {
            if !matches!(opcode, OP_EQ | OP_NE) {
                return Err(BallistaError::TypeMismatch.into());
            }
            compare_values!(left, right)
        }
        (RuntimeValue::Bytes(left), RuntimeValue::Bytes(right)) => {
            if !matches!(opcode, OP_EQ | OP_NE) {
                return Err(BallistaError::TypeMismatch.into());
            }
            compare_values!(left, right)
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

fn cast(opcode: u8, value: RuntimeValue<'_>) -> Result<RuntimeValue<'_>, ProgramError> {
    match opcode {
        OP_CAST_U64 => Ok(RuntimeValue::U64(match value {
            RuntimeValue::U64(value) => value,
            RuntimeValue::I64(value) => {
                u64::try_from(value).map_err(|_| BallistaError::ArithmeticOverflow)?
            }
            RuntimeValue::U128(value) => u64::try_from(u128::from_le_bytes(value))
                .map_err(|_| BallistaError::ArithmeticOverflow)?,
            _ => return Err(BallistaError::TypeMismatch.into()),
        })),
        OP_CAST_I64 => Ok(RuntimeValue::I64(match value {
            RuntimeValue::U64(value) => {
                i64::try_from(value).map_err(|_| BallistaError::ArithmeticOverflow)?
            }
            RuntimeValue::I64(value) => value,
            RuntimeValue::U128(value) => i64::try_from(u128::from_le_bytes(value))
                .map_err(|_| BallistaError::ArithmeticOverflow)?,
            _ => return Err(BallistaError::TypeMismatch.into()),
        })),
        OP_CAST_U128 => Ok(RuntimeValue::U128(match value {
            RuntimeValue::U64(value) => (value as u128).to_le_bytes(),
            RuntimeValue::I64(value) => u128::try_from(value)
                .map_err(|_| BallistaError::ArithmeticOverflow)?
                .to_le_bytes(),
            RuntimeValue::U128(value) => value,
            _ => return Err(BallistaError::TypeMismatch.into()),
        })),
        _ => Err(BallistaError::InvalidTemplateProgram.into()),
    }
}

fn set<'data>(
    registers: &mut [RuntimeValue<'data>],
    index: usize,
    value: RuntimeValue<'data>,
) -> ProgramResult {
    let register = registers
        .get_mut(index)
        .ok_or(BallistaError::InvalidRegister)?;
    *register = value;
    Ok(())
}

fn get<'data>(
    registers: &[RuntimeValue<'data>],
    index: u8,
) -> Result<RuntimeValue<'data>, ProgramError> {
    match registers.get(index as usize).copied() {
        Some(RuntimeValue::Unset) | None => Err(BallistaError::InvalidRegister.into()),
        Some(value) => Ok(value),
    }
}

fn as_bool(value: RuntimeValue<'_>) -> Result<bool, ProgramError> {
    match value {
        RuntimeValue::Bool(value) => Ok(value),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

fn as_u128(value: RuntimeValue<'_>) -> Result<u128, ProgramError> {
    match value {
        RuntimeValue::U64(value) => Ok(value as u128),
        RuntimeValue::U128(value) => Ok(u128::from_le_bytes(value)),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

fn read_array<const N: usize>(data: &[u8], offset: usize) -> Result<&[u8; N], ProgramError> {
    offset
        .checked_add(N)
        .and_then(|end| data.get(offset..end))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| BallistaError::InvalidRuntimeAccount.into())
}

fn take<const N: usize>(data: &[u8]) -> Result<(&[u8; N], &[u8]), ProgramError> {
    let (bytes, remaining) = data
        .split_at_checked(N)
        .ok_or(BallistaError::InvalidRunInputs)?;
    Ok((
        bytes
            .try_into()
            .map_err(|_| BallistaError::InvalidRunInputs)?,
        remaining,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_value_storage_is_bounded() {
        let value_size = core::mem::size_of::<RuntimeValue<'static>>();
        assert!(value_size <= 40);
        eprintln!(
            "runtime value: {value_size} bytes; register file: {} bytes; loop snapshot: {} bytes",
            value_size * MAX_REGISTERS,
            value_size * MAX_REGISTERS,
        );
    }
}
