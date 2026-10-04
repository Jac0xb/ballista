//! Register reuse for a template whose values need more than 64 registers: a port of the
//! TypeScript compiler's `reuseRegisters` and the functions it calls. A value takes the lowest
//! register free when it is written; the renumbered program is then replayed against the original
//! on every path through its loops, and every read must see the value it saw before.

use std::collections::HashMap;

use ballista_common::template::*;
use ballista_common::template::{
    DATA_LITERAL, INSTRUCTION_FLAG_DYNAMIC_OFFSET, NO_INDEX, OP_CREATE_PDA, OP_DERIVE_PDA, OP_EMIT,
    OP_FOREACH, OP_INVOKE, OP_REPEAT, OP_SET_RETURN_DATA,
};

use super::compile::SourceMapEntry;
use super::CompileError;

/// One 16-byte instruction record.
pub(crate) type Record = [u8; 16];

/// A compiled program, as register reuse reads it.
pub(crate) struct Program {
    pub instructions: Vec<Record>,
    pub cpis: Vec<[u8; 12]>,
    pub segments: Vec<[u8; 8]>,
    pub loops: Vec<LoopSpan>,
}

/// A loop: its record's program counter, the last instruction of its body, and what it carries.
#[derive(Clone)]
pub(crate) struct LoopSpan {
    pc: usize,
    last: usize,
    carried: Vec<usize>,
}

const A: usize = 2;
const B: usize = 3;
const C: usize = 4;

/// The operands among `a`, `b` and `c` that each opcode reads as registers, as `verify.rs` reads
/// them. An unknown opcode is a compiler bug.
fn register_operands(code: u8) -> &'static [usize] {
    match code {
        OP_LOAD_INPUT | OP_CONST_BOOL | OP_CONST_U64 | OP_CONST_I64 | OP_CONST_U128
        | OP_CONST_PUBKEY | OP_CONST_BYTES | OP_ACCOUNT_KEY | OP_ACCOUNT_OWNER
        | OP_ACCOUNT_LAMPORTS | OP_ACCOUNT_DATA_LEN | OP_ACCOUNT_IS_EMPTY | OP_CLOCK_SLOT
        | OP_CLOCK_TIMESTAMP | OP_LOOP_INDEX | OP_FOREACH | OP_DERIVE_PDA | OP_RETURN_DATA
        | OP_EMIT | OP_SET_RETURN_DATA | OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX
        | OP_READ_REGISTRY | OP_GROUP_LENGTH | OP_GROUP_ANY | OP_GROUP_COUNT => &[],
        OP_READ_U64 | OP_READ_I64 | OP_READ_U128 | OP_READ_PUBKEY | OP_READ_U8 | OP_READ_U16
        | OP_READ_U32 | OP_READ_BOOL | OP_READ_I32 => &[B],
        OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE
        | OP_AND | OP_OR | OP_MIN | OP_MAX | OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR
        | OP_BIT_XOR => &[A, B],
        OP_NOT | OP_CAST_U64 | OP_CAST_I64 | OP_CAST_U128 | OP_REQUIRE | OP_MOVE | OP_POW10
        | OP_BYTES_LEN | OP_WRITE_REGISTRY => &[A],
        OP_SELECT | OP_MUL_DIV | OP_MUL_DIV_CEIL => &[A, B, C],
        OP_INVOKE
        | OP_CREATE_PDA
        | OP_REPEAT
        | OP_INSTRUCTION_PROGRAM
        | OP_INSTRUCTION_ACCOUNT_COUNT
        | OP_INSTRUCTION_DATA_LEN
        | OP_READ_ACCOUNT_BYTES
        | OP_OPEN_REGISTRY => &[B],
        OP_INSTRUCTION_ACCOUNT
        | OP_INSTRUCTION_ACCOUNT_FLAGS
        | OP_READ_INSTRUCTION_DATA
        | OP_READ_INSTRUCTION_BYTES => &[B, C],
        other => panic!("Register reuse does not know opcode {other}"),
    }
}

fn is_read_opcode(code: u8) -> bool {
    matches!(
        code,
        OP_READ_BOOL
            | OP_READ_U8
            | OP_READ_U16
            | OP_READ_U32
            | OP_READ_I32
            | OP_READ_U64
            | OP_READ_I64
            | OP_READ_U128
            | OP_READ_PUBKEY
    )
}

/// The operands of `record` that name registers it reads.
fn read_operands(record: &Record) -> &'static [usize] {
    if is_read_opcode(record[0]) && record[5] & INSTRUCTION_FLAG_DYNAMIC_OFFSET == 0 {
        return &[];
    }
    register_operands(record[0])
}

