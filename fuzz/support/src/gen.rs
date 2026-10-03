//! Builds near-valid programs with `ProgramBuilder` from a stream of choices.
//!
//! `ballista_common::template::generate` builds programs that verify by construction, but invokes
//! nothing and reads no account data. This generator reaches the rest: CPIs with every privilege
//! and account group, return data, PDAs, introspection, byte reads, `bytes` values in data and
//! carries, and several registry entries. Operands come from a typed pool, so most programs are
//! valid; one pick in [`SLOPPY`] takes any register, account or value instead, so some are not.

use ballista_common::template::{
    record, ProgramBuilder, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_BOOL,
    DATA_REG_BYTES, DATA_REG_I64, DATA_REG_PUBKEY, DATA_REG_U128, DATA_REG_U16, DATA_REG_U32,
    DATA_REG_U64, DATA_REG_U8, INSTRUCTIONS_SYSVAR_ID, ITERATION_ACCOUNT_BIT, NO_INDEX,
    PROGRAM_FLAG_EMIT_EVENT, SYSTEM_PROGRAM_ADDRESS, VALUE_BOOL, VALUE_BYTES, VALUE_I64, VALUE_PUBKEY,
    VALUE_U128, VALUE_U64,
};

use crate::checker::op;
use crate::mutate::Source;

/// One operand pick in this many ignores the types.
pub const SLOPPY: usize = 48;

const MAX_REGISTERS: u8 = 58;
const MAX_INSTRUCTIONS: usize = 120;
const MAX_CPIS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Bool,
    U64,
    I64,
    U128,
    Pubkey,
    Bytes(usize),
}

impl Kind {
    fn same(self, other: Kind) -> bool {
        match (self, other) {
            (Kind::Bytes(_), Kind::Bytes(_)) => true,
            _ => self == other,
        }
    }

    fn numeric(self) -> bool {
        matches!(self, Kind::U64 | Kind::I64 | Kind::U128)
    }

    fn unsigned(self) -> bool {
        matches!(self, Kind::U64 | Kind::U128)
    }
}

fn read_kind(selector: u8) -> Kind {
    match selector {
        op::READ_I64 | op::READ_I32 => Kind::I64,
        op::READ_U128 => Kind::U128,
        op::READ_PUBKEY => Kind::Pubkey,
        op::READ_BOOL => Kind::Bool,
        _ => Kind::U64,
    }
}

fn read_width(selector: u8) -> u32 {
    crate::checker::read_opcode(selector).map_or(0, |(width, _)| width as u32)
}

const READS: [u8; 9] = [
    op::READ_U8,
    op::READ_U16,
    op::READ_U32,
    op::READ_U64,
    op::READ_I32,
    op::READ_I64,
    op::READ_U128,
    op::READ_PUBKEY,
    op::READ_BOOL,
];
const WRITABLE_FIELDS: [u8; 5] = [op::READ_BOOL, op::READ_U64, op::READ_I64, op::READ_U128, op::READ_PUBKEY];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    Root,
    Rows,
    Count,
}

/// A declared account: its reference, flags and minimum data length.
#[derive(Clone, Copy, Debug)]
struct Declared {
    reference: u8,
    flags: u8,
    min_len: u32,
}

#[derive(Default)]
struct Accounts {
    fixed: Vec<Declared>,
    row: Vec<Declared>,
    sysvar: Option<u8>,
    system: Option<u8>,
    payer: Option<u8>,
    entries: Vec<u8>,
}

impl Accounts {
    fn visible(&self, scope: Scope) -> Vec<Declared> {
        let mut all = self.fixed.clone();
        if scope == Scope::Rows {
            all.extend(self.row.iter().copied());
        }
        all
    }

}

type Pool = Vec<(u8, Kind)>;

struct Gen<'s, S: Source> {
    s: &'s mut S,
    b: ProgramBuilder,
    accounts: Accounts,
    fixed_inputs: Vec<(u8, Kind)>,
    row_inputs: Vec<(u8, Kind)>,
    groups: u8,
    /// `(entry account, registry index, size)` of each open so far.
    opened: Vec<(u8, u8, u16)>,
    cpis: usize,
    /// The batch's maximum rows: how many passes a FOREACH makes at most.
    max_rows: usize,
    /// Descriptors made so far, so a later invoke can name one again from another place.
    descriptors: Vec<Descriptor>,
    /// Entry accounts still to open. Opens go anywhere at the root, between other steps.
    pending_opens: Vec<u8>,
    /// Invoke often and push loop maxima toward the 64-CPI bound.
    cpi_heavy: bool,
}

/// A CPI descriptor the generator made: what an invoke of it needs to be valid.
#[derive(Clone, Debug)]
struct Descriptor {
    index: u8,
    /// The registers its data segments read, with the kinds they held.
    registers: Vec<(u8, Kind)>,
    /// Whether its program or an account record is a row account.
    rows: bool,
}

