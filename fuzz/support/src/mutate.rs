//! Structure-aware mutations of a decoded payload. Each one changes a field, a record or a loop in
//! a way a byte flip rarely reaches, and keeps the header's counts in step with the sections, so
//! the result still parses and the verifier has to judge it on its rules.

use arbitrary::Unstructured;

use crate::{
    checker::{op, GroupFilter, INSTRUCTIONS_SYSVAR, NONE, ROW_BIT, SYSTEM_PROGRAM},
    model::{self, Account, Cpi, CpiAccount, Instr, Program, Segment},
};

/// A stream of choices: libFuzzer's seed in the custom mutator, the input bytes in `structured`.
pub trait Source {
    fn word(&mut self) -> u64;

    /// A number below `bound`, or 0 when `bound` is 0.
    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.word() % bound as u64) as usize
        }
    }

    fn byte(&mut self) -> u8 {
        self.word() as u8
    }

    /// True `numerator` times in `denominator`.
    fn chance(&mut self, numerator: usize, denominator: usize) -> bool {
        self.below(denominator) < numerator
    }

    fn pick<T: Copy>(&mut self, options: &[T]) -> Option<T> {
        (!options.is_empty()).then(|| options[self.below(options.len())])
    }
}

/// xorshift64*, seeded by libFuzzer.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15 | 1)
    }
}