fn immediate(record: &Record) -> u64 {
    u64::from_le_bytes(record[6..14].try_into().expect("8 bytes"))
}

/// The data segments `record` reads when it runs, as `[start, end)`.
fn segment_range(program: &Program, record: &Record) -> Option<(usize, usize)> {
    let code = record[0];
    if code == OP_INVOKE {
        let descriptor = &program.cpis[record[A] as usize];
        let start = u16::from_le_bytes([descriptor[6], descriptor[7]]) as usize;
        return Some((start, start + descriptor[5] as usize));
    }
    if matches!(code, OP_GROUP_ANY | OP_GROUP_COUNT) {
        return Some(GroupScan::decode(immediate(record)).segment_range());
    }
    if matches!(
        code,
        OP_DERIVE_PDA | OP_CREATE_PDA | OP_EMIT | OP_SET_RETURN_DATA
    ) {
        let immediate = immediate(record);
        let start = (immediate & 0xffff_ffff) as usize;
        return Some((start, start + (immediate >> 32) as usize));
    }
    None
}

/// The registers `record` reads, through its operands and its data segments, and the one it
/// writes.
fn register_traffic(program: &Program, record: &Record) -> (Vec<usize>, Option<usize>) {
    let mut reads: Vec<usize> = read_operands(record)
        .iter()
        .map(|&operand| record[operand])
        .filter(|&register| register != NO_INDEX)
        .map(usize::from)
        .collect();
    if let Some((start, end)) = segment_range(program, record) {
        for segment in &program.segments[start..end] {
            if segment[0] != DATA_LITERAL {
                reads.push(segment[1].into());
            }
        }
    }
    let write = (record[1] != NO_INDEX).then_some(record[1] as usize);
    (reads, write)
}

/// Each loop in `instructions`, with the registers `carries` says it carries.
pub(crate) fn loop_spans(
    instructions: &[Record],
    carries: &HashMap<usize, Vec<usize>>,
) -> Vec<LoopSpan> {
    let mut loops = Vec::new();
    let mut pc = 0;
    while pc < instructions.len() {
        let record = &instructions[pc];
        if record[0] == OP_FOREACH || record[0] == OP_REPEAT {
            let body_length = record[A] as usize;
            loops.push(LoopSpan {
                pc,
                last: pc + body_length,
                carried: carries.get(&pc).cloned().unwrap_or_default(),
            });
            pc += body_length;
        }
        pc += 1;
    }
    loops
}

/// Each register's live range: the first and last program counter that writes or reads it, both
/// inclusive. A register nothing touches keeps `i64::MAX` and -1, the TypeScript compiler's
/// `Infinity` and -1. A value from before a loop that the loop reads or carries, or that code
/// after it reads, keeps its register through the whole loop.
pub(crate) fn live_ranges(program: &Program, count: usize) -> (Vec<i64>, Vec<i64>) {
    let mut first = vec![i64::MAX; count];
    let mut last = vec![-1i64; count];
    let touch = |first: &mut Vec<i64>, last: &mut Vec<i64>, register: usize, pc: usize| {
        first[register] = first[register].min(pc as i64);
        last[register] = last[register].max(pc as i64);
    };
    for (pc, record) in program.instructions.iter().enumerate() {
        let (reads, write) = register_traffic(program, record);
        for register in reads {
            touch(&mut first, &mut last, register, pc);
        }
        if let Some(register) = write {
            touch(&mut first, &mut last, register, pc);
        }
    }
    for span in &program.loops {
        for &register in &span.carried {
            touch(&mut first, &mut last, register, span.pc);
            touch(&mut first, &mut last, register, span.last);
        }
        for register in 0..count {
            if first[register] < span.pc as i64 && last[register] > span.pc as i64 {
                last[register] = last[register].max(span.last as i64);
            }
        }
    }
    (first, last)
}

/// The register each value takes, in the order the values are first written: the lowest one
/// whose earlier values were all last read before this value is written.
pub(crate) fn assign_registers(first: &[i64], last: &[i64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..first.len()).collect();
    order.sort_by(|&x, &y| {
        // `Infinity - Infinity` is NaN, which the TypeScript comparator treats as a tie.
        if first[x] == i64::MAX && first[y] == i64::MAX {
            x.cmp(&y)
        } else {
            first[x].cmp(&first[y]).then(x.cmp(&y))
        }
    });
    let mut busy_until: Vec<i64> = Vec::new();
    let mut assigned = vec![NO_INDEX as usize; first.len()];
    for register in order {
        let target = busy_until
            .iter()
            .position(|&end| end < first[register])
            .unwrap_or(busy_until.len());
        if target == busy_until.len() {
            busy_until.push(last[register]);
        } else {
            busy_until[target] = last[register];
        }
        assigned[register] = target;
    }
    assigned
}

