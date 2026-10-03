//! A reference checker for the guarantees finalization gives (`docs/guide/trust-model.md`,
//! "Finalization checks"), written apart from `verify.rs` and kept simple rather than fast.
//!
//! It reads the [`Program`] model, never the verifier's types, and states each rule the way the
//! docs or the executor do:
//!
//! - **Typing** follows the executor in `programs/ballista/src/processor`: for each opcode, the
//!   operand types the executor accepts without `InvalidRegister` or `TypeMismatch`, and the type
//!   it writes. Register types are tracked by abstract interpretation over every path. Loops run
//!   to a fixpoint, bounded by their own maximum, so a register must be set, with the right type,
//!   on every pass count from zero to the maximum.
//! - **References, bounds, privileges, loops, outputs, registries, account groups and limits**
//!   follow `docs/reference/wire-format.md`, `docs/reference/language.md` and
//!   `docs/reference/limits.md`.
//!
//! Every rule has a dotted name, so a violation says which promise broke. Rules under `format.`
//! and `unreferenced.` are the wire format's "zero" and "`0xff`" fields: a template that breaks
//! one still runs as written, but the docs say the verifier refuses it.

use core::fmt;

use crate::model::{Account, Instr, Program, Segment};

/// Opcode numbers, from the table in `docs/reference/wire-format.md`.
pub mod op {
    pub const LOAD_INPUT: u8 = 1;
    pub const CONST_BOOL: u8 = 2;
    pub const CONST_U64: u8 = 3;
    pub const CONST_I64: u8 = 4;
    pub const CONST_U128: u8 = 5;
    pub const CONST_PUBKEY: u8 = 6;
    pub const CONST_BYTES: u8 = 7;
    pub const ACCOUNT_KEY: u8 = 8;
    pub const ACCOUNT_OWNER: u8 = 9;
    pub const ACCOUNT_LAMPORTS: u8 = 10;
    pub const ACCOUNT_DATA_LEN: u8 = 11;
    pub const ACCOUNT_IS_EMPTY: u8 = 12;
    pub const READ_U64: u8 = 13;
    pub const READ_I64: u8 = 14;
    pub const READ_U128: u8 = 15;
    pub const READ_PUBKEY: u8 = 16;
    pub const CLOCK_SLOT: u8 = 17;
    pub const CLOCK_TIMESTAMP: u8 = 18;
    pub const ADD: u8 = 19;
    pub const SUB: u8 = 20;
    pub const MUL: u8 = 21;
    pub const DIV: u8 = 22;
    pub const EQ: u8 = 23;
    pub const NE: u8 = 24;
    pub const LT: u8 = 25;
    pub const LTE: u8 = 26;
    pub const GT: u8 = 27;
    pub const GTE: u8 = 28;
    pub const AND: u8 = 29;
    pub const OR: u8 = 30;
    pub const NOT: u8 = 31;
    pub const MIN: u8 = 32;
    pub const MAX: u8 = 33;
    pub const SELECT: u8 = 34;
    pub const CAST_U64: u8 = 35;
    pub const CAST_I64: u8 = 36;
    pub const CAST_U128: u8 = 37;
    pub const LOOP_INDEX: u8 = 38;
    pub const REQUIRE: u8 = 40;
    pub const INVOKE: u8 = 41;
    pub const FOREACH: u8 = 42;
    pub const READ_U8: u8 = 43;
    pub const READ_U16: u8 = 44;
    pub const READ_U32: u8 = 45;
    pub const READ_BOOL: u8 = 46;
    pub const DERIVE_PDA: u8 = 47;
    pub const RETURN_DATA: u8 = 48;
    pub const MOVE: u8 = 49;
    pub const CREATE_PDA: u8 = 50;
    pub const MUL_DIV: u8 = 51;
    pub const MUL_DIV_CEIL: u8 = 52;
    pub const REM: u8 = 53;
    pub const SHL: u8 = 54;
    pub const SHR: u8 = 55;
    pub const BIT_AND: u8 = 56;
    pub const BIT_OR: u8 = 57;
    pub const BIT_XOR: u8 = 58;
    pub const POW10: u8 = 59;
    pub const READ_I32: u8 = 60;
    pub const REPEAT: u8 = 61;
    pub const EMIT: u8 = 62;
    pub const SET_RETURN_DATA: u8 = 63;
    pub const INSTRUCTION_COUNT: u8 = 64;
    pub const INSTRUCTION_INDEX: u8 = 65;
    pub const INSTRUCTION_PROGRAM: u8 = 66;
    pub const INSTRUCTION_ACCOUNT_COUNT: u8 = 67;
    pub const INSTRUCTION_ACCOUNT: u8 = 68;
    pub const INSTRUCTION_ACCOUNT_FLAGS: u8 = 69;
    pub const INSTRUCTION_DATA_LEN: u8 = 70;
    pub const READ_INSTRUCTION_DATA: u8 = 71;
    pub const READ_INSTRUCTION_BYTES: u8 = 72;
    pub const READ_ACCOUNT_BYTES: u8 = 73;
    pub const BYTES_LEN: u8 = 74;
    pub const OPEN_REGISTRY: u8 = 75;
    pub const READ_REGISTRY: u8 = 76;
    pub const WRITE_REGISTRY: u8 = 77;
    pub const GROUP_LENGTH: u8 = 78;
    pub const GROUP_ANY: u8 = 79;
    pub const GROUP_COUNT: u8 = 80;
}

/// Limits from `docs/reference/limits.md`.
pub mod limit {
    pub const PAYLOAD: usize = 10_240;
    pub const RUNTIME_ACCOUNTS: usize = 120;
    pub const BATCH_STRIDE: usize = 8;
    pub const ACCOUNT_GROUPS: usize = 8;
    pub const CPI_ACCOUNTS: usize = 64;
    pub const CPIS: usize = 64;
    pub const CPI_DATA: usize = 4_096;
    pub const RETURN_DATA: usize = 1_024;
    pub const INPUTS: usize = 32;
    pub const ROW_INPUTS: usize = 8;
    pub const INPUT_VALUES: usize = 256;
    pub const BYTES: usize = 1_024;
    pub const LOOPS: usize = 8;
    pub const OUTPUT: usize = 1_024;
    pub const REGISTRIES: usize = 8;
    pub const REGISTRY_OPENS: usize = 8;
    pub const REGISTRY_SIZE: usize = 512;
    pub const REGISTRY_OPEN_CPIS: usize = 3;
    pub const REGISTERS: usize = 64;
    pub const INSTRUCTIONS: usize = 128;
    pub const PDA_SEEDS: usize = 15;
    pub const PDA_SEED_LEN: usize = 32;
    pub const EMIT_TAG: usize = 4;
    pub const GROUP_MATCHES: usize = 4;
    pub const GROUP_EXCEPTS: usize = 4;
}

pub const NONE: u8 = 0xff;
pub const ROW_BIT: u8 = 0x80;
pub const SIGNER: u8 = 1;
pub const WRITABLE: u8 = 2;
pub const EXECUTABLE: u8 = 4;
pub const FLAG_EMIT_EVENT: u8 = 1;
pub const FLAG_DYNAMIC_OFFSET: u8 = 1;

/// `Sysvar1nstructions1111111111111111111111111`, decoded from base58.
pub const INSTRUCTIONS_SYSVAR: [u8; 32] = [
    0x06, 0xa7, 0xd5, 0x17, 0x18, 0x7b, 0xd1, 0x66, 0x35, 0xda, 0xd4, 0x04, 0x55, 0xfd, 0xc2, 0xc0,
    0xc1, 0x24, 0xc6, 0x8f, 0x21, 0x56, 0x75, 0xa5, 0xdb, 0xba, 0xcb, 0x5f, 0x08, 0x00, 0x00, 0x00,
];
/// The System program's address: 32 zero bytes.
pub const SYSTEM_PROGRAM: [u8; 32] = [0; 32];

/// What a register can hold at one point of the program, over every path that reaches it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ty {
    /// Not set on at least one path. Reading it can fail with `InvalidRegister`.
    Undef,
    Bool,
    U64,
    I64,
    U128,
    Pubkey,
    /// A `bytes` value of at most this many bytes.
    Bytes(usize),
    /// Set on every path, but not with one type on all of them.
    Mixed,
}