/// Builds a program from `s`. Returns its payload, or `None` when it outgrew the size cap.
pub fn program(s: &mut impl Source) -> Option<Vec<u8>> {
    let mut generator = Gen {
        s,
        b: ProgramBuilder::new(),
        accounts: Accounts::default(),
        fixed_inputs: Vec::new(),
        row_inputs: Vec::new(),
        groups: 0,
        opened: Vec::new(),
        cpis: 0,
        max_rows: 0,
        descriptors: Vec::new(),
        pending_opens: Vec::new(),
        cpi_heavy: false,
    };
    generator.declare();
    generator.body();
    generator.b.build().ok()
}

impl<S: Source> Gen<'_, S> {
    fn sloppy(&mut self) -> bool {
        self.s.chance(1, SLOPPY)
    }

    fn instructions(&mut self) -> usize {
        self.b.instructions_mut().len()
    }

    fn room(&mut self) -> bool {
        self.b.register_count() < MAX_REGISTERS && self.instructions() < MAX_INSTRUCTIONS
    }

    // ---- Declarations ------------------------------------------------------------------------

    fn declare(&mut self) {
        if self.s.chance(1, 4) {
            self.b.flags(PROGRAM_FLAG_EMIT_EVENT);
        }
        // Special accounts first, each now and then.
        if self.s.chance(1, 3) {
            let reference = self.b.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
            self.accounts.sysvar = Some(reference);
            self.accounts.fixed.push(Declared { reference, flags: 0, min_len: 0 });
        }
        self.cpi_heavy = self.s.chance(1, 3);
        let programs = self.s.below(3) + usize::from(self.cpi_heavy);
        for index in 0..programs {
            let key = [0x40 + index as u8; 32];
            let reference = self.b.account(ACCOUNT_EXECUTABLE, Some(key), None, 0);
            self.accounts.fixed.push(Declared { reference, flags: ACCOUNT_EXECUTABLE, min_len: 0 });
            // The same address in a second slot, declared with other privileges: a run fills both
            // with one account, and the per-slot ceiling must still hold for each.
            if self.s.chance(1, 4) {
                let flags = [0u8, ACCOUNT_WRITABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE | ACCOUNT_EXECUTABLE][self.s.below(4)];
                let alias = self.b.account(flags, Some(key), None, 0);
                self.accounts.fixed.push(Declared { reference: alias, flags, min_len: 0 });
            }
        }
        if self.s.chance(1, 3) {
            let reference = self.b.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
            self.accounts.system = Some(reference);
            self.accounts.fixed.push(Declared { reference, flags: ACCOUNT_EXECUTABLE, min_len: 0 });
            let payer = self.b.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
            self.accounts.payer = Some(payer);
            self.accounts.fixed.push(Declared { reference: payer, flags: ACCOUNT_SIGNER | ACCOUNT_WRITABLE, min_len: 0 });
            for _ in 0..1 + self.s.below(3) {
                let entry = self.b.account(ACCOUNT_WRITABLE, None, None, 0);
                self.accounts.entries.push(entry);
                self.accounts.fixed.push(Declared { reference: entry, flags: ACCOUNT_WRITABLE, min_len: 0 });
            }
        }
        for _ in 0..self.s.below(5) {
            let (flags, min_len, owner) = self.data_account();
            let reference = self.b.account(flags, None, owner, min_len);
            self.accounts.fixed.push(Declared { reference, flags, min_len });
        }
        if self.s.chance(1, 2) {
            let stride = 1 + self.s.below(3);
            for _ in 0..stride {
                let (flags, min_len, owner) = self.data_account();
                let flags = if self.s.chance(1, 5) { flags | ACCOUNT_EXECUTABLE } else { flags };
                let reference = self.b.row_account(flags, None, owner, min_len);
                self.accounts.row.push(Declared { reference, flags, min_len });
            }
            let max = 1 + self.s.below(6) as u8;
            let min = self.s.below(max as usize + 1) as u8;
            self.b.batch(max, min);
            self.max_rows = max as usize;
        }
        for _ in 0..self.s.below(5) {
            let (value_type, kind, max_len) = self.input_type();
            let index = self.b.input(value_type, max_len);
            self.fixed_inputs.push((index, kind));
        }
        if !self.accounts.row.is_empty() {
            for _ in 0..self.s.below(3) {
                let (value_type, kind, max_len) = self.input_type();
                let reference = self.b.row_input(value_type, max_len);
                self.row_inputs.push((reference, kind));
            }
        }
        self.groups = self.s.below(3) as u8;
        self.b.account_groups(self.groups);
    }

    fn data_account(&mut self) -> (u8, u32, Option<[u8; 32]>) {
        let flags = self.s.below(4) as u8;
        let min_len = [0u32, 8, 32, 64, 165][self.s.below(5)];
        let owner = self.s.chance(1, 2).then_some([0x77; 32]);
        (flags, min_len, owner)
    }

    fn input_type(&mut self) -> (u8, Kind, u16) {
        match self.s.below(6) {
            0 => (VALUE_BOOL, Kind::Bool, 0),
            1 => (VALUE_U64, Kind::U64, 0),
            2 => (VALUE_I64, Kind::I64, 0),
            3 => (VALUE_U128, Kind::U128, 0),
            4 => (VALUE_PUBKEY, Kind::Pubkey, 0),
            _ => {
                let max_len = 1 + self.s.below(48) as u16;
                (VALUE_BYTES, Kind::Bytes(max_len as usize), max_len)
            }
        }
    }

    // ---- The program ----------------------------------------------------------------------------

    fn body(&mut self) {
        let mut pool: Pool = Vec::new();
        // Something of each common kind to start with.
        let seed = self.b.const_u64(self.s.word() % 1_000);
        pool.push((seed, Kind::U64));
        if self.s.chance(1, 2) {
            let flag = self.b.const_bool(true);
            pool.push((flag, Kind::Bool));
        }

        // Registry opens sit at the root, before any field is read or written. A key is often the
        // payer's address.
        if let (Some(payer), true) = (self.accounts.payer, self.s.chance(1, 2)) {
            let key = self.b.account_key(payer);
            pool.push((key, Kind::Pubkey));
        }
        let entries = self.accounts.entries.clone();
        for entry in entries {
            if self.s.chance(3, 4) {
                self.pending_opens.push(entry);
            }
        }

        let return_data = self.s.chance(1, 4);
        let root_ops = 1 + self.s.below(8);
        for _ in 0..root_ops {
            self.maybe_open(&mut pool);
            self.operation(Scope::Root, &mut pool, 1);
        }
        let batched = !self.accounts.row.is_empty();
        let loops = if batched { 1 + self.s.below(2) } else { self.s.below(3) };
        let forced = self.s.below(loops.max(1));
        for index in 0..loops {
            let rows = batched && (index == forced || self.s.chance(1, 2));
            self.emit_loop(rows, &mut pool);
            for _ in 0..self.s.below(3) {
                self.maybe_open(&mut pool);
                self.operation(Scope::Root, &mut pool, 1);
            }
        }
        // Every open still pending goes before the return data, which no open may follow.
        while let Some(entry) = self.pending_opens.pop() {
            self.open(entry, &mut pool);
        }
        if return_data && self.room() {
            let count = 1 + self.s.below(3);
            let parts = self.parts(&pool, count, false);
            self.b.set_return_data(&parts);
        }
    }

    /// Opens the next pending entry one time in three, so opens fall before, between and after
    /// the root's invokes and field reads.
    fn maybe_open(&mut self, pool: &mut Pool) {
        if !self.pending_opens.is_empty() && self.s.chance(1, 3) {
            let entry = self.pending_opens.remove(0);
            self.open(entry, pool);
        }
    }

    fn emit_loop(&mut self, rows: bool, pool: &mut Pool) {
        if !self.room() {
            return;
        }
        let (scope, passes, count) = if rows {
            (Scope::Rows, self.max_rows.max(1), None)
        } else {
            let max = if self.cpi_heavy {
                // Big enough that a body with a few invokes lands on either side of 64.
                [8usize, 16, 21, 32, 63, 64, 255][self.s.below(7)]
            } else {
                1 + self.s.below(5)
            };
            let count = match self.pick(pool, |kind| kind == Kind::U64) {
                Some(register) if self.s.chance(1, 2) => register,
                _ => {
                    let register = self.b.const_u64(self.s.below(max + 2) as u64);
                    pool.push((register, Kind::U64));
                    register
                }
            };
            (Scope::Count, max, Some((count, max as u8)))
        };
        // Carry up to two registers, numeric or `bytes`, and update them in the body.
        let mut carried: Vec<(u8, Kind)> = Vec::new();
        for _ in 0..self.s.below(3) {
            let candidates: Vec<(u8, Kind)> = pool
                .iter()
                .copied()
                .filter(|(register, kind)| {
                    (kind.numeric() || matches!(kind, Kind::Bytes(_))) && !carried.iter().any(|(r, _)| r == register)
                })
                .collect();
            if let Some(choice) = self.s.pick(&candidates) {
                carried.push(choice);
            }
        }
        let mut mask = carried.iter().fold(0u64, |mask, (register, _)| mask | 1u64 << register);
        if self.sloppy() {
            mask ^= 1u64 << self.s.below(64);
        }
        // The header goes in first and learns its body length once the body is written. The body
        // starts from the root's registers, and the root never sees the body's.
        let header = match count {
            Some((count, max)) => self.b.emit(record(op::REPEAT, NO_INDEX, 0, count, max, 0, mask)),
            None => self.b.emit(record(op::FOREACH, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, mask)),
        };
        let mut inner = pool.clone();
        for _ in 0..1 + self.s.below(6) {
            self.operation(scope, &mut inner, passes);
        }
        for &(register, kind) in &carried {
            self.update_carry(register, kind, &mut inner);
        }
        if self.instructions() == header + 1 || self.s.chance(1, 4) {
            let always = self.b.const_bool(true);
            self.b.require(always);
        }
        let body_len = self.instructions() - header - 1;
        self.b.instructions_mut()[header].a = body_len.min(255) as u8;
    }

    /// Reassigns a carried register with a value of exactly its type.
    fn update_carry(&mut self, register: u8, kind: Kind, pool: &mut Pool) {
        if !self.room() {
            return;
        }
        match kind {
            Kind::Bytes(_) => {
                // A `select` between the register and itself keeps its maximum length.
                let condition = self.bool_register(pool);
                let same = self.b.select(condition, register, register);
                self.b.mov(register, same);
            }
            _ => {
                let others: Vec<u8> = pool.iter().filter(|(_, k)| *k == kind).map(|(r, _)| *r).collect();
                let other = self.s.pick(&others).unwrap_or(register);
                let opcode = [op::MIN, op::MAX, op::ADD][self.s.below(3)];
                let value = self.b.binary(opcode, register, other);
                self.b.mov(register, value);
            }
        }
    }

    // ---- Operand picks ------------------------------------------------------------------------

    /// A register of a kind `accepts` takes, or now and then any register at all.
    fn pick(&mut self, pool: &Pool, accepts: impl Fn(Kind) -> bool) -> Option<u8> {
        if self.sloppy() {
            return Some(self.s.below(self.b.register_count() as usize + 1) as u8);
        }
        let matching: Vec<u8> = pool.iter().filter(|(_, kind)| accepts(*kind)).map(|(r, _)| *r).collect();
        self.s.pick(&matching)
    }

    fn kind_of(pool: &Pool, register: u8) -> Option<Kind> {
        pool.iter().rev().find(|(r, _)| *r == register).map(|(_, kind)| *kind)
    }

    fn bool_register(&mut self, pool: &mut Pool) -> u8 {
        match self.pick(pool, |kind| kind == Kind::Bool) {
            Some(register) => register,
            None => {
                let register = self.b.const_bool(self.s.chance(1, 2));
                pool.push((register, Kind::Bool));
                register
            }
        }
    }

    fn u64_register(&mut self, pool: &mut Pool, small: bool) -> u8 {
        match self.pick(pool, |kind| kind == Kind::U64) {
            Some(register) if !small || self.s.chance(1, 3) => register,
            _ => {
                let value = if small { self.s.below(8) as u64 } else { self.s.word() };
                let register = self.b.const_u64(value);
                pool.push((register, Kind::U64));
                register
            }
        }
    }

    fn account(&mut self, scope: Scope, accepts: impl Fn(&Declared) -> bool) -> Option<Declared> {
        if self.sloppy() {
            let reference = if self.s.chance(1, 2) {
                self.s.below(self.accounts.fixed.len() + 1) as u8
            } else {
                ITERATION_ACCOUNT_BIT | self.s.below(self.accounts.row.len() + 1) as u8
            };
            return Some(Declared { reference, flags: 0, min_len: 0 });
        }
        let candidates: Vec<Declared> = self.accounts.visible(scope).into_iter().filter(|d| accepts(d)).collect();
        self.s.pick(&candidates)
    }

    /// A data segment for invocation data, an output or a seed, of at most `max_width` bytes.
    fn part(&mut self, pool: &Pool, max_width: usize) -> Segment {
        let registers: Vec<(u8, Kind)> = pool
            .iter()
            .copied()
            .filter(|(_, kind)| match kind {
                Kind::Bytes(len) => *len <= max_width,
                Kind::U128 => max_width >= 16,
                Kind::Pubkey => max_width >= 32,
                _ => true,
            })
            .collect();
        if registers.is_empty() || self.s.chance(1, 4) {
            let len = 1 + self.s.below(max_width.clamp(1, 8));
            let bytes: Vec<u8> = (0..len).map(|_| self.s.byte()).collect();
            return Segment::Literal(self.b.blob(&bytes));
        }
        let (register, kind) = registers[self.s.below(registers.len())];
        let encoding = match kind {
            Kind::Bool => DATA_REG_BOOL,
            Kind::I64 => DATA_REG_I64,
            Kind::Pubkey => DATA_REG_PUBKEY,
            Kind::Bytes(_) => DATA_REG_BYTES,
            Kind::U128 if self.s.chance(1, 2) => DATA_REG_U128,
            _ => [DATA_REG_U8, DATA_REG_U16, DATA_REG_U32, DATA_REG_U64][self.s.below(4)],
        };
        if self.sloppy() {
            return Segment::Register(1 + self.s.below(9) as u8, register);
        }
        Segment::Register(encoding, register)
    }

    fn parts(&mut self, pool: &Pool, count: usize, tagged: bool) -> Vec<Segment> {
        let mut parts = Vec::with_capacity(count + 1);
        if tagged {
            let mut tag: Vec<u8> = (0..4 + self.s.below(5)).map(|_| self.s.byte()).collect();
            if tag.starts_with(b"BEV") && !self.sloppy() {
                tag[0] = b'X';
            }
            parts.push(Segment::Literal(self.b.blob(&tag)));
        }
        for _ in 0..count {
            parts.push(self.part(pool, 64));
        }
        parts
    }

    /// The most bytes `parts` encode, reading each register's kind from `pool`.
    fn width(&self, pool: &Pool, parts: &[Segment]) -> usize {
        parts
            .iter()
            .map(|part| match *part {
                Segment::Literal((_, len)) => len as usize,
                Segment::Register(kind, register) => match kind {
                    DATA_REG_U8 | DATA_REG_BOOL => 1,
                    DATA_REG_U16 => 2,
                    DATA_REG_U32 => 4,
                    DATA_REG_U64 | DATA_REG_I64 => 8,
                    DATA_REG_U128 => 16,
                    DATA_REG_PUBKEY => 32,
                    DATA_REG_BYTES => match Self::kind_of(pool, register) {
                        Some(Kind::Bytes(len)) => len,
                        _ => 0,
                    },
                    _ => 0,
                },
            })
            .sum()
    }

    // ---- Operations ---------------------------------------------------------------------------

    /// Emits one operation in `scope`. `passes` is how many times a loop body runs, to keep the
    /// worst-case CPI count within the limit.
    fn operation(&mut self, scope: Scope, pool: &mut Pool, passes: usize) {
        if !self.room() {
            return;
        }
        // In CPI-heavy programs, half of all operations invoke.
        if self.cpi_heavy && self.s.chance(1, 2) {
            self.invoke(scope, pool, passes);
            return;
        }
        match self.s.below(24) {
            0 => self.constant(pool),
            1 => self.load_input(scope, pool),
            2 => self.account_property(scope, pool),
            3 | 4 => self.account_read(scope, pool),
            5 | 6 => self.arithmetic(pool),
            7 => self.comparison(pool),
            8 => self.logic(pool),
            9 => self.select_or_cast(pool),
            10 | 11 => self.invoke(scope, pool, passes),
            12 => self.pda(scope, pool),
            13 => self.introspect(pool),
            14 => self.byte_read(scope, pool),
            15 => self.emit(pool),
            16 => self.registry_field(pool),
            17 => {
                let condition = self.bool_register(pool);
                self.b.require(condition);
            }
            18 if scope != Scope::Root => {
                let index = self.b.loop_index();
                pool.push((index, Kind::U64));
            }
            19 => {
                if let Some(source) = self.pick(pool, |_| true) {
                    let destination = self.b.register();
                    self.b.mov(destination, source);
                    if let Some(kind) = Self::kind_of(pool, source) {
                        pool.push((destination, kind));
                    }
                }
            }
            20 => {
                if let Some(value) = self.pick(pool, |kind| matches!(kind, Kind::Bytes(_))) {
                    let length = self.b.bytes_len(value);
                    pool.push((length, Kind::U64));
                }
            }
            _ => self.math(pool),
        }
    }

    fn constant(&mut self, pool: &mut Pool) {
        let (register, kind) = match self.s.below(6) {
            0 => (self.b.const_bool(self.s.chance(1, 2)), Kind::Bool),
            1 => (self.b.const_u64(self.s.word() >> self.s.below(64)), Kind::U64),
            2 => (self.b.const_i64(self.s.word() as i64 >> self.s.below(64)), Kind::I64),
            3 => (self.b.const_u128(self.s.word() as u128), Kind::U128),
            4 => (self.b.const_pubkey([self.s.byte(); 32]), Kind::Pubkey),
            _ => {
                let bytes: Vec<u8> = (0..self.s.below(40)).map(|_| self.s.byte()).collect();
                (self.b.const_bytes(&bytes), Kind::Bytes(bytes.len()))
            }
        };
        pool.push((register, kind));
    }

    fn load_input(&mut self, scope: Scope, pool: &mut Pool) {
        let mut inputs = self.fixed_inputs.clone();
        if scope == Scope::Rows {
            inputs.extend(self.row_inputs.iter().copied());
        }
        if self.sloppy() {
            inputs.push((ITERATION_ACCOUNT_BIT | self.s.below(4) as u8, Kind::U64));
        }
        if let Some((input, kind)) = self.s.pick(&inputs) {
            let register = self.b.load_input(input);
            pool.push((register, kind));
        }
    }

    fn account_property(&mut self, scope: Scope, pool: &mut Pool) {
        let Some(account) = self.account(scope, |_| true) else { return };
        let (opcode, kind) = [
            (op::ACCOUNT_KEY, Kind::Pubkey),
            (op::ACCOUNT_OWNER, Kind::Pubkey),
            (op::ACCOUNT_LAMPORTS, Kind::U64),
            (op::ACCOUNT_DATA_LEN, Kind::U64),
            (op::ACCOUNT_IS_EMPTY, Kind::Bool),
        ][self.s.below(5)];
        let register = self.b.op(opcode, account.reference, NO_INDEX, NO_INDEX, 0);
        pool.push((register, kind));
    }

    /// A typed read of account data, at a fixed offset inside the declared minimum length or at
    /// an offset from a register.
    fn account_read(&mut self, scope: Scope, pool: &mut Pool) {
        let selector = READS[self.s.below(READS.len())];
        let width = read_width(selector);
        let entries = self.accounts.entries.clone();
        let dynamic = self.s.chance(1, 3);
        let Some(account) = self.account(scope, |d| {
            !entries.contains(&d.reference) && (dynamic || d.min_len >= width)
        }) else {
            return;
        };
        let register = if dynamic {
            let offset = self.u64_register(pool, true);
            self.b.read_dynamic(selector, account.reference, offset)
        } else {
            let room = account.min_len.saturating_sub(width) as u64;
            let mut offset = self.s.below(room as usize + 1) as u64;
            if self.sloppy() {
                offset += 1 + self.s.below(4) as u64;
            }
            self.b.read(selector, account.reference, offset)
        };
        pool.push((register, read_kind(selector)));
    }

    fn arithmetic(&mut self, pool: &mut Pool) {
        let Some(left) = self.pick(pool, Kind::numeric) else { return };
        let kind = Self::kind_of(pool, left).unwrap_or(Kind::U64);
        let right = self.pick(pool, |k| k == kind).unwrap_or(left);
        let opcode = [op::ADD, op::SUB, op::MUL, op::DIV, op::MIN, op::MAX, op::REM][self.s.below(7)];
        let register = self.b.binary(opcode, left, right);
        pool.push((register, kind));
    }

    fn comparison(&mut self, pool: &mut Pool) {
        let Some(left) = self.pick(pool, |_| true) else { return };
        let kind = Self::kind_of(pool, left).unwrap_or(Kind::U64);
        let right = self.pick(pool, |k| k.same(kind)).unwrap_or(left);
        let opcode = if kind.numeric() {
            [op::EQ, op::NE, op::LT, op::LTE, op::GT, op::GTE][self.s.below(6)]
        } else {
            [op::EQ, op::NE][self.s.below(2)]
        };
        let register = self.b.binary(opcode, left, right);
        pool.push((register, Kind::Bool));
    }

    fn logic(&mut self, pool: &mut Pool) {
        let left = self.bool_register(pool);
        let register = match self.s.below(3) {
            0 => self.b.not(left),
            choice => {
                let right = self.bool_register(pool);
                self.b.binary(if choice == 1 { op::AND } else { op::OR }, left, right)
            }
        };
        pool.push((register, Kind::Bool));
    }

    fn select_or_cast(&mut self, pool: &mut Pool) {
        if self.s.chance(1, 2) {
            let condition = self.bool_register(pool);
            let Some(if_true) = self.pick(pool, |_| true) else { return };
            let kind = Self::kind_of(pool, if_true).unwrap_or(Kind::U64);
            let if_false = self.pick(pool, |k| k.same(kind)).unwrap_or(if_true);
            let result = match (kind, Self::kind_of(pool, if_false)) {
                (Kind::Bytes(left), Some(Kind::Bytes(right))) => Kind::Bytes(left.max(right)),
                _ => kind,
            };
            let register = self.b.select(condition, if_true, if_false);
            pool.push((register, result));
        } else {
            let Some(value) = self.pick(pool, Kind::numeric) else { return };
            let (opcode, kind) = [(op::CAST_U64, Kind::U64), (op::CAST_I64, Kind::I64), (op::CAST_U128, Kind::U128)]
                [self.s.below(3)];
            let register = self.b.cast(opcode, value);
            pool.push((register, kind));
        }
    }

    fn math(&mut self, pool: &mut Pool) {
        match self.s.below(4) {
            0 => {
                let Some(left) = self.pick(pool, Kind::unsigned) else { return };
                let kind = Self::kind_of(pool, left).unwrap_or(Kind::U64);
                let right = self.pick(pool, |k| k == kind).unwrap_or(left);
                let opcode = [op::BIT_AND, op::BIT_OR, op::BIT_XOR][self.s.below(3)];
                let register = self.b.binary(opcode, left, right);
                pool.push((register, kind));
            }
            1 => {
                let Some(value) = self.pick(pool, Kind::unsigned) else { return };
                let kind = Self::kind_of(pool, value).unwrap_or(Kind::U64);
                let shift = self.u64_register(pool, true);
                let opcode = if self.s.chance(1, 2) { op::SHL } else { op::SHR };
                let register = self.b.binary(opcode, value, shift);
                pool.push((register, kind));
            }
            2 => {
                let Some(a) = self.pick(pool, Kind::unsigned) else { return };
                let kind = Self::kind_of(pool, a).unwrap_or(Kind::U64);
                let b = self.pick(pool, |k| k == kind).unwrap_or(a);
                let c = self.pick(pool, |k| k == kind).unwrap_or(a);
                let register = if self.s.chance(1, 2) { self.b.mul_div(a, b, c) } else { self.b.mul_div_ceil(a, b, c) };
                pool.push((register, kind));
            }
            _ => {
                let exponent = self.u64_register(pool, true);
                let register = self.b.pow10(exponent);
                pool.push((register, Kind::U128));
            }
        }
    }

    /// A CPI: a program account declared executable, up to four accounts passed with at most
    /// their declared privileges, data from the pool, sometimes a group, a guard, and return data.
    fn invoke(&mut self, scope: Scope, pool: &mut Pool, passes: usize) {
        if self.cpis + passes > MAX_CPIS && !self.sloppy() {
            return;
        }
        let cpi = match self.reinvoke(scope, pool) {
            Some(index) => index,
            None => match self.descriptor(scope, pool) {
                Some(index) => index,
                None => return,
            },
        };
        let guard = self.s.chance(1, 3).then(|| self.bool_register(pool));
        self.b.invoke(cpi, guard);
        self.cpis += passes;
        if guard.is_none() && self.s.chance(1, 3) && self.room() {
            let selector = READS[self.s.below(READS.len())];
            let offset = self.s.below(64) as u64;
            let register = self.b.return_data(selector, offset);
            pool.push((register, read_kind(selector)));
        }
    }

    /// One time in four, a descriptor made earlier, invoked again from here: from another loop,
    /// the root after a loop, or the same body. Usually one whose registers and row accounts are
    /// still in reach; now and then any, so the verifier sees stale ones too.
    fn reinvoke(&mut self, scope: Scope, pool: &Pool) -> Option<u8> {
        if self.descriptors.is_empty() || !self.s.chance(1, 4) {
            return None;
        }
        let reachable: Vec<u8> = self
            .descriptors
            .iter()
            .filter(|descriptor| {
                (!descriptor.rows || scope == Scope::Rows)
                    && descriptor.registers.iter().all(|(register, kind)| {
                        Self::kind_of(pool, *register).is_some_and(|held| match (held, kind) {
                            (Kind::Bytes(held), Kind::Bytes(wanted)) => held == *wanted,
                            _ => held == *kind,
                        })
                    })
            })
            .map(|descriptor| descriptor.index)
            .collect();
        if self.sloppy() {
            let all: Vec<u8> = self.descriptors.iter().map(|descriptor| descriptor.index).collect();
            return self.s.pick(&all);
        }
        self.s.pick(&reachable)
    }

    /// A new descriptor: a program account declared executable and up to four account records,
    /// each passed with at most its slot's declared privileges, often exactly them, and sometimes
    /// naming one slot twice. Data comes from the pool. One descriptor in six gets a data segment
    /// whose unused fields are not the canonical ones.
    fn descriptor(&mut self, scope: Scope, pool: &Pool) -> Option<u8> {
        let program = self.account(scope, |d| d.flags & ACCOUNT_EXECUTABLE != 0)?;
        let entries = self.accounts.entries.clone();
        let mut records: Vec<(u8, u8)> = Vec::new();
        for _ in 0..self.s.below(5) {
            let account = match records.last() {
                // The slot just listed, again, with its own flags.
                Some(&(reference, _)) if self.s.chance(1, 6) => {
                    self.accounts.visible(scope).into_iter().find(|d| d.reference == reference)
                }
                _ => self.account(scope, |_| true),
            };
            let Some(account) = account else { break };
            let ceiling = account.flags & (ACCOUNT_SIGNER | ACCOUNT_WRITABLE);
            let mut flags = if self.s.chance(1, 2) { ceiling } else { self.s.below(4) as u8 & ceiling };
            if entries.contains(&account.reference) {
                flags &= !ACCOUNT_WRITABLE;
            }
            if self.sloppy() {
                // Any of the three account flags, the executable bit included, which no record
                // may carry even when its slot declares it.
                flags = self.s.below(8) as u8;
            }
            records.push((account.reference, flags));
        }
        let parts: Vec<Segment> = (0..self.s.below(4)).map(|_| self.part(pool, 64)).collect();
        let group = if self.groups > 0 && self.s.chance(1, 3) { self.s.below(self.groups as usize) as u8 } else { NO_INDEX };
        let width = self.width(pool, &parts);
        let first_segment = self.b.segments_mut().len();
        let cpi = self.b.cpi_with_group(program.reference, &records, &parts, group);
        let declared = if self.sloppy() { width as u16 ^ 1 } else { width as u16 };
        self.b.set_cpi_max_data_len(cpi, declared);
        if !parts.is_empty() && self.s.chance(1, 6) {
            // The wire format fixes a literal's source register at 0xff and a register
            // segment's offset and length at zero.
            let segment = &mut self.b.segments_mut()[first_segment + self.s.below(parts.len())];
            if segment.kind == 0 {
                segment.register = self.s.below(255) as u8;
            } else {
                segment.offset_le = (1 + self.s.below(0xffff) as u16).to_le_bytes();
                segment.len_le = (self.s.below(0x1_0000) as u16).to_le_bytes();
            }
        }
        let registers = parts
            .iter()
            .filter_map(|part| match *part {
                Segment::Register(_, register) => Self::kind_of(pool, register).map(|kind| (register, kind)),
                Segment::Literal(_) => None,
            })
            .collect();
        let rows = program.reference & ITERATION_ACCOUNT_BIT != 0
            || records.iter().any(|(reference, _)| reference & ITERATION_ACCOUNT_BIT != 0);
        self.descriptors.push(Descriptor { index: cpi, registers, rows });
        Some(cpi)
    }

    fn pda(&mut self, scope: Scope, pool: &mut Pool) {
        let Some(program) = self.account(scope, |d| d.flags & ACCOUNT_EXECUTABLE != 0) else { return };
        let seeds: Vec<Segment> = (0..1 + self.s.below(3)).map(|_| self.part(pool, 32)).collect();
        let register = if self.s.chance(1, 2) {
            self.b.derive_pda(program.reference, &seeds)
        } else {
            let bump = self.u64_register(pool, false);
            self.b.create_pda(program.reference, bump, &seeds)
        };
        pool.push((register, Kind::Pubkey));
    }

    fn introspect(&mut self, pool: &mut Pool) {
        let Some(sysvar) = self.accounts.sysvar else { return };
        let sysvar = if self.sloppy() { self.s.below(self.accounts.fixed.len() + 1) as u8 } else { sysvar };
        let index = self.u64_register(pool, true);
        let position = self.u64_register(pool, true);
        let (register, kind) = match self.s.below(9) {
            0 => (self.b.introspect(op::INSTRUCTION_COUNT, sysvar, NO_INDEX, NO_INDEX), Kind::U64),
            1 => (self.b.introspect(op::INSTRUCTION_INDEX, sysvar, NO_INDEX, NO_INDEX), Kind::U64),
            2 => (self.b.introspect(op::INSTRUCTION_PROGRAM, sysvar, index, NO_INDEX), Kind::Pubkey),
            3 => (self.b.introspect(op::INSTRUCTION_ACCOUNT_COUNT, sysvar, index, NO_INDEX), Kind::U64),
            4 => (self.b.introspect(op::INSTRUCTION_ACCOUNT, sysvar, index, position), Kind::Pubkey),
            5 => (self.b.introspect(op::INSTRUCTION_ACCOUNT_FLAGS, sysvar, index, position), Kind::U64),
            6 => (self.b.introspect(op::INSTRUCTION_DATA_LEN, sysvar, index, NO_INDEX), Kind::U64),
            7 => {
                let selector = READS[self.s.below(READS.len())];
                (self.b.read_instruction_data(selector, sysvar, index, position), read_kind(selector))
            }
            _ => {
                let len = 1 + self.s.below(64) as u16;
                (self.b.read_instruction_bytes(sysvar, index, position, len), Kind::Bytes(len as usize))
            }
        };
        pool.push((register, kind));
    }

    fn byte_read(&mut self, scope: Scope, pool: &mut Pool) {
        let entries = self.accounts.entries.clone();
        let Some(account) = self.account(scope, |d| {
            !entries.contains(&d.reference)
                && (d.reference & ITERATION_ACCOUNT_BIT != 0 || d.flags & ACCOUNT_WRITABLE == 0)
        }) else {
            return;
        };
        let offset = self.u64_register(pool, true);
        let len = 1 + self.s.below(64) as u16;
        let register = self.b.read_account_bytes(account.reference, offset, len);
        pool.push((register, Kind::Bytes(len as usize)));
    }

    fn emit(&mut self, pool: &mut Pool) {
        let count = self.s.below(4);
        let parts = self.parts(pool, count, true);
        self.b.emit_data(&parts);
    }

    fn open(&mut self, entry: u8, pool: &mut Pool) {
        let (Some(system), Some(payer)) = (self.accounts.system, self.accounts.payer) else { return };
        if self.cpis + 3 > MAX_CPIS {
            return;
        }
        let index = self.s.below(8) as u8;
        // Every open of one registry index declares the same size.
        let size = self
            .opened
            .iter()
            .find(|(_, other, _)| *other == index)
            .map(|(_, _, size)| *size)
            .unwrap_or_else(|| [8u16, 16, 33, 64, 512][self.s.below(5)]);
        let key = if self.s.chance(1, 2) {
            self.pick(pool, |kind| kind == Kind::Pubkey)
        } else {
            None
        };
        self.b.open_registry(entry, key, payer, index, size, system);
        self.opened.push((entry, index, size));
        self.cpis += 3;
    }

    fn registry_field(&mut self, pool: &mut Pool) {
        let opened = self.opened.clone();
        let Some((entry, _, size)) = self.s.pick(&opened) else { return };
        if self.s.chance(1, 2) {
            let selector = READS[self.s.below(READS.len())];
            let width = read_width(selector) as u16;
            if width > size {
                return;
            }
            let offset = self.s.below((size - width) as usize + 1) as u16;
            let register = self.b.read_registry(entry, offset, selector);
            pool.push((register, read_kind(selector)));
        } else {
            let selector = WRITABLE_FIELDS[self.s.below(WRITABLE_FIELDS.len())];
            let width = read_width(selector) as u16;
            if width > size {
                return;
            }
            let kind = read_kind(selector);
            let Some(value) = self.pick(pool, |k| k == kind) else { return };
            let offset = self.s.below((size - width) as usize + 1) as u16;
            self.b.write_registry(entry, offset, selector, value);
        }
    }
}