/// The program counter where the most values are in use at once, preferring the first in a step
/// over one where inputs and constants load, so the error names a step the author can change.
pub(crate) fn busiest_instruction(
    first: &[i64],
    last: &[i64],
    source_map: &[SourceMapEntry],
) -> usize {
    let mut in_use = vec![0usize; source_map.len()];
    for (register, &start) in first.iter().enumerate() {
        let mut pc = start;
        while pc <= last[register] {
            in_use[pc as usize] += 1;
            pc += 1;
        }
    }
    let most = in_use.iter().copied().max().unwrap_or(0);
    let peaks: Vec<usize> = (0..in_use.len()).filter(|&pc| in_use[pc] == most).collect();
    peaks
        .iter()
        .copied()
        .find(|&pc| source_map[pc].path.starts_with("steps"))
        .unwrap_or(peaks[0])
}

/// `program` with every register renamed as `assigned` says, carry masks included.
pub(crate) fn rename_registers(
    program: &Program,
    assigned: &[usize],
) -> Result<Program, CompileError> {
    let rename = |register: u8| -> Result<u8, CompileError> {
        match assigned.get(register as usize) {
            Some(&target) if target != NO_INDEX as usize => Ok(target as u8),
            _ => Err(CompileError::new(format!(
                "Register reuse has no register for {register}"
            ))),
        }
    };
    let mut instructions = Vec::with_capacity(program.instructions.len());
    for record in &program.instructions {
        let mut renamed = *record;
        if renamed[1] != NO_INDEX {
            renamed[1] = rename(renamed[1])?;
        }
        for &operand in read_operands(record) {
            if renamed[operand] != NO_INDEX {
                renamed[operand] = rename(renamed[operand])?;
            }
        }
        instructions.push(renamed);
    }
    let mut loops = Vec::with_capacity(program.loops.len());
    for span in &program.loops {
        let carried = span
            .carried
            .iter()
            .map(|&register| rename(register as u8).map(usize::from))
            .collect::<Result<Vec<_>, _>>()?;
        let carry = carried.iter().fold(0u64, |mask, &register| {
            mask | 1u64.checked_shl(register as u32).unwrap_or(0)
        });
        instructions[span.pc][6..14].copy_from_slice(&carry.to_le_bytes());
        loops.push(LoopSpan {
            pc: span.pc,
            last: span.last,
            carried,
        });
    }
    let mut segments = Vec::with_capacity(program.segments.len());
    for segment in &program.segments {
        let mut renamed = *segment;
        if renamed[0] != DATA_LITERAL {
            renamed[1] = rename(renamed[1])?;
        }
        segments.push(renamed);
    }
    Ok(Program {
        instructions,
        cpis: program.cpis.clone(),
        segments,
        loops,
    })
}

/// What every register read sees, in order, on one path through the loops: bit `n` of `runs`
/// set makes loop `n` run, clear makes it run no passes. A value is named by the program counter
/// of the instruction that wrote it, or -1 before any write.
pub(crate) fn values_read(program: &Program, runs: usize) -> Vec<i64> {
    let mut seen = Vec::new();
    let mut values = vec![-1i64; NO_INDEX as usize];
    let run = |pc: usize, values: &mut Vec<i64>, seen: &mut Vec<i64>| {
        let (reads, write) = register_traffic(program, &program.instructions[pc]);
        for register in reads {
            seen.push(values[register]);
        }
        if let Some(register) = write {
            values[register] = pc as i64;
        }
    };
    let mut pc = 0;
    for (index, span) in program.loops.iter().enumerate() {
        while pc < span.pc {
            run(pc, &mut values, &mut seen);
            pc += 1;
        }
        run(span.pc, &mut values, &mut seen);
        for &register in &span.carried {
            seen.push(values[register]);
        }
        if (runs >> index) & 1 == 1 {
            let mut snapshot = values.clone();
            for _ in 0..2 {
                for body in span.pc + 1..=span.last {
                    run(body, &mut values, &mut seen);
                }
                for &register in &span.carried {
                    seen.push(values[register]);
                    snapshot[register] = values[register];
                }
                values = snapshot.clone();
            }
        }
        pc = span.last + 1;
    }
    while pc < program.instructions.len() {
        run(pc, &mut values, &mut seen);
        pc += 1;
    }
    seen
}