impl Ty {
    fn join(self, other: Ty) -> Ty {
        match (self, other) {
            (Ty::Undef, _) | (_, Ty::Undef) => Ty::Undef,
            (Ty::Bytes(left), Ty::Bytes(right)) => Ty::Bytes(left.max(right)),
            (left, right) if left == right => left,
            _ => Ty::Mixed,
        }
    }

    fn is_set(self) -> bool {
        self != Ty::Undef
    }

    fn is_bytes(self) -> bool {
        matches!(self, Ty::Bytes(_))
    }

    /// Same kind, ignoring a `bytes` value's maximum length.
    fn same_kind(self, other: Ty) -> bool {
        match (self, other) {
            (Ty::Bytes(_), Ty::Bytes(_)) => true,
            (Ty::Undef | Ty::Mixed, _) | (_, Ty::Undef | Ty::Mixed) => false,
            (left, right) => left == right,
        }
    }

    fn is_numeric(self) -> bool {
        matches!(self, Ty::U64 | Ty::I64 | Ty::U128)
    }

    fn is_unsigned(self) -> bool {
        matches!(self, Ty::U64 | Ty::U128)
    }
}

/// The type an input of `value_type` loads as: 1 `bool`, 2 `u64`, 3 `i64`, 4 `u128`, 5 `pubkey`,
/// 6 `bytes`.
fn input_type(value_type: u8, max_len: u16) -> Option<Ty> {
    Some(match value_type {
        1 => Ty::Bool,
        2 => Ty::U64,
        3 => Ty::I64,
        4 => Ty::U128,
        5 => Ty::Pubkey,
        6 => Ty::Bytes(max_len as usize),
        _ => return None,
    })
}

/// Width and result type of each read opcode, from the executor's `decode_value`.
pub fn read_opcode(opcode: u8) -> Option<(usize, Ty)> {
    Some(match opcode {
        op::READ_U8 => (1, Ty::U64),
        op::READ_U16 => (2, Ty::U64),
        op::READ_U32 => (4, Ty::U64),
        op::READ_U64 => (8, Ty::U64),
        op::READ_I32 => (4, Ty::I64),
        op::READ_I64 => (8, Ty::I64),
        op::READ_U128 => (16, Ty::U128),
        op::READ_PUBKEY => (32, Ty::Pubkey),
        op::READ_BOOL => (1, Ty::Bool),
        _ => return None,
    })
}

fn is_loop(opcode: u8) -> bool {
    matches!(opcode, op::FOREACH | op::REPEAT)
}

fn known_opcode(opcode: u8) -> bool {
    matches!(opcode, 1..=38 | 40..=80)
}

/// The fields of an instruction record one opcode uses. Every other field must be `0xff`, or zero
/// for the immediate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Fields {
    pub dst: bool,
    pub a: bool,
    pub b: bool,
    pub c: bool,
    pub imm: bool,
}

/// The fields `opcode` uses with `flags`, from `docs/reference/wire-format.md`: the destination of
/// every opcode that produces a value (all but the eight the opcode table's preamble lists), and
/// the operands and immediate its row's Operands column names. A read's `b` is its offset register
/// only under the dynamic-offset flag. `None` for an unassigned opcode.
pub fn used_fields(opcode: u8, flags: u8) -> Option<Fields> {
    let produces_value = !matches!(
        opcode,
        op::REQUIRE
            | op::INVOKE
            | op::FOREACH
            | op::REPEAT
            | op::EMIT
            | op::SET_RETURN_DATA
            | op::OPEN_REGISTRY
            | op::WRITE_REGISTRY
    );
    // (a, b, c, immediate), row by row.
    let (a, b, c, imm) = match opcode {
        op::CLOCK_SLOT | op::CLOCK_TIMESTAMP | op::LOOP_INDEX => (false, false, false, false),
        op::CONST_U64 | op::CONST_I64 | op::CONST_U128 | op::CONST_BYTES => (false, false, false, true),
        op::LOAD_INPUT
        | op::CONST_BOOL
        | op::CONST_PUBKEY
        | op::ACCOUNT_KEY
        | op::ACCOUNT_OWNER
        | op::ACCOUNT_LAMPORTS
        | op::ACCOUNT_DATA_LEN
        | op::ACCOUNT_IS_EMPTY
        | op::NOT
        | op::CAST_U64
        | op::CAST_I64
        | op::CAST_U128
        | op::MOVE
        | op::POW10
        | op::BYTES_LEN
        | op::INSTRUCTION_COUNT
        | op::INSTRUCTION_INDEX
        | op::GROUP_LENGTH
        | op::REQUIRE => (true, false, false, false),
        opcode if read_opcode(opcode).is_some() => (true, flags & FLAG_DYNAMIC_OFFSET != 0, false, true),
        op::DERIVE_PDA | op::RETURN_DATA | op::READ_REGISTRY | op::FOREACH => (true, false, false, true),
        op::ADD
        | op::SUB
        | op::MUL
        | op::DIV
        | op::REM
        | op::MIN
        | op::MAX
        | op::EQ
        | op::NE
        | op::LT
        | op::LTE
        | op::GT
        | op::GTE
        | op::AND
        | op::OR
        | op::SHL
        | op::SHR
        | op::BIT_AND
        | op::BIT_OR
        | op::BIT_XOR
        | op::INSTRUCTION_PROGRAM
        | op::INSTRUCTION_ACCOUNT_COUNT
        | op::INSTRUCTION_DATA_LEN
        | op::INVOKE => (true, true, false, false),
        op::CREATE_PDA | op::READ_ACCOUNT_BYTES | op::WRITE_REGISTRY => (true, true, false, true),
        op::SELECT | op::MUL_DIV | op::MUL_DIV_CEIL | op::INSTRUCTION_ACCOUNT | op::INSTRUCTION_ACCOUNT_FLAGS => {
            (true, true, true, false)
        }
        op::READ_INSTRUCTION_DATA
        | op::READ_INSTRUCTION_BYTES
        | op::REPEAT
        | op::OPEN_REGISTRY
        | op::GROUP_ANY
        | op::GROUP_COUNT => (true, true, true, true),
        op::EMIT | op::SET_RETURN_DATA => (false, false, false, true),
        _ => return None,
    };
    Some(Fields { dst: produces_value, a, b, c, imm })
}

/// What a data segment builds. `docs/reference/wire-format.md` gives every use one layout: a
/// literal's source register is `0xff`, and a register segment's offset and length are zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    CpiData,
    Output,
    Seed,
}

impl Role {
    fn literal_register(self) -> &'static str {
        match self {
            Role::CpiData => "cpi.segment-literal-register",
            Role::Output => "output.segment-literal-register",
            Role::Seed => "pda.segment-literal-register",
        }
    }

    fn register_fields(self) -> &'static str {
        match self {
            Role::CpiData => "cpi.segment-register-fields",
            Role::Output => "output.segment-register-fields",
            Role::Seed => "pda.segment-register-fields",
        }
    }
}

/// What a `GROUP_ANY` or `GROUP_COUNT` immediate packs ("Account groups" in
/// `docs/reference/wire-format.md`): the filter's first data segment in bytes 0 and 1, its match
/// segments in byte 2, its except segments in byte 3, and the minimum data length a member needs
/// in bytes 4 to 7. Every bit is a field, so any immediate decodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupFilter {
    pub segment_start: u16,
    pub matches: u8,
    pub excepts: u8,
    pub min_data_len: u32,
}

impl GroupFilter {
    pub fn decode(imm: u64) -> Self {
        Self {
            segment_start: (imm & 0xffff) as u16,
            matches: ((imm >> 16) & 0xff) as u8,
            excepts: ((imm >> 24) & 0xff) as u8,
            min_data_len: (imm >> 32) as u32,
        }
    }

    pub fn encode(self) -> u64 {
        self.segment_start as u64
            | (self.matches as u64) << 16
            | (self.excepts as u64) << 24
            | (self.min_data_len as u64) << 32
    }

    /// The segments the filter names, in one run: its matches, then its excepts.
    pub fn segments(self) -> core::ops::Range<usize> {
        let start = self.segment_start as usize;
        start..start + self.matches as usize + self.excepts as usize
    }
}