impl Source for Rng {
    fn word(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

/// The input bytes, read through `arbitrary`. Zero once they run out, so every stream ends.
pub struct Input<'a, 'b>(pub &'b mut Unstructured<'a>);

impl Source for Input<'_, '_> {
    fn word(&mut self) -> u64 {
        if self.0.is_empty() {
            return 0;
        }
        self.0.arbitrary::<u64>().unwrap_or(0)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound <= 1 || self.0.is_empty() {
            return 0;
        }
        self.0.int_in_range(0..=bound - 1).unwrap_or(0)
    }

    fn byte(&mut self) -> u8 {
        self.0.arbitrary::<u8>().unwrap_or(0)
    }
}

/// Section sizes the mutators keep below, so a payload stays near the 10,240-byte cap and the
/// header's one- and two-byte counts never wrap.
const MAX_ACCOUNTS: usize = 140;
const MAX_INSTRS: usize = 140;
const MAX_TABLE: usize = 400;
const MAX_PUBKEYS: usize = 40;
const MAX_BLOB: usize = 3_000;

/// Every assigned opcode, plus the unassigned 0, 39 and 81 now and then.
fn opcode(s: &mut impl Source) -> u8 {
    if s.chance(1, 32) {
        return s.pick(&[0, 39, 81, 0xff]).unwrap_or(0);
    }
    let value = 1 + s.below(79) as u8;
    if value >= 39 { value + 1 } else { value }
}

/// A register operand: one in range, the boundary, or none.
pub fn register(p: &Program, s: &mut impl Source) -> u8 {
    let count = p.header.registers as usize;
    match s.below(10) {
        0 => NONE,
        1 => count as u8,
        2 => s.byte(),
        _ => s.below(count.max(1)) as u8,
    }
}

/// An account reference: a fixed account, a row account, or one past either.
pub fn account_ref(p: &Program, s: &mut impl Source) -> u8 {
    let fixed = p.fixed_count();
    let stride = p.stride();
    match s.below(10) {
        0 => NONE,
        1 => fixed as u8,
        2 => ROW_BIT | stride as u8,
        3 => s.byte(),
        4..=5 if stride > 0 => ROW_BIT | s.below(stride) as u8,
        _ => s.below(fixed.max(1)) as u8,
    }
}

/// A packed range into a table of `len` records or bytes.
fn range(len: usize, s: &mut impl Source) -> u64 {
    let start = s.below(len + 1) as u64;
    let count = match s.below(6) {
        0 => 0,
        1 => 16,
        2 => s.below(len + 2) as u64,
        _ => 1 + s.below(3) as u64,
    };
    start | count << 32
}

/// An immediate that means something to some opcode.
pub fn immediate(p: &Program, s: &mut impl Source) -> u64 {
    match s.below(15) {
        0 => s.pick(&[0, 1, 2, 7, 8, 15, 16, 17, 31, 32, 33, 64, 165, 255, 256]).unwrap_or(0),
        1 => s.pick(&[511, 512, 513, 1_023, 1_024, 1_025, 4_095, 4_096, 4_097, 10_240]).unwrap_or(0),
        2 => s.pick(&[u32::MAX as u64, 1 << 32, u64::MAX, i64::MIN as u64, u64::MAX >> 1]).unwrap_or(0),
        3 => range(p.segments.len(), s),
        4 => range(p.blob.len(), s),
        5 => s.pick(&[13u64, 14, 15, 16, 43, 44, 45, 46, 60, 12, 17, 47]).unwrap_or(13),
        // A registry open: index, size, the System program account.
        6 => {
            let index = s.below(9) as u64;
            let size = s.pick(&[0u64, 1, 8, 16, 65, 512, 513]).unwrap_or(8);
            let system = account_ref(p, s) as u64;
            index | size << 8 | system << 24
        }
        // A registry field: offset, read selector.
        7 => {
            let offset = s.pick(&[0u64, 1, 8, 9, 16, 48, 504, 511, 512]).unwrap_or(0);
            let selector = s.pick(&[13u64, 14, 15, 16, 43, 44, 45, 46, 60, 0, 47]).unwrap_or(13);
            offset | selector << 16
        }
        // A carry mask.
        8 => {
            let registers = (p.header.registers as usize).max(1);
            let mut mask = 1u64 << s.below(registers.min(64));
            if s.chance(1, 3) {
                mask |= 1u64 << s.below(64);
            }
            mask
        }
        9 => s.below(64) as u64,
        10 => group_filter(p, s),
        _ => s.word(),
    }
}

/// A group filter's immediate: a run of segments in the table or just past it, 0 to 5 matches and
/// excepts, and a minimum data length around the ends of common matches.
fn group_filter(p: &Program, s: &mut impl Source) -> u64 {
    GroupFilter {
        segment_start: s.below(p.segments.len() + 1) as u16,
        matches: s.below(6) as u8,
        excepts: s.below(6) as u8,
        min_data_len: s.pick(&[0u32, 1, 8, 32, 40, 63, 64, 72, 165]).unwrap_or(0),
    }
    .encode()
}

fn instruction(p: &Program, s: &mut impl Source) -> Instr {
    let op = opcode(s);
    let mut instr = Instr::new(op, register(p, s), 0, 0, 0, immediate(p, s));
    // Fill each operand with what it most often is for this opcode, and sometimes anything.
    let account_first = matches!(
        op,
        op::ACCOUNT_KEY..=op::READ_PUBKEY
            | op::READ_U8..=op::DERIVE_PDA
            | op::CREATE_PDA
            | op::READ_I32
            | op::INSTRUCTION_COUNT..=op::READ_ACCOUNT_BYTES
            | op::OPEN_REGISTRY
            | op::READ_REGISTRY
    );
    instr.a = if account_first { account_ref(p, s) } else { register(p, s) };
    instr.b = if op == op::WRITE_REGISTRY { account_ref(p, s) } else { register(p, s) };
    instr.c = if op == op::OPEN_REGISTRY { account_ref(p, s) } else { register(p, s) };
    match op {
        op::LOAD_INPUT => {
            let inputs = p.header.fixed_inputs as usize;
            instr.a = if s.chance(1, 3) { ROW_BIT | s.below(4) as u8 } else { s.below(inputs + 1) as u8 };
        }
        op::CONST_BOOL => instr.a = s.below(3) as u8,
        op::CONST_PUBKEY => instr.a = s.below(p.pubkeys.len() + 1) as u8,
        op::INVOKE => {
            instr.a = s.below(p.cpis.len() + 1) as u8;
            instr.b = if s.chance(1, 2) { NONE } else { register(p, s) };
        }
        op::RETURN_DATA => instr.a = s.pick(&[13u8, 14, 15, 16, 43, 44, 45, 46, 60, 0]).unwrap_or(13),
        op::FOREACH | op::REPEAT => {
            instr.a = 1 + s.below(4) as u8;
            instr.c = s.pick(&[0u8, 1, 2, 4, 255]).unwrap_or(1);
            instr.dst = if s.chance(3, 4) { NONE } else { register(p, s) };
        }
        op::EMIT | op::SET_RETURN_DATA | op::OPEN_REGISTRY | op::WRITE_REGISTRY | op::REQUIRE => {
            if s.chance(3, 4) {
                instr.dst = NONE;
            }
            if matches!(op, op::EMIT | op::SET_RETURN_DATA) && s.chance(3, 4) {
                instr.a = NONE;
                instr.b = NONE;
                instr.c = NONE;
                instr.imm = range(p.segments.len(), s);
            }
        }
        op::READ_REGISTRY => {
            instr.b = NONE;
            instr.c = NONE;
        }
        // A group, then for a filter one or two programs from the pubkey table.
        op::GROUP_LENGTH | op::GROUP_ANY | op::GROUP_COUNT => {
            instr.a = s.below(p.header.account_groups as usize + 2) as u8;
            if op == op::GROUP_LENGTH {
                if s.chance(3, 4) {
                    instr.b = NONE;
                    instr.c = NONE;
                    instr.imm = 0;
                }
            } else {
                instr.b = s.below(p.pubkeys.len() + 1) as u8;
                instr.c = if s.chance(1, 2) { NONE } else { s.below(p.pubkeys.len() + 1) as u8 };
                if s.chance(3, 4) {
                    instr.imm = group_filter(p, s);
                }
            }
        }
        _ => {}
    }
    if crate::checker::read_opcode(op).is_some() && s.chance(1, 4) {
        instr.flags = 1;
        instr.imm = 0;
    }
    if s.chance(1, 64) {
        instr.flags = s.byte();
    }
    if s.chance(1, 64) {
        instr.reserved = [s.byte(), s.byte()];
    }
    instr
}

/// The loop, if any, whose body holds `pc`: its index and its body's end.
fn enclosing_loop(p: &Program, pc: usize) -> Option<(usize, usize)> {
    let mut at = 0;
    while at < p.instrs.len() {
        let instr = p.instrs[at];
        if matches!(instr.op, op::FOREACH | op::REPEAT) {
            let end = at + 1 + instr.a as usize;
            if pc > at && pc < end {
                return Some((at, end));
            }
            at = end.max(at + 1);
        } else {
            at += 1;
        }
    }
    None
}

fn instr_field(p: &mut Program, s: &mut impl Source) {
    if p.instrs.is_empty() {
        return;
    }
    let pc = s.below(p.instrs.len());
    let fresh = instruction(p, s);
    let target = &mut p.instrs[pc];
    match s.below(9) {
        0 => target.op = fresh.op,
        1 => target.dst = fresh.dst,
        2 => target.a = fresh.a,
        3 => target.b = fresh.b,
        4 => target.c = fresh.c,
        5 => target.flags = if s.chance(1, 2) { target.flags ^ 1 } else { s.byte() },
        6 => target.imm = fresh.imm,
        7 => target.imm ^= 1u64 << s.below(64),
        _ => target.reserved[s.below(2)] = s.byte(),
    }
}

fn insert_instr(p: &mut Program, s: &mut impl Source) {
    if p.instrs.len() >= MAX_INSTRS {
        return;
    }
    let pc = s.below(p.instrs.len() + 1);
    let instr = if !p.instrs.is_empty() && s.chance(1, 3) {
        p.instrs[s.below(p.instrs.len())]
    } else {
        instruction(p, s)
    };
    let enclosing = enclosing_loop(p, pc).or_else(|| {
        // Inserting right at a body's end extends that body too, when asked to.
        (pc > 0).then(|| enclosing_loop(p, pc - 1)).flatten().filter(|(_, end)| *end == pc)
    });
    p.instrs.insert(pc, instr);
    if let Some((header, _)) = enclosing {
        if s.chance(3, 4) {
            p.instrs[header].a = p.instrs[header].a.wrapping_add(1);
        }
    }
}

fn delete_instr(p: &mut Program, s: &mut impl Source) {
    if p.instrs.len() <= 1 {
        return;
    }
    let pc = s.below(p.instrs.len());
    let enclosing = enclosing_loop(p, pc);
    p.instrs.remove(pc);
    if let Some((header, _)) = enclosing {
        if s.chance(3, 4) {
            p.instrs[header].a = p.instrs[header].a.wrapping_sub(1);
        }
    }
}

fn shuffle_instrs(p: &mut Program, s: &mut impl Source) {
    if p.instrs.len() < 2 {
        return;
    }
    let left = s.below(p.instrs.len());
    let right = s.below(p.instrs.len());
    if s.chance(1, 2) {
        p.instrs.swap(left, right);
    } else {
        // Move one instruction elsewhere, keeping the count.
        let instr = p.instrs.remove(left);
        p.instrs.insert(right.min(p.instrs.len()), instr);
    }
}

fn account_field(p: &mut Program, s: &mut impl Source) {
    if p.accounts.is_empty() {
        return;
    }
    let pubkeys = p.pubkeys.len();
    let index = s.below(p.accounts.len());
    let account = &mut p.accounts[index];
    match s.below(6) {
        0 => account.flags ^= 1 << s.below(3),
        1 => account.flags = s.byte(),
        2 => account.address = if s.chance(1, 2) { NONE } else { s.below(pubkeys + 1) as u8 },
        3 => account.owner = if s.chance(1, 2) { NONE } else { s.below(pubkeys + 1) as u8 },
        4 => account.min_len = s.pick(&[0u32, 1, 8, 16, 32, 64, 72, 165, 1_024, u32::MAX]).unwrap_or(0),
        _ => account.reserved = s.byte(),
    }
}

fn cpi_field(p: &mut Program, s: &mut impl Source) {
    if p.cpis.is_empty() {
        return;
    }
    let groups = p.header.account_groups;
    let records = p.cpi_accounts.len();
    let segments = p.segments.len();
    let program = account_ref(p, s);
    let index = s.below(p.cpis.len());
    let cpi = &mut p.cpis[index];
    match s.below(8) {
        0 => cpi.program = program,
        1 => cpi.group = if s.chance(1, 2) { NONE } else { s.below(groups as usize + 1) as u8 },
        2 => cpi.account_start = s.below(records + 1) as u16,
        3 => cpi.account_len = s.pick(&[0u8, 1, 2, 3, 64, 65]).unwrap_or(1),
        4 => cpi.segment_start = s.below(segments + 1) as u16,
        5 => cpi.segment_len = s.below(4) as u8,
        6 => {
            cpi.max_data_len = match s.below(4) {
                0 => cpi.max_data_len.wrapping_add(1),
                1 => cpi.max_data_len.wrapping_sub(1),
                2 => 4_096,
                _ => 4_097,
            }
        }
        _ => cpi.reserved[s.below(2)] = s.byte(),
    }
}

fn cpi_account_field(p: &mut Program, s: &mut impl Source) {
    if p.cpi_accounts.is_empty() {
        return;
    }
    let account = account_ref(p, s);
    let index = s.below(p.cpi_accounts.len());
    let record = &mut p.cpi_accounts[index];
    match s.below(4) {
        0 => record.account = account,
        1 => record.flags ^= 1 << s.below(2),
        2 => record.flags = s.byte(),
        _ => record.flags = 3,
    }
}

fn segment_field(p: &mut Program, s: &mut impl Source) {
    if p.segments.is_empty() {
        return;
    }
    let blob = p.blob.len();
    let register = register(p, s);
    let index = s.below(p.segments.len());
    let segment = &mut p.segments[index];
    match s.below(7) {
        0 => segment.kind = s.below(11) as u8,
        1 => segment.register = register,
        2 => segment.offset = s.below(blob + 1) as u16,
        3 => segment.len = s.pick(&[0u16, 1, 3, 4, 16, 32, 33]).unwrap_or(1),
        4 => segment.reserved[s.below(2)] = s.byte(),
        5 => {
            // Turn a literal into a register segment or back, leaving the other fields as they are.
            segment.kind = if segment.kind == 0 { 1 + s.below(9) as u8 } else { 0 };
        }
        _ => segment.register = NONE,
    }
}

fn header_field(p: &mut Program, s: &mut impl Source) {
    let header = &mut p.header;
    match s.below(10) {
        0 => header.registers = s.pick(&[0u8, 1, 63, 64, 65, 255]).unwrap_or(1),
        1 => header.registers = header.registers.wrapping_add(if s.chance(1, 2) { 1 } else { 255 }),
        2 => header.max_rows = s.pick(&[0u8, 1, 2, 4, 8, 60, 120, 255]).unwrap_or(1),
        3 => header.min_rows = s.below(header.max_rows as usize + 2) as u8,
        4 => header.flags ^= 1 << s.below(8),
        5 => header.account_groups = s.below(10) as u8,
        // Move the split between fixed and row accounts, or inputs, without moving records.
        6 => header.batch_stride = s.below(p.accounts.len().min(9) + 1) as u8,
        7 => header.row_inputs = s.below(p.inputs.len().min(9) + 1) as u8,
        8 => header.reserved = s.byte(),
        _ => header.version = s.byte(),
    }
}

fn add_record(p: &mut Program, s: &mut impl Source) {
    match s.below(8) {
        0 if p.pubkeys.len() < MAX_PUBKEYS => {
            let key = match s.below(4) {
                0 => INSTRUCTIONS_SYSVAR,
                1 => SYSTEM_PROGRAM,
                _ => [s.byte(); 32],
            };
            p.pubkeys.push(key);
        }
        1 if p.accounts.len() < MAX_ACCOUNTS => {
            let pubkeys = p.pubkeys.len();
            let account = Account {
                flags: s.below(8) as u8,
                address: if s.chance(1, 2) { NONE } else { s.below(pubkeys.max(1)) as u8 },
                owner: if s.chance(3, 4) { NONE } else { s.below(pubkeys.max(1)) as u8 },
                reserved: 0,
                min_len: s.pick(&[0u32, 8, 32, 165]).unwrap_or(0),
            };
            // A fixed account goes before the row; a row account after it.
            let at = if s.chance(3, 4) { p.fixed_count() } else { p.accounts.len() };
            p.accounts.insert(at, account);
            if at == p.accounts.len() - 1 && at >= p.fixed_count() && s.chance(1, 2) {
                p.header.batch_stride = p.header.batch_stride.saturating_add(1);
            }
        }
        2 if p.segments.len() < MAX_TABLE => {
            let segment = if s.chance(1, 2) {
                let offset = s.below(p.blob.len() + 1);
                let len = s.below(p.blob.len() - offset + 1).min(32);
                Segment { kind: 0, register: NONE, offset: offset as u16, len: len as u16, reserved: [0; 2] }
            } else {
                Segment { kind: 1 + s.below(9) as u8, register: register(p, s), ..Segment::default() }
            };
            p.segments.push(segment);
        }
        3 if p.cpis.len() < 40 => {
            let records = p.cpi_accounts.len();
            let segments = p.segments.len();
            let account_start = s.below(records + 1);
            let segment_start = s.below(segments + 1);
            let cpi = Cpi {
                program: account_ref(p, s),
                group: if s.chance(3, 4) { NONE } else { s.below(p.header.account_groups as usize + 1) as u8 },
                account_start: account_start as u16,
                account_len: s.below(records - account_start + 1).min(4) as u8,
                segment_len: s.below(segments - segment_start + 1).min(3) as u8,
                segment_start: segment_start as u16,
                max_data_len: s.below(64) as u16,
                reserved: [0; 2],
            };
            p.cpis.push(cpi);
        }
        4 if p.cpi_accounts.len() < MAX_TABLE => {
            let record = CpiAccount { account: account_ref(p, s), flags: s.below(4) as u8 };
            p.cpi_accounts.push(record);
        }
        5 if p.inputs.len() < 40 => {
            let value_type = 1 + s.below(6) as u8;
            let max_len = if value_type == 6 { 1 + s.below(64) as u16 } else { 0 };
            let input = model::Input { value_type, reserved: 0, max_len };
            if s.chance(3, 4) {
                p.inputs.insert(p.header.fixed_inputs as usize, input);
                p.header.fixed_inputs = p.header.fixed_inputs.saturating_add(1);
            } else {
                p.inputs.push(input);
                p.header.row_inputs = p.header.row_inputs.saturating_add(1);
            }
        }
        6 if p.blob.len() < MAX_BLOB => {
            let len = 1 + s.below(16);
            for _ in 0..len {
                p.blob.push(s.byte());
            }
        }
        _ => {
            // Remove a record from the end of some table.
            match s.below(5) {
                0 => {
                    p.pubkeys.pop();
                }
                1 => {
                    p.segments.pop();
                }
                2 => {
                    p.cpi_accounts.pop();
                }
                3 => {
                    p.cpis.pop();
                }
                _ => {
                    p.blob.pop();
                }
            }
        }
    }
}

fn blob_field(p: &mut Program, s: &mut impl Source) {
    if p.blob.is_empty() {
        return;
    }
    let at = s.below(p.blob.len());
    match s.below(3) {
        // Make or break an EMIT tag's reserved prefix.
        0 if p.blob.len() >= at + 3 => p.blob[at..at + 3].copy_from_slice(b"BEV"),
        _ => p.blob[at] = s.byte(),
    }
}

/// Grows or shrinks a loop's body, or rewrites its carry mask or maximum.
fn loop_surgery(p: &mut Program, s: &mut impl Source) {
    let loops: Vec<usize> = (0..p.instrs.len())
        .filter(|&pc| matches!(p.instrs[pc].op, op::FOREACH | op::REPEAT))
        .collect();
    let Some(pc) = s.pick(&loops) else {
        // No loop yet: wrap a run of instructions in one.
        if p.instrs.len() >= MAX_INSTRS || p.instrs.is_empty() {
            return;
        }
        let at = s.below(p.instrs.len());
        let body = 1 + s.below(p.instrs.len() - at);
        let kind = if p.header.batch_stride > 0 && s.chance(1, 2) { op::FOREACH } else { op::REPEAT };
        let count = register(p, s);
        let mut header = Instr::new(kind, NONE, body as u8, count, 1 + s.below(4) as u8, 0);
        if s.chance(1, 2) {
            header.imm = immediate(p, s);
        }
        p.instrs.insert(at, header);
        return;
    };
    let registers = p.header.registers;
    let instr = &mut p.instrs[pc];
    match s.below(6) {
        0 => instr.a = instr.a.wrapping_add(1),
        1 => instr.a = instr.a.wrapping_sub(1),
        2 => instr.imm ^= 1u64 << s.below(registers.clamp(1, 64) as usize),
        3 => instr.imm = 0,
        4 => instr.c = s.pick(&[0u8, 1, 2, 3, 8, 64, 255]).unwrap_or(1),
        _ => instr.op = if instr.op == op::FOREACH { op::REPEAT } else { op::FOREACH },
    }
}

/// Applies one random mutation and makes the header's counts match the sections again.
pub fn mutate(p: &mut Program, s: &mut impl Source) {
    match s.below(20) {
        0..=5 => instr_field(p, s),
        6 => insert_instr(p, s),
        7 => delete_instr(p, s),
        8 => shuffle_instrs(p, s),
        9 => account_field(p, s),
        10 => cpi_field(p, s),
        11 => cpi_account_field(p, s),
        12 | 13 => segment_field(p, s),
        14 => header_field(p, s),
        15 | 16 => add_record(p, s),
        17 => blob_field(p, s),
        _ => loop_surgery(p, s),
    }
    p.sync_counts();
}