/// The type and width of a group filter entry's kind: one of the five fixed-width kinds, each
/// naming a register of exactly its own type. Unlike invocation data, kind 4 takes a `u64` and
/// nothing wider, and the narrow kinds 1 to 3, `bytes` and literals are no filter entry at all.
pub fn group_entry(kind: u8) -> Option<(Ty, usize)> {
    Some(match kind {
        4 => (Ty::U64, 8),
        5 => (Ty::I64, 8),
        6 => (Ty::U128, 16),
        7 => (Ty::Pubkey, 32),
        8 => (Ty::Bool, 1),
        _ => return None,
    })
}

/// Where an instruction sits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    Root,
    /// A `FOREACH` body: row accounts and row inputs resolve.
    Rows,
    /// A `REPEAT` body: a loop index, but no rows.
    Count,
}

/// A broken promise: the rule's name, the instruction it was found at, and what was seen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub rule: &'static str,
    pub pc: Option<usize>,
    pub detail: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pc {
            Some(pc) => write!(formatter, "[{}] at pc {}: {}", self.rule, pc, self.detail),
            None => write!(formatter, "[{}]: {}", self.rule, self.detail),
        }
    }
}

/// What the checker worked out about a program it accepts, to compare with the verifier's stats.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    /// CPIs in the worst case: every loop at its maximum, guarded invokes counted, and three per
    /// registry open.
    pub worst_case_cpis: usize,
    /// The largest instruction data any invoked CPI can build.
    pub max_cpi_data_len: usize,
    pub loops: usize,
    pub registry_opens: usize,
    /// Broken encoding rules that do not change how the program runs: a field its record's kind
    /// leaves unused holding something other than the canonical value, a record nothing reaches,
    /// a declared data length above the worst case. Collected, the first of each rule, without
    /// stopping, so a known one never hides a rule checked after it.
    pub notes: Vec<Violation>,
}

type Check<T = ()> = Result<T, Violation>;

fn fail<T>(rule: &'static str, pc: Option<usize>, detail: impl Into<String>) -> Check<T> {
    Err(Violation { rule, pc, detail: detail.into() })
}

fn ensure(condition: bool, rule: &'static str, pc: Option<usize>, detail: impl FnOnce() -> String) -> Check {
    if condition {
        Ok(())
    } else {
        fail(rule, pc, detail())
    }
}

/// Checks `program`, whose encoding is `payload_len` bytes long.
pub fn check(program: &Program, payload_len: usize) -> Check<Report> {
    let mut checker = Checker::new(program);
    checker.header(payload_len)?;
    checker.records()?;
    checker.walk()?;
    checker.registry_globals()?;
    checker.unreferenced()?;
    Ok(checker.report)
}

/// The register types on entry to the root instruction at `pc`, over every path, as the checker
/// tracks them; `None` if `pc` is not a root instruction or the checker stops before it. The
/// negative mutations use it to pick a register of a known type.
pub fn root_state_before(program: &Program, pc: usize) -> Option<Vec<Ty>> {
    let mut checker = Checker::new(program);
    checker.probe = Some(pc);
    let _ = checker.header(0).and_then(|_| checker.records()).and_then(|_| checker.walk());
    checker.probed
}

struct Checker<'p> {
    p: &'p Program,
    fixed: usize,
    stride: usize,
    report: Report,
    used_segments: Vec<bool>,
    used_cpis: Vec<bool>,
    /// A root pc whose entry state `walk` records in `probed`.
    probe: Option<usize>,
    probed: Option<Vec<Ty>>,
}

impl<'p> Checker<'p> {
    fn new(p: &'p Program) -> Self {
        Self {
            p,
            fixed: p.header.fixed_accounts as usize,
            stride: p.header.batch_stride as usize,
            report: Report::default(),
            used_segments: vec![false; p.segments.len()],
            used_cpis: vec![false; p.cpis.len()],
            probe: None,
            probed: None,
        }
    }

    // ---- The header and the limits it carries ------------------------------------------------

    fn header(&self, payload_len: usize) -> Check {
        let h = &self.p.header;
        ensure(payload_len <= limit::PAYLOAD, "format.payload-size", None, || {
            format!("{payload_len} bytes")
        })?;
        ensure(h.magic == crate::model::MAGIC, "format.magic", None, || format!("{:?}", h.magic))?;
        ensure(h.version == crate::model::VERSION, "format.version", None, || h.version.to_string())?;
        ensure(h.flags & !FLAG_EMIT_EVENT == 0, "format.header-flags", None, || format!("{:#x}", h.flags))?;
        ensure(h.reserved == 0, "format.header-reserved", None, || h.reserved.to_string())?;

        let fixed_inputs = h.fixed_inputs as usize;
        let row_inputs = h.row_inputs as usize;
        let max_rows = h.max_rows as usize;
        ensure(fixed_inputs <= limit::INPUTS, "limits.inputs", None, || fixed_inputs.to_string())?;
        ensure(h.registers as usize <= limit::REGISTERS, "limits.registers", None, || h.registers.to_string())?;
        ensure(
            (1..=limit::INSTRUCTIONS).contains(&(h.instructions as usize)),
            "limits.instructions",
            None,
            || h.instructions.to_string(),
        )?;
        let runtime_accounts = self.fixed + self.stride * max_rows;
        ensure(runtime_accounts <= limit::RUNTIME_ACCOUNTS, "limits.runtime-accounts", None, || {
            format!("{} fixed + {} × {} rows", self.fixed, self.stride, max_rows)
        })?;
        ensure(self.stride <= limit::BATCH_STRIDE, "limits.batch-stride", None, || self.stride.to_string())?;
        ensure((self.stride == 0) == (max_rows == 0), "loops.batch-shape", None, || {
            format!("stride {} with {} maximum rows", self.stride, max_rows)
        })?;
        ensure(h.min_rows <= h.max_rows, "loops.min-rows", None, || {
            format!("minimum {} above maximum {}", h.min_rows, h.max_rows)
        })?;
        ensure(row_inputs <= limit::ROW_INPUTS, "limits.row-inputs", None, || row_inputs.to_string())?;
        ensure(fixed_inputs + row_inputs <= limit::INPUTS, "limits.inputs", None, || {
            format!("{fixed_inputs} fixed + {row_inputs} row")
        })?;
        ensure(row_inputs == 0 || self.stride > 0, "loops.row-inputs-need-batch", None, || {
            format!("{row_inputs} row inputs without a batch")
        })?;
        let values = fixed_inputs + row_inputs * max_rows;
        ensure(values <= limit::INPUT_VALUES, "limits.input-values", None, || values.to_string())?;
        ensure(h.account_groups as usize <= limit::ACCOUNT_GROUPS, "limits.account-groups", None, || {
            h.account_groups.to_string()
        })
    }

    // ---- Every record, whether or not anything uses it ---------------------------------------

    fn records(&mut self) -> Check {
        let p = self.p;
        for (index, account) in p.accounts.iter().enumerate() {
            let pin_ok = |pin: u8| pin == NONE || (pin as usize) < p.pubkeys.len();
            ensure(
                account.flags & !(SIGNER | WRITABLE | EXECUTABLE) == 0
                    && account.reserved == 0
                    && pin_ok(account.address)
                    && pin_ok(account.owner),
                "format.account",
                None,
                || format!("account {index}: {account:?}"),
            )?;
        }
        for (index, input) in p.inputs.iter().enumerate() {
            let valid_len = if input.value_type == 6 {
                (1..=limit::BYTES).contains(&(input.max_len as usize))
            } else {
                input.max_len == 0
            };
            ensure(
                input.reserved == 0 && input_type(input.value_type, input.max_len).is_some() && valid_len,
                "format.input",
                None,
                || format!("input {index}: {input:?}"),
            )?;
        }
        for (pc, instr) in p.instrs.iter().enumerate() {
            ensure(instr.reserved == [0; 2], "format.instruction-reserved", Some(pc), || {
                format!("{:?}", instr.reserved)
            })?;
            ensure(known_opcode(instr.op), "format.unknown-opcode", Some(pc), || instr.op.to_string())?;
            let allowed = if read_opcode(instr.op).is_some() { FLAG_DYNAMIC_OFFSET } else { 0 };
            ensure(instr.flags & !allowed == 0, "format.instruction-flags", Some(pc), || {
                format!("flags {:#x} on opcode {}", instr.flags, instr.op)
            })?;
            let used = used_fields(instr.op, instr.flags).expect("a known opcode");
            let canonical = (used.dst || instr.dst == NONE)
                && (used.a || instr.a == NONE)
                && (used.b || instr.b == NONE)
                && (used.c || instr.c == NONE)
                && (used.imm || instr.imm == 0);
            self.note(canonical, "format.unused-field", Some(pc), || format!("{instr:?} uses only {used:?}"));
        }
        for (index, cpi) in p.cpis.iter().enumerate() {
            self.cpi_shape(index, cpi)?;
        }
        Ok(())
    }

    /// What `docs/reference/wire-format.md` says of every CPI descriptor.
    fn cpi_shape(&self, index: usize, cpi: &crate::model::Cpi) -> Check {
        let p = self.p;
        let detail = || format!("descriptor {index}: {cpi:?}");
        ensure(cpi.reserved == [0; 2], "format.cpi-reserved", None, detail)?;
        ensure(
            cpi.group == NONE || cpi.group < p.header.account_groups,
            "cpi.group-out-of-range",
            None,
            detail,
        )?;
        ensure(cpi.account_len as usize <= limit::CPI_ACCOUNTS, "limits.cpi-accounts", None, detail)?;
        ensure(cpi.max_data_len as usize <= limit::CPI_DATA, "limits.cpi-data", None, detail)?;
        ensure(
            cpi.account_start as usize + cpi.account_len as usize <= p.cpi_accounts.len(),
            "cpi.account-range",
            None,
            detail,
        )?;
        ensure(
            cpi.segment_start as usize + cpi.segment_len as usize <= p.segments.len(),
            "cpi.segment-range",
            None,
            detail,
        )
    }

    // ---- The program, path by path -----------------------------------------------------------

    fn walk(&mut self) -> Check {
        let p = self.p;
        let count = p.instrs.len();
        let mut state = vec![Ty::Undef; p.header.registers as usize];
        let mut previous: Option<Instr> = None;
        let mut row_loops = 0usize;
        let mut pc = 0usize;
        while pc < count {
            let instr = p.instrs[pc];
            if self.probe == Some(pc) {
                self.probed = Some(state.clone());
            }
            if !is_loop(instr.op) {
                let invokes = self.step(pc, Scope::Root, &mut state, previous)?;
                self.report.worst_case_cpis += invokes;
                previous = Some(instr);
                pc += 1;
                continue;
            }

            self.report.loops += 1;
            ensure(self.report.loops <= limit::LOOPS, "loops.count", Some(pc), || {
                format!("loop number {}", self.report.loops)
            })?;
            let body_len = instr.a as usize;
            let body_end = pc + 1 + body_len;
            ensure(body_len >= 1, "loops.empty-body", Some(pc), String::new)?;
            ensure(body_end <= count, "loops.body-out-of-range", Some(pc), || {
                format!("body ends at {body_end} of {count}")
            })?;
            for inner in pc + 1..body_end {
                ensure(!is_loop(p.instrs[inner].op), "loops.nested", Some(inner), String::new)?;
            }
            let (scope, max_passes) = if instr.op == op::FOREACH {
                ensure(self.stride > 0, "loops.foreach-needs-batch", Some(pc), String::new)?;
                row_loops += 1;
                (Scope::Rows, p.header.max_rows as usize)
            } else {
                ensure(instr.c >= 1, "loops.repeat-max", Some(pc), String::new)?;
                ensure(instr.dst == NONE, "format.repeat-destination", Some(pc), || instr.dst.to_string())?;
                self.need(&state, instr.b, Some(pc), "the REPEAT count", |ty| ty == Ty::U64)?;
                (Scope::Count, instr.c as usize)
            };
            // The body is checked inside the pass loop below, so a loop must allow a pass. The
            // header rules already imply it; stated here so the body can never go unchecked.
            ensure(max_passes >= 1, "loops.no-pass", Some(pc), String::new)?;

            // Carried registers: created before the loop, and of the same type after each pass
            // (`docs/reference/language.md`, "Carried values").
            let carry = instr.imm;
            let carried: Vec<usize> = (0..64).filter(|bit| carry & (1u64 << bit) != 0).collect();
            for &register in &carried {
                ensure(register < state.len(), "loops.carry-out-of-range", Some(pc), || {
                    format!("r{register} of {}", state.len())
                })?;
                ensure(state[register].is_set(), "loops.carry-unset", Some(pc), || format!("r{register}"))?;
            }

            // Pass k starts from the registers as the loop found them, with each carried register
            // as pass k - 1 left it: the executor snapshots the registers at entry, copies the
            // carried ones into the snapshot after each pass, and restores it. After the loop the
            // registers are the snapshot, or the entry state when no pass runs. `pass_start` joins
            // every pass's starting state seen so far, so checking the body from it covers each
            // of them, and the loop stops once another pass adds nothing new.
            let entry = state.clone();
            let mut pass_start = entry.clone();
            let mut after = entry.clone();
            let mut body_invokes = 0usize;
            for _pass in 0..max_passes {
                let mut body = pass_start.clone();
                body_invokes = self.range(pc + 1, body_end, scope, &mut body)?;
                for &register in &carried {
                    ensure(body[register] == entry[register], "loops.carry-type", Some(pc), || {
                        format!("r{register} enters as {:?}, leaves as {:?}", entry[register], body[register])
                    })?;
                }
                let snapshot: Vec<Ty> = (0..entry.len())
                    .map(|register| if carried.contains(&register) { body[register] } else { entry[register] })
                    .collect();
                after = join_states(&after, &snapshot);
                let next = join_states(&pass_start, &snapshot);
                if next == pass_start {
                    break;
                }
                pass_start = next;
            }
            self.report.worst_case_cpis += body_invokes * max_passes;
            state = after;
            previous = None;
            pc = body_end;
        }

        ensure((self.stride > 0) == (row_loops > 0), "loops.batch-needs-foreach", None, || {
            format!("stride {} with {row_loops} FOREACH loops", self.stride)
        })?;
        ensure(self.report.worst_case_cpis <= limit::CPIS, "limits.cpis", None, || {
            format!("{} CPIs in the worst case", self.report.worst_case_cpis)
        })
    }

    /// Checks a loop body from `state`, returning the invokes it holds.
    fn range(&mut self, start: usize, end: usize, scope: Scope, state: &mut [Ty]) -> Check<usize> {
        let mut invokes = 0;
        let mut previous: Option<Instr> = None;
        for pc in start..end {
            invokes += self.step(pc, scope, state, previous)?;
            previous = Some(self.p.instrs[pc]);
        }
        Ok(invokes)
    }

    // ---- Operands ----------------------------------------------------------------------------

    /// The register's type, if it is in range and set on every path.
    fn read(&self, state: &[Ty], register: u8, pc: Option<usize>, role: &str) -> Check<Ty> {
        let Some(&ty) = state.get(register as usize) else {
            return fail("types.register-out-of-range", pc, format!("{role}: r{register} of {}", state.len()));
        };
        ensure(ty.is_set(), "types.read-before-write", pc, || format!("{role}: r{register}"))?;
        Ok(ty)
    }

    /// As [`Checker::read`], and the type must satisfy `accepts`.
    fn need(
        &self,
        state: &[Ty],
        register: u8,
        pc: Option<usize>,
        role: &str,
        accepts: impl Fn(Ty) -> bool,
    ) -> Check<Ty> {
        let ty = self.read(state, register, pc, role)?;
        ensure(accepts(ty), "types.mismatch", pc, || format!("{role}: r{register} holds {ty:?}"))?;
        Ok(ty)
    }

    fn write(&self, state: &mut [Ty], register: u8, ty: Ty, pc: usize) -> Check {
        let Some(slot) = state.get_mut(register as usize) else {
            return fail("types.register-out-of-range", Some(pc), format!("destination r{register}"));
        };
        *slot = ty;
        Ok(())
    }

    /// The constraint an account reference names, in this scope.
    fn account(&self, reference: u8, scope: Scope, pc: usize) -> Check<&'p Account> {
        let p = self.p;
        if reference & ROW_BIT == 0 {
            ensure((reference as usize) < self.fixed, "accounts.fixed-out-of-range", Some(pc), || {
                format!("account {reference} of {}", self.fixed)
            })?;
            return Ok(&p.accounts[reference as usize]);
        }
        ensure(scope == Scope::Rows, "accounts.row-outside-foreach", Some(pc), || {
            format!("row account {:#x} in {scope:?}", reference)
        })?;
        let offset = (reference & !ROW_BIT) as usize;
        ensure(offset < self.stride, "accounts.row-out-of-range", Some(pc), || {
            format!("row account {offset} of {}", self.stride)
        })?;
        Ok(&p.accounts[self.fixed + offset])
    }

    /// A fixed account whose address is pinned to `key`.
    fn pinned(&self, reference: u8, key: &[u8; 32]) -> bool {
        let p = self.p;
        if reference & ROW_BIT != 0 || reference as usize >= self.fixed {
            return false;
        }
        let address = p.accounts[reference as usize].address;
        address != NONE && p.pubkeys.get(address as usize) == Some(key)
    }

    /// Every account `OPEN_REGISTRY` names as an entry, by reference.
    fn entry_accounts(&self) -> Vec<u8> {
        self.p.instrs.iter().filter(|instr| instr.op == op::OPEN_REGISTRY).map(|instr| instr.a).collect()
    }

    /// The most bytes `segment` encodes, as the executor's `encode_segment` does. The two rules on
    /// fields a segment's kind leaves unused are named by `role`, so a gap in one use cannot hide
    /// the same gap in another.
    fn segment(&mut self, index: usize, state: &[Ty], pc: usize, role: Role) -> Check<usize> {
        let segment: Segment = self.p.segments[index];
        let detail = || format!("segment {index}: {segment:?}");
        ensure(segment.reserved == [0; 2], "format.segment-reserved", Some(pc), detail)?;
        if segment.kind == 0 {
            self.note(segment.register == NONE, role.literal_register(), Some(pc), detail);
            ensure(
                segment.offset as usize + segment.len as usize <= self.p.blob.len(),
                "bounds.segment-literal",
                Some(pc),
                detail,
            )?;
            return Ok(segment.len as usize);
        }
        self.note(segment.offset == 0 && segment.len == 0, role.register_fields(), Some(pc), detail);
        let role = "a data segment";
        let register = segment.register;
        Ok(match segment.kind {
            1..=4 => {
                self.need(state, register, Some(pc), role, Ty::is_unsigned)?;
                [1, 2, 4, 8][segment.kind as usize - 1]
            }
            5 => {
                self.need(state, register, Some(pc), role, |ty| ty == Ty::I64)?;
                8
            }
            6 => {
                self.need(state, register, Some(pc), role, |ty| ty == Ty::U128)?;
                16
            }
            7 => {
                self.need(state, register, Some(pc), role, |ty| ty == Ty::Pubkey)?;
                32
            }
            8 => {
                self.need(state, register, Some(pc), role, |ty| ty == Ty::Bool)?;
                1
            }
            9 => match self.need(state, register, Some(pc), role, Ty::is_bytes)? {
                Ty::Bytes(max) => max,
                _ => unreachable!("is_bytes"),
            },
            _ => return fail("format.segment-kind", Some(pc), detail()),
        })
    }

    /// The segments a packed range names, each checked, and their total worst-case length.
    fn segments(&mut self, start: usize, len: usize, state: &[Ty], pc: usize, role: Role) -> Check<Vec<usize>> {
        ensure(
            start.checked_add(len).is_some_and(|end| end <= self.p.segments.len()),
            "bounds.segment-range",
            Some(pc),
            || format!("segments {start}+{len} of {}", self.p.segments.len()),
        )?;
        let mut widths = Vec::with_capacity(len);
        for index in start..start + len {
            self.used_segments[index] = true;
            widths.push(self.segment(index, state, pc, role)?);
        }
        Ok(widths)
    }

    // ---- One instruction ---------------------------------------------------------------------

    /// Checks the instruction at `pc` and applies it to `state`. Returns the CPIs it can make.
    fn step(&mut self, pc: usize, scope: Scope, state: &mut [Ty], previous: Option<Instr>) -> Check<usize> {
        let p = self.p;
        let i = p.instrs[pc];
        let at = Some(pc);
        match i.op {
            op::LOAD_INPUT => {
                let fixed_inputs = p.header.fixed_inputs as usize;
                let index = if i.a & ROW_BIT == 0 {
                    ensure((i.a as usize) < fixed_inputs, "inputs.fixed-out-of-range", at, || {
                        format!("input {} of {fixed_inputs}", i.a)
                    })?;
                    i.a as usize
                } else {
                    ensure(scope == Scope::Rows, "inputs.row-outside-foreach", at, || format!("{scope:?}"))?;
                    let offset = (i.a & !ROW_BIT) as usize;
                    ensure(offset < p.header.row_inputs as usize, "inputs.row-out-of-range", at, || {
                        format!("row input {offset} of {}", p.header.row_inputs)
                    })?;
                    fixed_inputs + offset
                };
                let input = p.inputs[index];
                let ty = input_type(input.value_type, input.max_len).expect("records() checked the type");
                self.write(state, i.dst, ty, pc)?;
            }
            op::CONST_BOOL => {
                ensure(i.a <= 1, "format.const-bool", at, || i.a.to_string())?;
                self.write(state, i.dst, Ty::Bool, pc)?;
            }
            op::CONST_U64 => self.write(state, i.dst, Ty::U64, pc)?,
            op::CONST_I64 => self.write(state, i.dst, Ty::I64, pc)?,
            op::CONST_U128 => {
                let (offset, len) = i.range();
                ensure(len == 16 && offset + len <= p.blob.len(), "bounds.blob", at, || format!("{offset}+{len}"))?;
                self.write(state, i.dst, Ty::U128, pc)?;
            }
            op::CONST_PUBKEY => {
                ensure((i.a as usize) < p.pubkeys.len(), "bounds.pubkey", at, || i.a.to_string())?;
                self.write(state, i.dst, Ty::Pubkey, pc)?;
            }
            op::CONST_BYTES => {
                let (offset, len) = i.range();
                ensure(offset + len <= p.blob.len() && len <= limit::BYTES, "bounds.blob", at, || {
                    format!("{offset}+{len} of {}", p.blob.len())
                })?;
                self.write(state, i.dst, Ty::Bytes(len), pc)?;
            }
            op::ACCOUNT_KEY | op::ACCOUNT_OWNER => {
                self.account(i.a, scope, pc)?;
                self.write(state, i.dst, Ty::Pubkey, pc)?;
            }
            op::ACCOUNT_LAMPORTS | op::ACCOUNT_DATA_LEN => {
                self.account(i.a, scope, pc)?;
                self.write(state, i.dst, Ty::U64, pc)?;
            }
            op::ACCOUNT_IS_EMPTY => {
                self.account(i.a, scope, pc)?;
                self.write(state, i.dst, Ty::Bool, pc)?;
            }
            opcode if read_opcode(opcode).is_some() => {
                let (width, ty) = read_opcode(opcode).expect("guarded");
                let account = self.account(i.a, scope, pc)?;
                ensure(!self.entry_accounts().contains(&i.a), "registry.entry-data-read", at, || {
                    format!("account {} is a registry entry", i.a)
                })?;
                if i.flags & FLAG_DYNAMIC_OFFSET != 0 {
                    self.need(state, i.b, at, "the offset", |ty| ty == Ty::U64)?;
                    ensure(i.imm == 0, "format.dynamic-read-immediate", at, || i.imm.to_string())?;
                } else {
                    let end = (i.imm as u128) + width as u128;
                    ensure(end <= account.min_len as u128, "bounds.fixed-read", at, || {
                        format!("{} + {width} past a minimum length of {}", i.imm, account.min_len)
                    })?;
                }
                self.write(state, i.dst, ty, pc)?;
            }
            op::CLOCK_SLOT => self.write(state, i.dst, Ty::U64, pc)?,
            op::CLOCK_TIMESTAMP => self.write(state, i.dst, Ty::I64, pc)?,
            op::ADD | op::SUB | op::MUL | op::DIV | op::MIN | op::MAX | op::REM => {
                let left = self.read(state, i.a, at, "left")?;
                let right = self.read(state, i.b, at, "right")?;
                ensure(left == right && left.is_numeric(), "types.mismatch", at, || {
                    format!("{left:?} and {right:?}")
                })?;
                self.write(state, i.dst, left, pc)?;
            }
            op::SHL | op::SHR => {
                let value = self.need(state, i.a, at, "the value", Ty::is_unsigned)?;
                self.need(state, i.b, at, "the shift", |ty| ty == Ty::U64)?;
                self.write(state, i.dst, value, pc)?;
            }
            op::BIT_AND | op::BIT_OR | op::BIT_XOR => {
                let left = self.read(state, i.a, at, "left")?;
                let right = self.read(state, i.b, at, "right")?;
                ensure(left == right && left.is_unsigned(), "types.mismatch", at, || {
                    format!("{left:?} and {right:?}")
                })?;
                self.write(state, i.dst, left, pc)?;
            }
            op::MUL_DIV | op::MUL_DIV_CEIL => {
                let a = self.read(state, i.a, at, "a")?;
                let b = self.read(state, i.b, at, "b")?;
                let c = self.read(state, i.c, at, "c")?;
                ensure(a == b && b == c && a.is_unsigned(), "types.mismatch", at, || {
                    format!("{a:?}, {b:?} and {c:?}")
                })?;
                self.write(state, i.dst, a, pc)?;
            }
            op::POW10 => {
                self.need(state, i.a, at, "the exponent", |ty| ty == Ty::U64)?;
                self.write(state, i.dst, Ty::U128, pc)?;
            }
            op::EQ | op::NE => {
                let left = self.read(state, i.a, at, "left")?;
                let right = self.read(state, i.b, at, "right")?;
                ensure(left.same_kind(right), "types.mismatch", at, || format!("{left:?} and {right:?}"))?;
                self.write(state, i.dst, Ty::Bool, pc)?;
            }
            op::LT | op::LTE | op::GT | op::GTE => {
                let left = self.read(state, i.a, at, "left")?;
                let right = self.read(state, i.b, at, "right")?;
                ensure(left == right && left.is_numeric(), "types.mismatch", at, || {
                    format!("{left:?} and {right:?}")
                })?;
                self.write(state, i.dst, Ty::Bool, pc)?;
            }
            op::AND | op::OR => {
                self.need(state, i.a, at, "left", |ty| ty == Ty::Bool)?;
                self.need(state, i.b, at, "right", |ty| ty == Ty::Bool)?;
                self.write(state, i.dst, Ty::Bool, pc)?;
            }
            op::NOT => {
                self.need(state, i.a, at, "the operand", |ty| ty == Ty::Bool)?;
                self.write(state, i.dst, Ty::Bool, pc)?;
            }
            op::SELECT => {
                // The executor reads only the side the condition picks, so either side can be the
                // result, and both must be set. The wire format also requires "the same type as
                // b" of `c`; a `bytes` result is as long as the longer side.
                self.need(state, i.a, at, "the condition", |ty| ty == Ty::Bool)?;
                let if_true = self.read(state, i.b, at, "if true")?;
                let if_false = self.read(state, i.c, at, "if false")?;
                ensure(if_true.same_kind(if_false), "types.mismatch", at, || {
                    format!("select between {if_true:?} and {if_false:?}")
                })?;
                self.write(state, i.dst, if_true.join(if_false), pc)?;
            }
            op::CAST_U64 | op::CAST_I64 | op::CAST_U128 => {
                self.need(state, i.a, at, "the source", Ty::is_numeric)?;
                let target = [Ty::U64, Ty::I64, Ty::U128][(i.op - op::CAST_U64) as usize];
                self.write(state, i.dst, target, pc)?;
            }
            op::LOOP_INDEX => {
                ensure(scope != Scope::Root, "loops.index-outside-loop", at, String::new)?;
                self.write(state, i.dst, Ty::U64, pc)?;
            }
            op::MOVE => {
                let ty = self.read(state, i.a, at, "the source")?;
                self.write(state, i.dst, ty, pc)?;
            }
            op::DERIVE_PDA | op::CREATE_PDA => {
                let program = self.account(i.a, scope, pc)?;
                ensure(program.flags & EXECUTABLE != 0, "accounts.pda-program-not-executable", at, || {
                    format!("account {}", i.a)
                })?;
                if i.op == op::CREATE_PDA {
                    self.need(state, i.b, at, "the bump", |ty| ty == Ty::U64)?;
                }
                let (start, len) = i.range();
                ensure((1..=limit::PDA_SEEDS).contains(&len), "limits.pda-seeds", at, || len.to_string())?;
                for (seed, width) in self.segments(start, len, state, pc, Role::Seed)?.into_iter().enumerate() {
                    ensure(width <= limit::PDA_SEED_LEN, "limits.pda-seed-len", at, || {
                        format!("seed {seed} can be {width} bytes")
                    })?;
                }
                self.write(state, i.dst, Ty::Pubkey, pc)?;
            }
            op::REQUIRE => {
                self.need(state, i.a, at, "the condition", |ty| ty == Ty::Bool)?;
            }
            op::RETURN_DATA => {
                ensure(
                    previous.is_some_and(|before| before.op == op::INVOKE && before.b == NONE),
                    "return-data.not-after-unguarded-invoke",
                    at,
                    || format!("previous instruction {previous:?}"),
                )?;
                let Some((width, ty)) = read_opcode(i.a) else {
                    return fail("return-data.selector", at, i.a.to_string());
                };
                ensure((i.imm as u128) + width as u128 <= limit::RETURN_DATA as u128, "return-data.range", at, || {
                    format!("{} + {width}", i.imm)
                })?;
                self.write(state, i.dst, ty, pc)?;
            }
            op::INVOKE => {
                if i.b != NONE {
                    self.need(state, i.b, at, "the guard", |ty| ty == Ty::Bool)?;
                }
                self.invoke(i.a as usize, scope, state, pc)?;
                return Ok(1);
            }
            op::FOREACH | op::REPEAT => return fail("loops.nested", at, String::new()),
            op::EMIT | op::SET_RETURN_DATA => self.output(i, scope, state, pc)?,
            op::INSTRUCTION_COUNT | op::INSTRUCTION_INDEX => {
                self.sysvar(i.a, pc)?;
                self.write(state, i.dst, Ty::U64, pc)?;
            }
            op::INSTRUCTION_PROGRAM | op::INSTRUCTION_ACCOUNT_COUNT | op::INSTRUCTION_DATA_LEN => {
                self.sysvar(i.a, pc)?;
                self.need(state, i.b, at, "the index", |ty| ty == Ty::U64)?;
                let ty = if i.op == op::INSTRUCTION_PROGRAM { Ty::Pubkey } else { Ty::U64 };
                self.write(state, i.dst, ty, pc)?;
            }
            op::INSTRUCTION_ACCOUNT | op::INSTRUCTION_ACCOUNT_FLAGS => {
                self.sysvar(i.a, pc)?;
                self.need(state, i.b, at, "the index", |ty| ty == Ty::U64)?;
                self.need(state, i.c, at, "the position", |ty| ty == Ty::U64)?;
                let ty = if i.op == op::INSTRUCTION_ACCOUNT { Ty::Pubkey } else { Ty::U64 };
                self.write(state, i.dst, ty, pc)?;
            }
            op::READ_INSTRUCTION_DATA => {
                self.sysvar(i.a, pc)?;
                self.need(state, i.b, at, "the index", |ty| ty == Ty::U64)?;
                self.need(state, i.c, at, "the offset", |ty| ty == Ty::U64)?;
                let selector = u8::try_from(i.imm).ok().and_then(read_opcode);
                let Some((_, ty)) = selector else {
                    return fail("introspection.selector", at, i.imm.to_string());
                };
                self.write(state, i.dst, ty, pc)?;
            }
            op::READ_INSTRUCTION_BYTES => {
                self.sysvar(i.a, pc)?;
                self.need(state, i.b, at, "the index", |ty| ty == Ty::U64)?;
                self.need(state, i.c, at, "the offset", |ty| ty == Ty::U64)?;
                ensure((1..=limit::BYTES as u64).contains(&i.imm), "limits.byte-read", at, || i.imm.to_string())?;
                self.write(state, i.dst, Ty::Bytes(i.imm as usize), pc)?;
            }
            op::READ_ACCOUNT_BYTES => {
                let account = self.account(i.a, scope, pc)?;
                ensure(!self.entry_accounts().contains(&i.a), "registry.entry-data-read", at, || {
                    format!("account {} is a registry entry", i.a)
                })?;
                // A fixed account declared writable is writable in every run, and the run refuses
                // a byte read of an account it can write, so the read could never succeed.
                ensure(
                    i.a & ROW_BIT != 0 || account.flags & WRITABLE == 0,
                    "bytes.writable-account",
                    at,
                    || format!("account {}", i.a),
                )?;
                self.need(state, i.b, at, "the offset", |ty| ty == Ty::U64)?;
                ensure((1..=limit::BYTES as u64).contains(&i.imm), "limits.byte-read", at, || i.imm.to_string())?;
                self.write(state, i.dst, Ty::Bytes(i.imm as usize), pc)?;
            }
            op::BYTES_LEN => {
                self.need(state, i.a, at, "the value", Ty::is_bytes)?;
                self.write(state, i.dst, Ty::U64, pc)?;
            }
            op::OPEN_REGISTRY => {
                self.open_registry(pc, scope, state)?;
                return Ok(limit::REGISTRY_OPEN_CPIS);
            }
            op::READ_REGISTRY => {
                ensure(i.b == NONE && i.c == NONE, "format.registry-read-operands", at, || {
                    format!("b {} c {}", i.b, i.c)
                })?;
                let (_, ty) = self.registry_field(pc, i.a, false)?;
                self.write(state, i.dst, ty, pc)?;
            }
            op::WRITE_REGISTRY => {
                ensure(i.dst == NONE && i.c == NONE, "format.registry-write-operands", at, || {
                    format!("dst {} c {}", i.dst, i.c)
                })?;
                let (_, ty) = self.registry_field(pc, i.b, true)?;
                self.need(state, i.a, at, "the value", |held| held == ty)?;
            }
            // Account groups, in every scope: the caller sizes them, so a loop body sees the same
            // members as the root.
            op::GROUP_LENGTH => {
                ensure(i.b == NONE && i.c == NONE && i.imm == 0, "format.group-length-operands", at, || {
                    format!("b {} c {} immediate {:#x}", i.b, i.c, i.imm)
                })?;
                self.group(i.a, pc)?;
                self.write(state, i.dst, Ty::U64, pc)?;
            }
            op::GROUP_ANY | op::GROUP_COUNT => {
                self.group_filter(i, state, pc)?;
                let ty = if i.op == op::GROUP_ANY { Ty::Bool } else { Ty::U64 };
                self.write(state, i.dst, ty, pc)?;
            }
            _ => return fail("format.unknown-opcode", at, i.op.to_string()),
        }
        Ok(0)
    }

    fn sysvar(&self, reference: u8, pc: usize) -> Check {
        ensure(self.pinned(reference, &INSTRUCTIONS_SYSVAR), "introspection.sysvar", Some(pc), || {
            format!("account {reference} is not a fixed account pinned to the Instructions sysvar")
        })
    }

    /// An invoke of descriptor `index` in `scope`: the privilege ceiling, the executable program,
    /// and the worst-case data against the declared maximum.
    fn invoke(&mut self, index: usize, scope: Scope, state: &[Ty], pc: usize) -> Check {
        let p = self.p;
        let at = Some(pc);
        let Some(cpi) = p.cpis.get(index).copied() else {
            return fail("cpi.out-of-range", at, format!("descriptor {index} of {}", p.cpis.len()));
        };
        self.used_cpis[index] = true;
        let program = self.account(cpi.program, scope, pc)?;
        ensure(program.flags & EXECUTABLE != 0, "cpi.program-not-executable", at, || {
            format!("program account {}", cpi.program)
        })?;
        let records = &p.cpi_accounts[cpi.account_start as usize..cpi.account_start as usize + cpi.account_len as usize];
        for record in records {
            ensure(record.flags & !(SIGNER | WRITABLE) == 0, "cpi.account-flags", at, || format!("{record:?}"))?;
            let declared = self.account(record.account, scope, pc)?;
            ensure(record.flags & !declared.flags == 0, "cpi.privilege-ceiling", at, || {
                format!(
                    "account {:#x} passed with flags {:#x}, declared {:#x}",
                    record.account, record.flags, declared.flags
                )
            })?;
        }
        let worst: usize = self
            .segments(cpi.segment_start as usize, cpi.segment_len as usize, state, pc, Role::CpiData)?
            .into_iter()
            .sum();
        ensure(worst <= limit::CPI_DATA, "limits.cpi-data", at, || format!("{worst} bytes"))?;
        ensure(worst <= cpi.max_data_len as usize, "cpi.data-above-declared", at, || {
            format!("{worst} bytes against a declared {}", cpi.max_data_len)
        })?;
        self.note(worst == cpi.max_data_len as usize, "format.cpi-declared-data-len", at, || {
            format!("declared {} for a worst case of {worst}", cpi.max_data_len)
        });
        self.report.max_cpi_data_len = self.report.max_cpi_data_len.max(worst);
        Ok(())
    }

    /// `EMIT` and `SET_RETURN_DATA` ("Output" in `docs/reference/language.md`).
    fn output(&mut self, i: Instr, scope: Scope, state: &[Ty], pc: usize) -> Check {
        let p = self.p;
        let at = Some(pc);
        ensure([i.dst, i.a, i.b, i.c] == [NONE; 4], "format.output-operands", at, || {
            format!("{:?}", [i.dst, i.a, i.b, i.c])
        })?;
        let (start, len) = i.range();
        ensure(len >= 1, "output.empty", at, String::new)?;
        let total: usize = self.segments(start, len, state, pc, Role::Output)?.into_iter().sum();
        ensure(total <= limit::OUTPUT, "limits.output", at, || format!("{total} bytes"))?;
        if i.op == op::EMIT {
            let tag = p.segments[start];
            let bytes = (tag.kind == 0).then(|| &p.blob[tag.offset as usize..tag.offset as usize + tag.len as usize]);
            ensure(
                bytes.is_some_and(|tag| tag.len() >= limit::EMIT_TAG && !tag.starts_with(b"BEV")),
                "output.emit-tag",
                at,
                || format!("first segment {tag:?}"),
            )?;
            return Ok(());
        }
        // `SET_RETURN_DATA`: once, outside every loop, and nothing that calls a program after it.
        ensure(scope == Scope::Root, "output.return-data-in-loop", at, || format!("{scope:?}"))?;
        let sets = p.instrs.iter().filter(|instr| instr.op == op::SET_RETURN_DATA).count();
        ensure(sets == 1, "output.return-data-once", at, || format!("{sets} SET_RETURN_DATA"))?;
        for (later, instr) in p.instrs.iter().enumerate().skip(pc + 1) {
            ensure(
                !matches!(instr.op, op::INVOKE | op::OPEN_REGISTRY),
                "output.return-data-before-call",
                at,
                || format!("opcode {} at {later}", instr.op),
            )?;
        }
        Ok(())
    }

    /// `OPEN_REGISTRY` ("Registries" in `docs/reference/wire-format.md`).
    fn open_registry(&mut self, pc: usize, scope: Scope, state: &[Ty]) -> Check {
        let p = self.p;
        let i = p.instrs[pc];
        let at = Some(pc);
        ensure(scope == Scope::Root, "registry.open-in-loop", at, || format!("{scope:?}"))?;
        ensure(i.dst == NONE, "format.registry-open-destination", at, || i.dst.to_string())?;
        ensure(i.imm >> 32 == 0, "format.registry-open-immediate", at, || format!("{:#x}", i.imm))?;
        let index = (i.imm & 0xff) as usize;
        let size = ((i.imm >> 8) & 0xffff) as usize;
        let system = ((i.imm >> 24) & 0xff) as u8;
        ensure(index < limit::REGISTRIES, "registry.index", at, || index.to_string())?;
        ensure((1..=limit::REGISTRY_SIZE).contains(&size), "registry.size", at, || size.to_string())?;

        ensure(i.a & ROW_BIT == 0, "registry.entry-not-fixed", at, || format!("{:#x}", i.a))?;
        let entry = self.account(i.a, Scope::Root, pc)?;
        ensure(
            entry.flags == WRITABLE && entry.address == NONE && entry.owner == NONE && entry.min_len == 0,
            "registry.entry-declaration",
            at,
            || format!("{entry:?}"),
        )?;
        ensure(i.c & ROW_BIT == 0, "registry.payer-not-fixed", at, || format!("{:#x}", i.c))?;
        let payer = self.account(i.c, Scope::Root, pc)?;
        ensure(payer.flags & (SIGNER | WRITABLE) == SIGNER | WRITABLE, "registry.payer", at, || {
            format!("{payer:?}")
        })?;
        ensure(self.pinned(system, &SYSTEM_PROGRAM), "registry.system-program", at, || {
            format!("account {system}")
        })?;
        if i.b != NONE {
            self.need(state, i.b, at, "the key", |ty| ty == Ty::Pubkey)?;
        }

        let opens: Vec<(usize, Instr)> =
            p.instrs.iter().copied().enumerate().filter(|(_, instr)| instr.op == op::OPEN_REGISTRY).collect();
        ensure(opens.len() <= limit::REGISTRY_OPENS, "registry.opens", at, || opens.len().to_string())?;
        for (other_pc, other) in &opens {
            if *other_pc == pc {
                continue;
            }
            ensure(other.a != i.a, "registry.entry-opened-twice", at, || format!("also at {other_pc}"))?;
            let other_index = (other.imm & 0xff) as usize;
            let other_size = ((other.imm >> 8) & 0xffff) as usize;
            ensure(other_index != index || other_size == size, "registry.size-differs", at, || {
                format!("registry {index}: {size} here, {other_size} at {other_pc}")
            })?;
        }
        ensure(
            !p.instrs[..pc].iter().any(|instr| instr.op == op::SET_RETURN_DATA),
            "registry.open-after-return-data",
            at,
            String::new,
        )?;
        // Until its open marks the entry, a CPI could reach it through another slot or a group.
        ensure(!p.instrs[..pc].iter().any(|instr| instr.op == op::INVOKE), "registry.open-after-invoke", at, || {
            let invoke = p.instrs[..pc].iter().position(|instr| instr.op == op::INVOKE).unwrap_or_default();
            format!("invoke at {invoke}")
        })?;
        self.report.registry_opens += 1;
        Ok(())
    }

    /// The field a registry read or write names on `entry`: a valid selector, an open of that
    /// account at a lower pc, and a range inside that registry's size.
    fn registry_field(&self, pc: usize, entry: u8, write: bool) -> Check<(usize, Ty)> {
        let p = self.p;
        let i = p.instrs[pc];
        let at = Some(pc);
        ensure(i.imm >> 24 == 0, "format.registry-field-immediate", at, || format!("{:#x}", i.imm))?;
        let offset = (i.imm & 0xffff) as usize;
        let selector = ((i.imm >> 16) & 0xff) as u8;
        let Some((width, ty)) = read_opcode(selector) else {
            return fail("registry.field-selector", at, selector.to_string());
        };
        if write {
            ensure(
                matches!(selector, op::READ_BOOL | op::READ_U64 | op::READ_I64 | op::READ_U128 | op::READ_PUBKEY),
                "registry.write-selector",
                at,
                || selector.to_string(),
            )?;
        }
        let open = p.instrs[..pc].iter().find(|instr| instr.op == op::OPEN_REGISTRY && instr.a == entry);
        let Some(open) = open else {
            return fail("registry.field-before-open", at, format!("account {entry}"));
        };
        let size = ((open.imm >> 8) & 0xffff) as usize;
        ensure(offset + width <= size, "registry.field-range", at, || {
            format!("{offset} + {width} past {size} bytes")
        })?;
        Ok((offset, ty))
    }

    /// An account group the header declares.
    fn group(&self, group: u8, pc: usize) -> Check {
        let declared = self.p.header.account_groups;
        ensure(group < declared, "groups.undeclared", Some(pc), || format!("group {group} of {declared}"))
    }

    /// `GROUP_ANY` and `GROUP_COUNT` ("Account groups" in `docs/reference/wire-format.md`, and
    /// what the executor's `count_matches` reads): a declared group; one or two programs a
    /// member's owner must be, as pubkey table indices, `c` `0xff` for one; 1 to 4 matches and at
    /// most 4 excepts, in one run inside the segment table. Entry by entry: zero reserved and
    /// length fields, a fixed-width kind ([`group_entry`]), an except a `pubkey` at offset zero
    /// (a match's offset is where its value sits in a member's data), and a register set with
    /// exactly the kind's type. Last, the minimum data length covers every match's bytes, so the
    /// run compares only bytes a member it tests has. The run's segments count as used.
    fn group_filter(&mut self, i: Instr, state: &[Ty], pc: usize) -> Check {
        let p = self.p;
        let at = Some(pc);
        self.group(i.a, pc)?;
        let pubkeys = p.pubkeys.len();
        ensure(
            (i.b as usize) < pubkeys && (i.c == NONE || (i.c as usize) < pubkeys),
            "groups.program",
            at,
            || format!("programs {} and {} of {pubkeys} pubkeys", i.b, i.c),
        )?;
        let filter = GroupFilter::decode(i.imm);
        let (matches, excepts) = (filter.matches as usize, filter.excepts as usize);
        ensure((1..=limit::GROUP_MATCHES).contains(&matches), "groups.match-count", at, || matches.to_string())?;
        ensure(excepts <= limit::GROUP_EXCEPTS, "groups.except-count", at, || excepts.to_string())?;
        let run = filter.segments();
        ensure(run.end <= p.segments.len(), "groups.segment-range", at, || {
            format!("segments {}..{} of {}", run.start, run.end, p.segments.len())
        })?;
        let mut floor = 0usize;
        for (position, index) in run.enumerate() {
            self.used_segments[index] = true;
            let segment = p.segments[index];
            let detail = || format!("segment {index}: {segment:?}");
            ensure(segment.reserved == [0; 2] && segment.len == 0, "groups.segment-fields", at, detail)?;
            let Some((ty, width)) = group_entry(segment.kind) else {
                return fail("groups.segment-kind", at, detail());
            };
            let role = if position < matches {
                floor = floor.max(segment.offset as usize + width);
                "a group filter's match"
            } else {
                ensure(ty == Ty::Pubkey && segment.offset == 0, "groups.except", at, detail)?;
                "a group filter's except"
            };
            self.need(state, segment.register, at, role, |held| held == ty)?;
        }
        ensure(filter.min_data_len as usize >= floor, "groups.floor", at, || {
            format!("a minimum of {} bytes under matches that reach {floor}", filter.min_data_len)
        })
    }

    /// No CPI account record lists an entry account writable, whatever invokes it or not.
    fn registry_globals(&self) -> Check {
        let entries = self.entry_accounts();
        for (index, record) in self.p.cpi_accounts.iter().enumerate() {
            ensure(
                !(entries.contains(&record.account) && record.flags & WRITABLE != 0),
                "registry.entry-passed-writable",
                None,
                || format!("CPI account record {index} passes entry {} writable", record.account),
            )?;
        }
        Ok(())
    }

    /// Nothing is left unreached (`docs/reference/wire-format.md`, "CPI descriptors" and "Data
    /// segments"): every descriptor is invoked, and every data segment is part of an invoked
    /// descriptor's data, an output, a PDA's seeds or a group filter. Unreached, a record would
    /// never be checked.
    fn unreferenced(&mut self) -> Check {
        let p = self.p;
        for (index, segment) in p.segments.iter().enumerate() {
            let used = self.used_segments[index];
            self.note(used, "unreferenced.segment", None, || format!("segment {index}: {segment:?}"));
        }
        for (index, cpi) in p.cpis.iter().enumerate() {
            let used = self.used_cpis[index];
            self.note(used, "unreferenced.cpi", None, || format!("descriptor {index}, which nothing invokes: {cpi:?}"));
        }
        Ok(())
    }

    /// Records a broken encoding rule without stopping; see [`Report::notes`].
    fn note(&mut self, holds: bool, rule: &'static str, pc: Option<usize>, detail: impl FnOnce() -> String) {
        if !holds && !self.report.notes.iter().any(|note| note.rule == rule) {
            self.report.notes.push(Violation { rule, pc, detail: detail() });
        }
    }
}

fn join_states(left: &[Ty], right: &[Ty]) -> Vec<Ty> {
    left.iter().zip(right).map(|(left, right)| left.join(*right)).collect()
}
