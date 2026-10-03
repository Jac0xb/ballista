//! Random templates that the verifier accepts by construction, covering every opcode family the
//! executor runs: typed reads at fixed and computed offsets, byte reads, PDAs, introspection,
//! outputs, registries, account-group lengths and filters, both loop kinds with carried values, and
//! invocations of the System program, the probe program, the Token program and Ballista itself.
//!
//! Unlike `ballista_common::template::generate`, which avoids every value-dependent failure but
//! overflow, these templates read account data the caller supplies and call programs, so runs fail
//! in every way the executor can fail. The harness checks invariants on each outcome rather than
//! expecting success.

use ballista_common::template::*;

use crate::source::Gen;

/// The probe program's operations, chosen by the first byte of its instruction data.
pub mod probe {
    /// Records the call and does nothing else.
    pub const NOOP: u8 = 0;
    /// Sets the rest of the data as return data.
    pub const SET_RETURN: u8 = 1;
    /// Copies the rest of the data over the start of the first account's data, if the probe owns
    /// it and it is writable.
    pub const WRITE_FIRST: u8 = 2;
    /// Resizes the first account's data to `4 * data[1]` bytes, if the probe owns it and it is
    /// writable.
    pub const RESIZE_FIRST: u8 = 3;
    /// Fails with `FAIL_BASE | data[1]`.
    pub const FAIL: u8 = 4;
    /// Invokes the program of its first account with the rest of its accounts and `data[2..]`.
    /// `data[1]` is a policy: bit `n` set demotes the writable flag of the forwarded account `n`.
    /// From inside a Ballista run this is reentrancy, which the runtime refuses.
    pub const INVOKE: u8 = 5;
    /// Moves `data[1]` lamports from the first account to the second, if the probe owns the
    /// first and both are writable.
    pub const TRANSFER: u8 = 6;
    /// The custom error code base of `FAIL`. Distinct from every code Ballista or the System
    /// program raises, so a pass-through is recognizable.
    pub const FAIL_BASE: u32 = 0x0fee_d000;
}

/// A program a template can call or derive a PDA with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Program {
    System,
    Probe,
    Ballista,
    Token,
    /// The probe's program loaded at a second address, so a batch can call a different program
    /// in each row with the same data and every call succeeds: what a CPI into a row-account
    /// program needs to be checked per row.
    ProbeCopy,
}

pub const ALL_PROGRAMS: [Program; 5] = [Program::System, Program::Probe, Program::Ballista, Program::Token, Program::ProbeCopy];

/// Addresses the generators know about. The harness creates every one of these accounts.
#[derive(Clone, Debug)]
pub struct World {
    pub ballista: [u8; 32],
    pub probe: [u8; 32],
    pub token: [u8; 32],
    /// Where the harness loads the probe a second time: the probe's address with its last byte
    /// flipped.
    pub probe_copy: [u8; 32],
    /// System-owned accounts a template may pin by address.
    pub users: Vec<[u8; 32]>,
    /// Probe-owned accounts a template may pin by address.
    pub probe_data: Vec<[u8; 32]>,
}

impl World {
    pub fn new(ballista: [u8; 32], probe: [u8; 32], token: [u8; 32]) -> Self {
        let tagged = |tag: u8, index: u8| {
            let mut address = [tag; 32];
            address[0] = index;
            address[31] = 0x5a;
            address
        };
        let mut probe_copy = probe;
        probe_copy[31] ^= 0xff;
        Self {
            ballista,
            probe,
            token,
            probe_copy,
            users: (0..6).map(|index| tagged(0xa1, index)).collect(),
            probe_data: (0..4).map(|index| tagged(0xb2, index)).collect(),
        }
    }

    pub fn program(&self, program: Program) -> [u8; 32] {
        match program {
            Program::System => SYSTEM_PROGRAM_ADDRESS,
            Program::Probe => self.probe,
            Program::Ballista => self.ballista,
            Program::Token => self.token,
            Program::ProbeCopy => self.probe_copy,
        }
    }
}

/// What a declared account is for, which tells the account generator what to supply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Plain,
    Program(Program),
    /// The Instructions sysvar, pinned.
    Sysvar,
    /// A registry payer: signer and writable.
    Payer,
    /// The entry account of `opens[n]`.
    Entry(usize),
    /// The first account of a nested Ballista run: a template account.
    NestedTemplate,
}

/// One declared account.
#[derive(Clone, Debug)]
pub struct Slot {
    pub flags: u8,
    pub address: Option<[u8; 32]>,
    pub owner: Option<[u8; 32]>,
    pub min_len: u32,
    pub role: Role,
}

/// Where a registry open takes its key from. Restricted to sources the account generator can
/// predict, so it can derive the entry's address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySource {
    Zero,
    /// The key of fixed account `n`.
    SlotKey(usize),
    /// The fixed `pubkey` input `n`.
    Input(usize),
    Const([u8; 32]),
}

/// One `OPEN_REGISTRY`.
#[derive(Clone, Debug)]
pub struct Open {
    pub entry: usize,
    pub payer: usize,
    pub system: usize,
    pub index: u8,
    pub size: u16,
    pub key: KeySource,
    pub pc: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub value_type: u8,
    pub max_len: u16,
}

/// Where a group filter's match value or except key comes from: what the account generator needs
/// to supply a member that holds the value, or that sits at the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterValue {
    /// A constant, as invocation data encodes it.
    Bytes(Vec<u8>),
    /// The fixed input `n`, encoded as its type.
    Input(usize),
    /// The key of fixed account `n`.
    SlotKey(usize),
    /// A register whose value only the run knows.
    Unknown,
}

/// One `GROUP_ANY` or `GROUP_COUNT`: what a member of `group` must be to match.
#[derive(Clone, Debug)]
pub struct GroupFilter {
    pub group: usize,
    /// The one or two programs a member's owner must be.
    pub programs: Vec<[u8; 32]>,
    pub min_data_len: u32,
    /// `(data offset, value)` of each match.
    pub matches: Vec<(usize, FilterValue)>,
    /// The except keys: a member at one of them never matches.
    pub excepts: Vec<FilterValue>,
}

/// A generated template and everything the account and invariant checks need to know about it.
#[derive(Clone, Debug)]
pub struct TemplatePlan {
    pub bytes: Vec<u8>,
    pub fixed: Vec<Slot>,
    pub row: Vec<Slot>,
    pub batch_max: usize,
    pub batch_min: usize,
    pub inputs: Vec<Input>,
    pub row_inputs: Vec<Input>,
    pub groups: usize,
    /// Every `GROUP_ANY` and `GROUP_COUNT`, so the account generator can supply members that pass
    /// each of a filter's tests and members that fail one.
    pub group_filters: Vec<GroupFilter>,
    pub opens: Vec<Open>,
    /// `(open, offset past the header, width)` of every `WRITE_REGISTRY`.
    pub registry_writes: Vec<(usize, u16, usize)>,
    /// The literal tag of every `EMIT`.
    pub emit_tags: Vec<Vec<u8>>,
    pub sets_return_data: bool,
    pub emits_event: bool,
    pub calls: Vec<Program>,
    pub reads_clock: bool,
    /// True when generation overran a limit and the minimal fallback template was returned.
    pub fell_back: bool,
    /// Half the templates, and their runs, avoid the deliberate faults: small constants and inputs,
    /// counts and offsets in range, no probe failures or junk calls, valid accounts and run data.
    /// So more runs succeed and the model compares them; the other half keeps every fault.
    pub friendly: bool,
}

impl TemplatePlan {
    pub fn stride(&self) -> usize {
        self.row.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    Bool,
    U64,
    I64,
    U128,
    Pubkey,
    Bytes(u16),
}

impl Ty {
    fn numeric(self) -> bool {
        matches!(self, Ty::U64 | Ty::I64 | Ty::U128)
    }

    fn unsigned(self) -> bool {
        matches!(self, Ty::U64 | Ty::U128)
    }

    fn from_value_type(value_type: u8, max_len: u16) -> Ty {
        match value_type {
            VALUE_BOOL => Ty::Bool,
            VALUE_U64 => Ty::U64,
            VALUE_I64 => Ty::I64,
            VALUE_U128 => Ty::U128,
            VALUE_PUBKEY => Ty::Pubkey,
            _ => Ty::Bytes(max_len),
        }
    }

    fn from_read(selector: u8) -> Ty {
        match read_type(selector) {
            VALUE_BOOL => Ty::Bool,
            VALUE_I64 => Ty::I64,
            VALUE_U128 => Ty::U128,
            VALUE_PUBKEY => Ty::Pubkey,
            _ => Ty::U64,
        }
    }

    /// Width when encoded as data, and an encoding its type accepts.
    fn encoding(self, g: &mut Gen) -> (u8, usize) {
        match self {
            Ty::Bool => (DATA_REG_BOOL, 1),
            Ty::I64 => (DATA_REG_I64, 8),
            Ty::Pubkey => (DATA_REG_PUBKEY, 32),
            Ty::Bytes(len) => (DATA_REG_BYTES, len as usize),
            Ty::U128 if g.chance(1, 2) => (DATA_REG_U128, 16),
            Ty::U64 | Ty::U128 => {
                let kind = [DATA_REG_U8, DATA_REG_U16, DATA_REG_U32, DATA_REG_U64][g.below(4)];
                (kind, segment_width(kind) as usize)
            }
        }
    }
}

const READS: [u8; 9] = [
    OP_READ_U8,
    OP_READ_U16,
    OP_READ_U32,
    OP_READ_U64,
    OP_READ_I64,
    OP_READ_U128,
    OP_READ_PUBKEY,
    OP_READ_BOOL,
    OP_READ_I32,
];

/// Field types a registry can declare, with the selector a write uses.
const FIELD_TYPES: [(Ty, u8, u16); 5] = [
    (Ty::Bool, OP_READ_BOOL, 1),
    (Ty::U64, OP_READ_U64, 8),
    (Ty::I64, OP_READ_I64, 8),
    (Ty::U128, OP_READ_U128, 16),
    (Ty::Pubkey, OP_READ_PUBKEY, 32),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Root,
    Rows,
    Count,
}

/// What the instruction just emitted leaves for a `RETURN_DATA` read: the most bytes the call
/// could return. Present only right after an unguarded invoke.
#[derive(Clone, Copy, Debug)]
struct ReturnShape {
    max_len: usize,
}

/// Instructions and registers kept free for the epilogue.
const INSTRUCTION_LIMIT: usize = 108;
const REGISTER_LIMIT: u8 = 58;

/// How far the template generator pushes the wire format.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Generate at the format's limits instead of well inside them; see [`Config::limits`].
    pub limits: bool,
}

impl Config {
    /// Templates at the format's limits: up to `MAX_VM_INSTRUCTIONS` (128) instructions and
    /// `MAX_REGISTERS` (64) registers, invokes whose worst-case count reaches `MAX_EXPANDED_CPIS`
    /// (64), CPIs listing up to `MAX_CPI_ACCOUNTS` (64) accounts, batches of up to 64 rows, and
    /// batches whose rows name the program a CPI calls (a row account with the executable flag,
    /// supplied as the probe or its copy, so each row may call a different program).
    pub fn limits() -> Self {
        Self { limits: true }
    }
}

struct TemplateGen<'a, 'g> {
    g: &'a mut Gen<'g>,
    world: &'a World,
    limits: bool,
    instruction_limit: usize,
    register_limit: u8,
    b: ProgramBuilder,
    plan: TemplatePlan,
    regs: Vec<Option<Ty>>,
    scope: Scope,
    /// Passes the current range makes at most: 1 at the root.
    passes: usize,
    cpi_budget: usize,
    loops: usize,
    /// Registers a loop carries; the body must keep their types.
    carried: Vec<u8>,
    after_invoke: Option<ReturnShape>,
    /// Field layout of each registry index: `(type, write selector, offset)`.
    layouts: [Option<Vec<(Ty, u8, u16)>>; MAX_REGISTRIES],
    /// Opens emitted so far, by index into `plan.opens`.
    opened: Vec<usize>,
}

/// Generates one template. The result verifies unless the generator has a bug; callers check.
pub fn generate_template(g: &mut Gen, world: &World) -> TemplatePlan {
    generate_template_with(g, world, &Config::default())
}

/// Generates one template under `config`.
pub fn generate_template_with(g: &mut Gen, world: &World, config: &Config) -> TemplatePlan {
    let mut generator = TemplateGen {
        g,
        world,
        limits: config.limits,
        // At the limits the whole instruction and register budget is the generator's; the
        // epilogue's reserve below still keeps a forced loop and the output in reach.
        instruction_limit: if config.limits { MAX_VM_INSTRUCTIONS } else { INSTRUCTION_LIMIT },
        register_limit: if config.limits { MAX_REGISTERS as u8 - 3 } else { REGISTER_LIMIT },
        b: ProgramBuilder::new(),
        plan: TemplatePlan {
            bytes: Vec::new(),
            fixed: Vec::new(),
            row: Vec::new(),
            batch_max: 0,
            batch_min: 0,
            inputs: Vec::new(),
            row_inputs: Vec::new(),
            groups: 0,
            group_filters: Vec::new(),
            opens: Vec::new(),
            registry_writes: Vec::new(),
            emit_tags: Vec::new(),
            sets_return_data: false,
            emits_event: false,
            calls: Vec::new(),
            reads_clock: false,
            fell_back: false,
            friendly: false,
        },
        regs: vec![None; MAX_REGISTERS],
        scope: Scope::Root,
        passes: 1,
        cpi_budget: MAX_EXPANDED_CPIS,
        loops: 0,
        carried: Vec::new(),
        after_invoke: None,
        layouts: Default::default(),
        opened: Vec::new(),
    };
    generator.run();
    let plan = generator.plan;
    // The generator's contract is that every template it returns verifies. If a draw combination
    // overran a limit anyway, fall back to a minimal valid template rather than hand the harness
    // one the program rejects; the harness counts these so the rate stays visible.
    match ProgramView::parse(&plan.bytes).and_then(|program| program.verify()) {
        Ok(_) => plan,
        Err(_) => minimal_plan(),
    }
}

/// The smallest valid template: require a true constant. Used only as a fallback.
fn minimal_plan() -> TemplatePlan {
    let mut builder = ProgramBuilder::new();
    let flag = builder.const_bool(true);
    builder.require(flag);
    let bytes = builder.build().expect("minimal template builds");
    TemplatePlan {
        bytes,
        fixed: Vec::new(),
        row: Vec::new(),
        batch_max: 0,
        batch_min: 0,
        inputs: Vec::new(),
        row_inputs: Vec::new(),
        groups: 0,
        group_filters: Vec::new(),
        opens: Vec::new(),
        registry_writes: Vec::new(),
        emit_tags: Vec::new(),
        sets_return_data: false,
        emits_event: false,
        calls: Vec::new(),
        reads_clock: false,
        fell_back: true,
        friendly: false,
    }
}

impl TemplateGen<'_, '_> {
    fn run(&mut self) {
        self.plan.friendly = self.g.chance(1, 2);
        self.declare_accounts();
        self.declare_inputs();
        // Each open counts as three calls: creating a pre-funded entry takes three.
        self.cpi_budget = MAX_EXPANDED_CPIS - REGISTRY_OPEN_CPIS * self.plan.opens.len();
        if self.g.chance(1, 3) {
            self.plan.emits_event = true;
            self.b.flags(PROGRAM_FLAG_EMIT_EVENT);
        }

        // Some templates do work before their opens, never a call: the verifier refuses an open
        // after an invoke (`statement` picks a value instead).
        let early = if self.g.chance(1, 4) { self.g.range(1, 4) } else { 0 };
        for _ in 0..early {
            self.statement();
        }
        for open in 0..self.plan.opens.len() {
            self.emit_open(open);
        }

        let segments = self.g.range(1, 4);
        let mut foreach_left = usize::from(self.plan.batch_max > 0);
        for segment in 0..segments {
            for _ in 0..self.g.range(1, 4) {
                self.statement();
            }
            let last = segment + 1 == segments;
            if self.loops < MAX_LOOPS && self.room(20) {
                if foreach_left > 0 && (last || self.g.chance(1, 2)) {
                    self.emit_loop(true);
                    foreach_left -= 1;
                } else if self.g.chance(1, 3) {
                    let foreach = self.plan.batch_max > 0 && self.g.chance(1, 3);
                    self.emit_loop(foreach);
                }
            }
        }
        if foreach_left > 0 {
            // A batch needs a FOREACH; keep one even when the instruction budget is tight.
            self.emit_loop(true);
        }
        for _ in 0..self.g.below(3) {
            self.statement();
        }
        // Half the templates end by logging their latest values, so what the template computed,
        // and not just what it passed to a call, is compared with the model.
        if self.g.chance(1, 2) {
            let latest: Vec<u8> = (0..self.b.register_count()).rev().filter(|&r| self.regs[r as usize].is_some()).take(4).collect();
            self.emit_registers(&latest);
        }
        if self.limits {
            self.spend_cpi_budget();
        }
        if self.g.chance(1, 3) && self.room(4) {
            self.emit_output(false);
        }
        if self.limits {
            self.fill_to_the_limits();
        }
        self.plan.bytes = self.b.build().expect("generated templates stay under the payload limit");
    }

    /// At the limits: a `REPEAT` whose maximum is the whole CPI budget left, around one probe
    /// call, so the worst-case invoke count reaches exactly `MAX_EXPANDED_CPIS`. Its count is
    /// usually small, so a run makes a few of those calls and stays under the runtime's
    /// instruction-trace limit of 64.
    fn spend_cpi_budget(&mut self) {
        let Some(probe) = self.program_slot(Program::Probe) else { return };
        if self.cpi_budget == 0 || self.loops >= MAX_LOOPS || !self.room(6) || !self.g.chance(3, 4) {
            return;
        }
        let max = self.cpi_budget.min(255);
        let value = if self.g.chance(7, 8) { self.g.below(4) as u64 } else { self.g.below(max + 1) as u64 };
        let count = self.b.const_u64(value);
        self.define(count, Ty::U64);
        self.mark();
        let start = self.b.emit(record(OP_REPEAT, NO_INDEX, 0, count, max as u8, 0, 0));
        let noop = self.b.blob(&[probe::NOOP]);
        let cpi = self.b.cpi(probe, &[], &[Segment::Literal(noop)]);
        self.b.set_cpi_max_data_len(cpi, 1);
        self.b.invoke(cpi, None);
        let body = self.b.instructions_mut().len() - start - 1;
        self.b.instructions_mut()[start].a = body as u8;
        self.loops += 1;
        self.cpi_budget = 0;
        if !self.plan.calls.contains(&Program::Probe) {
            self.plan.calls.push(Program::Probe);
        }
    }

    /// At the limits: constants until the register file is full, then requirements of a true
    /// constant until the instruction count is, so the template's header and the executor's
    /// register file sit at their maximum.
    fn fill_to_the_limits(&mut self) {
        if self.b.register_count() as usize >= MAX_REGISTERS || self.b.instructions_mut().len() >= MAX_VM_INSTRUCTIONS {
            return;
        }
        let truth = self.b.const_bool(true);
        self.define(truth, Ty::Bool);
        while self.b.instructions_mut().len() < MAX_VM_INSTRUCTIONS {
            if (self.b.register_count() as usize) < MAX_REGISTERS {
                let ty = self.random_type();
                self.constant(ty);
            } else {
                self.b.require(truth);
            }
        }
    }

    // ---- declarations -------------------------------------------------------------------------

    fn fixed(&mut self, flags: u8, address: Option<[u8; 32]>, owner: Option<[u8; 32]>, min_len: u32, role: Role) -> usize {
        let reference = self.b.account(flags, address, owner, min_len);
        self.plan.fixed.push(Slot { flags, address, owner, min_len, role });
        debug_assert_eq!(reference as usize, self.plan.fixed.len() - 1);
        reference as usize
    }

    fn slot_of_role(&self, role: &Role) -> Option<usize> {
        self.plan.fixed.iter().position(|slot| &slot.role == role)
    }

    fn declare_accounts(&mut self) {
        let world = self.world;
        let mut programs = Vec::new();
        // At the limits the probe is always there: it takes the many-account calls and spends the
        // CPI budget.
        if self.limits || self.g.chance(2, 3) {
            programs.push(Program::Probe);
        }
        if self.g.chance(1, 2) {
            programs.push(Program::System);
        }
        if self.g.chance(1, 8) {
            programs.push(Program::Token);
        }
        if self.g.chance(1, 8) {
            programs.push(Program::Ballista);
        }
        for program in programs {
            // Unpinned programs let the caller pick any executable account.
            let address = (program == Program::System || self.g.chance(7, 8)).then(|| world.program(program));
            self.fixed(ACCOUNT_EXECUTABLE, address, None, 0, Role::Program(program));
        }
        if self.g.chance(1, 4) {
            self.fixed(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0, Role::Sysvar);
        }
        for _ in 0..self.g.range(0, 5) {
            self.plain_fixed();
        }
        if self.slot_of_role(&Role::Program(Program::Ballista)).is_some() {
            // The account a nested run names as its template: read-only, owned by Ballista.
            let owner = self.g.chance(1, 2).then_some(world.ballista);
            self.fixed(0, None, owner, 0, Role::NestedTemplate);
        }

        if self.g.chance(1, 3) {
            self.declare_registries();
        }

        if self.g.chance(2, 5) || (self.limits && self.g.chance(1, 2)) {
            let stride = if self.limits { self.g.range(1, 2) } else { self.g.range(1, 3) };
            // At the limits, half the batches name a program in each row, which the body calls.
            let program_row = self.limits && self.g.chance(1, 2);
            for position in 0..stride {
                let slot = if program_row && position == 0 {
                    Slot { flags: ACCOUNT_EXECUTABLE, address: None, owner: None, min_len: 0, role: Role::Program(Program::Probe) }
                } else {
                    self.random_slot(true)
                };
                let reference = self.b.row_account(slot.flags, slot.address, slot.owner, slot.min_len);
                debug_assert_eq!(reference & !ITERATION_ACCOUNT_BIT, self.plan.row.len() as u8);
                self.plan.row.push(slot);
            }
            // At the limits, up to 64 rows when the runtime accounts allow: at most 120, and the
            // fixed ones and the groups need room too.
            let max = if self.limits && self.g.chance(1, 2) {
                let room = 100usize.saturating_sub(self.plan.fixed.len()) / stride;
                self.g.range(1, room.clamp(1, 64))
            } else {
                self.g.range(1, 5)
            };
            let min = self.g.below(max + 1);
            self.plan.batch_max = max;
            self.plan.batch_min = min;
            self.b.batch(max as u8, min as u8);
        }
        if self.g.chance(1, 3) {
            self.plan.groups = self.g.range(1, 2);
            self.b.account_groups(self.plan.groups as u8);
        }
    }

    fn random_slot(&mut self, row: bool) -> Slot {
        let world = self.world;
        let mut flags = 0;
        if self.g.chance(1, 3) {
            flags |= ACCOUNT_SIGNER;
        }
        if self.g.chance(2, 5) {
            flags |= ACCOUNT_WRITABLE;
        }
        if !row && self.g.chance(1, 16) {
            flags |= ACCOUNT_EXECUTABLE;
        }
        let address = (!row && self.g.chance(1, 8)).then(|| {
            let mut pool = world.users.clone();
            pool.extend(world.probe_data.iter().copied());
            pool[self.g.below(pool.len())]
        });
        let mut owner = self.g.chance(2, 5).then(|| match self.g.below(5) {
            0 | 1 => world.probe,
            2 => SYSTEM_PROGRAM_ADDRESS,
            3 => world.ballista,
            _ => world.token,
        });
        let mut min_len = if self.g.chance(1, 2) { 0 } else { self.g.range(1, 96) as u32 };
        // A real template pins facts that can hold together. Usually make them consistent: a pinned
        // address keeps its own owner and no length floor (its data varies run to run), a program
        // has its loader as owner and no floor, and a signer is never a Ballista PDA. Rarely leave
        // them as drawn, so a run still meets declarations no account satisfies.
        if !self.g.chance(1, 20) {
            if let Some(address) = address {
                let natural = if world.probe_data.contains(&address) { world.probe } else { SYSTEM_PROGRAM_ADDRESS };
                owner = owner.map(|_| natural);
                min_len = 0;
            }
            if flags & ACCOUNT_EXECUTABLE != 0 {
                owner = None;
                min_len = 0;
            }
            if flags & ACCOUNT_SIGNER != 0 && owner == Some(world.ballista) {
                owner = None;
            }
        }
        Slot { flags, address, owner, min_len, role: Role::Plain }
    }

    fn plain_fixed(&mut self) -> usize {
        let slot = self.random_slot(false);
        self.fixed(slot.flags, slot.address, slot.owner, slot.min_len, Role::Plain)
    }

    fn declare_registries(&mut self) {
        let system = match self.slot_of_role(&Role::Program(Program::System)) {
            Some(slot) => slot,
            None => self.fixed(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0, Role::Program(Program::System)),
        };
        let payer = self.fixed(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0, Role::Payer);
        let opens = self.g.range(1, 3);
        for _ in 0..opens {
            let index = if self.g.chance(1, 2) { 0 } else { self.g.below(MAX_REGISTRIES) as u8 };
            if self.layouts[index as usize].is_none() {
                let mut layout = Vec::new();
                let mut offset = 0u16;
                for _ in 0..self.g.range(1, 4) {
                    let (ty, selector, width) = FIELD_TYPES[self.g.below(FIELD_TYPES.len())];
                    layout.push((ty, selector, offset));
                    offset += width;
                }
                self.layouts[index as usize] = Some(layout);
            }
            let size = self.layout_size(index);
            let entry = self.fixed(ACCOUNT_WRITABLE, None, None, 0, Role::Entry(self.plan.opens.len()));
            let key = match self.g.below(5) {
                0 | 1 => KeySource::Zero,
                2 => {
                    let candidates: Vec<usize> = (0..self.plan.fixed.len())
                        .filter(|slot| matches!(self.plan.fixed[*slot].role, Role::Plain | Role::Payer))
                        .collect();
                    match self.g.pick(&candidates) {
                        Some(slot) => KeySource::SlotKey(slot),
                        None => KeySource::SlotKey(payer),
                    }
                }
                3 => KeySource::Input(usize::MAX),
                _ => {
                    let mut key = [0u8; 32];
                    key[0] = self.g.u8();
                    key[1] = 0x77;
                    KeySource::Const(key)
                }
            };
            self.plan.opens.push(Open { entry, payer, system, index, size, key, pc: usize::MAX });
        }
    }

    /// The fields' bytes plus a few unused ones for some indexes. A function of the index alone,
    /// so every open of one registry declares one size, as the verifier requires.
    fn layout_size(&self, index: u8) -> u16 {
        let layout = self.layouts[index as usize].as_ref().expect("declared");
        let (ty, _, offset) = *layout.last().expect("at least one field");
        let width = FIELD_TYPES.iter().find(|(t, _, _)| *t == ty).map(|(_, _, w)| *w).unwrap();
        offset + width + (index as u16 * 3) % 5
    }

    fn declare_inputs(&mut self) {
        for _ in 0..self.g.range(0, 5) {
            let input = self.random_input();
            self.b.input(input.value_type, input.max_len);
            self.plan.inputs.push(input);
        }
        // Keys a registry takes from an input need a pubkey input.
        for open in 0..self.plan.opens.len() {
            if self.plan.opens[open].key == KeySource::Input(usize::MAX) {
                let existing: Vec<usize> = (0..self.plan.inputs.len())
                    .filter(|index| self.plan.inputs[*index].value_type == VALUE_PUBKEY)
                    .collect();
                let index = match self.g.pick(&existing) {
                    Some(index) => index,
                    None => {
                        let input = Input { value_type: VALUE_PUBKEY, max_len: 0 };
                        self.b.input(input.value_type, input.max_len);
                        self.plan.inputs.push(input);
                        self.plan.inputs.len() - 1
                    }
                };
                self.plan.opens[open].key = KeySource::Input(index);
            }
        }
        if self.plan.batch_max > 0 {
            for _ in 0..self.g.range(0, 2) {
                let input = self.random_input();
                self.b.row_input(input.value_type, input.max_len);
                self.plan.row_inputs.push(input);
            }
        }
    }

    fn random_input(&mut self) -> Input {
        let types = [VALUE_BOOL, VALUE_U64, VALUE_I64, VALUE_U128, VALUE_PUBKEY, VALUE_BYTES];
        let value_type = types[self.g.weighted(&[2, 5, 2, 2, 2, 2])];
        let max_len = if value_type == VALUE_BYTES { self.g.range(1, 40) as u16 } else { 0 };
        Input { value_type, max_len }
    }

    // ---- registers ----------------------------------------------------------------------------

    fn room(&mut self, instructions: usize) -> bool {
        self.b.instructions_mut().len() + instructions + 8 <= self.instruction_limit
            && self.b.register_count() < self.register_limit
    }

    fn of(&self, predicate: impl Fn(Ty) -> bool) -> Vec<u8> {
        (0..self.b.register_count())
            .filter(|register| self.regs[*register as usize].is_some_and(&predicate))
            .collect()
    }

    fn ty(&self, register: u8) -> Ty {
        self.regs[register as usize].expect("a set register")
    }

    fn define(&mut self, register: u8, ty: Ty) -> u8 {
        self.regs[register as usize] = Some(ty);
        register
    }

    /// A register of type `ty`: an existing one, or a fresh constant.
    fn need(&mut self, ty: Ty) -> u8 {
        let existing = self.of(|candidate| candidate == ty);
        if !existing.is_empty() && self.g.chance(3, 4) {
            return existing[self.g.below(existing.len())];
        }
        self.constant(ty)
    }

    fn small_u64(&mut self, bound: usize) -> u8 {
        let value = self.g.below(bound) as u64;
        let register = self.b.const_u64(value);
        self.define(register, Ty::U64)
    }

    fn constant(&mut self, ty: Ty) -> u8 {
        let friendly = self.plan.friendly;
        let register = match ty {
            Ty::Bool => self.b.const_bool(self.g.chance(2, 3)),
            Ty::U64 => {
                let value = if friendly { self.g.below(1000) as u64 } else { self.g.interesting_u64() };
                self.b.const_u64(value)
            }
            Ty::I64 => {
                let value = if friendly { self.g.below(2001) as i64 - 1000 } else { self.g.interesting_i64() };
                self.b.const_i64(value)
            }
            Ty::U128 => {
                let value = if friendly { self.g.below(1000) as u128 } else { self.g.interesting_u128() };
                self.b.const_u128(value)
            }
            Ty::Pubkey => {
                let value = self.interesting_pubkey();
                self.b.const_pubkey(value)
            }
            Ty::Bytes(_) => {
                let len = self.g.range(0, 24);
                let bytes = self.g.bytes(len);
                let register = self.b.const_bytes(&bytes);
                return self.define(register, Ty::Bytes(len as u16));
            }
        };
        self.define(register, ty)
    }

    fn interesting_pubkey(&mut self) -> [u8; 32] {
        let world = self.world;
        match self.g.below(6) {
            0 => world.users[self.g.below(world.users.len())],
            1 => world.probe_data[self.g.below(world.probe_data.len())],
            2 => world.program(ALL_PROGRAMS[self.g.below(4)]),
            3 => [0; 32],
            _ => {
                let mut key = [0u8; 32];
                key[0] = self.g.u8();
                key[1] = self.g.u8();
                key
            }
        }
    }

    fn random_type(&mut self) -> Ty {
        match self.g.weighted(&[2, 5, 2, 2, 2, 1]) {
            0 => Ty::Bool,
            1 => Ty::U64,
            2 => Ty::I64,
            3 => Ty::U128,
            4 => Ty::Pubkey,
            _ => Ty::Bytes(0),
        }
    }

    // ---- accounts in scope ---------------------------------------------------------------------

    /// Account references usable here with their slots.
    fn accounts(&self) -> Vec<(u8, Slot)> {
        let mut accounts: Vec<(u8, Slot)> = self
            .plan
            .fixed
            .iter()
            .enumerate()
            .map(|(index, slot)| (index as u8, slot.clone()))
            .collect();
        if self.scope == Scope::Rows {
            for (offset, slot) in self.plan.row.iter().enumerate() {
                accounts.push((ITERATION_ACCOUNT_BIT | offset as u8, slot.clone()));
            }
        }
        accounts
    }

    fn is_entry(slot: &Slot) -> bool {
        matches!(slot.role, Role::Entry(_))
    }

    // ---- statements ---------------------------------------------------------------------------

    fn statement(&mut self) {
        if !self.room(3) {
            return;
        }
        let invoke_weight = if self.limits { 9 } else { 5 };
        match self.g.weighted(&[14, 2, invoke_weight, 2, 2]) {
            0 => self.value(),
            1 => self.require(),
            2 if self.opened.len() < self.plan.opens.len() => self.value(),
            2 => self.invoke(),
            3 => self.emit_output(true),
            _ => self.write_registry(),
        }
    }

    fn mark(&mut self) {
        self.after_invoke = None;
    }

    fn require(&mut self) {
        let condition = if self.g.chance(4, 5) {
            let register = self.b.const_bool(true);
            self.define(register, Ty::Bool)
        } else {
            self.need(Ty::Bool)
        };
        self.mark();
        self.b.require(condition);
    }

    fn value(&mut self) {
        // Group expressions only where the template declares a group; a zero weight leaves every
        // other template's draws as they were.
        let group_weight = if self.plan.groups > 0 { 6 } else { 0 };
        let choice = self.g.weighted(&[6, 4, 4, 3, 2, 2, 1, 6, 3, 2, 2, 2, 3, 2, 3, 1, 2, 2, 2, group_weight]);
        let before = self.b.instructions_mut().len();
        match choice {
            0 => {
                let ty = self.random_type();
                self.constant(ty);
            }
            1 => self.load_input(),
            2 => self.account_field(),
            3 => self.read_fixed(),
            4 => self.read_dynamic(),
            5 => self.read_account_bytes(),
            6 => {
                if let Some(bytes) = self.g.pick(&self.of(|ty| matches!(ty, Ty::Bytes(_)))) {
                    let register = self.b.bytes_len(bytes);
                    self.define(register, Ty::U64);
                }
            }
            7 => self.arithmetic(),
            8 => self.compare(),
            9 => self.logic(),
            10 => self.select(),
            11 => self.cast(),
            12 => self.math(),
            13 => self.pda(),
            14 => self.introspect(),
            15 => {
                self.plan.reads_clock = true;
                if self.g.chance(1, 2) {
                    let register = self.b.clock_slot();
                    self.define(register, Ty::U64);
                } else {
                    let register = self.b.clock_timestamp();
                    self.define(register, Ty::I64);
                }
            }
            16 => {
                if self.scope != Scope::Root {
                    let register = self.b.loop_index();
                    self.define(register, Ty::U64);
                }
            }
            17 => self.mov(),
            18 => self.read_registry(),
            _ => self.group(),
        }
        if self.b.instructions_mut().len() != before {
            self.mark();
        }
    }

    fn load_input(&mut self) {
        let mut choices: Vec<(u8, Input)> = self
            .plan
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| (index as u8, *input))
            .collect();
        if self.scope == Scope::Rows {
            for (index, input) in self.plan.row_inputs.iter().enumerate() {
                choices.push((ITERATION_INPUT_BIT | index as u8, *input));
            }
        }
        if let Some((reference, input)) = self.g.pick(&choices) {
            let register = self.b.load_input(reference);
            self.define(register, Ty::from_value_type(input.value_type, input.max_len));
        }
    }

    fn account_field(&mut self) {
        let accounts = self.accounts();
        let Some((reference, _)) = self.g.pick(&accounts) else { return };
        let (opcode, ty) = [
            (OP_ACCOUNT_KEY, Ty::Pubkey),
            (OP_ACCOUNT_OWNER, Ty::Pubkey),
            (OP_ACCOUNT_LAMPORTS, Ty::U64),
            (OP_ACCOUNT_DATA_LEN, Ty::U64),
            (OP_ACCOUNT_IS_EMPTY, Ty::Bool),
        ][self.g.below(5)];
        let register = self.b.op(opcode, reference, NO_INDEX, NO_INDEX, 0);
        self.define(register, ty);
    }

    /// Accounts whose data a read may name: never a registry entry.
    fn readable(&self) -> Vec<(u8, Slot)> {
        self.accounts().into_iter().filter(|(_, slot)| !Self::is_entry(slot)).collect()
    }

    fn read_fixed(&mut self) {
        let candidates: Vec<(u8, Slot)> = self.readable().into_iter().filter(|(_, slot)| slot.min_len > 0).collect();
        let Some((reference, slot)) = self.g.pick(&candidates) else { return };
        let fitting: Vec<u8> = READS.iter().copied().filter(|read| read_width(*read) <= slot.min_len as usize).collect();
        let Some(read) = self.g.pick(&fitting) else { return };
        let offset = self.g.below(slot.min_len as usize - read_width(read) + 1);
        let register = self.b.read(read, reference, offset as u64);
        self.define(register, Ty::from_read(read));
    }

    fn read_dynamic(&mut self) {
        let candidates = self.readable();
        let Some((reference, slot)) = self.g.pick(&candidates) else { return };
        let read = READS[self.g.below(READS.len())];
        let width = read_width(read);
        let offset = if self.plan.friendly && slot.min_len as usize >= width {
            self.small_u64(slot.min_len as usize - width + 1)
        } else if self.g.chance(3, 4) {
            self.small_u64(slot.min_len as usize + 40)
        } else {
            self.need(Ty::U64)
        };
        let register = self.b.read_dynamic(read, reference, offset);
        self.define(register, Ty::from_read(read));
    }

    fn read_account_bytes(&mut self) {
        // A fixed account declared writable is refused by the verifier; a row account is checked
        // at run time.
        let candidates: Vec<(u8, Slot)> = self
            .readable()
            .into_iter()
            .filter(|(reference, slot)| reference & ITERATION_ACCOUNT_BIT != 0 || slot.flags & ACCOUNT_WRITABLE == 0)
            .collect();
        let Some((reference, slot)) = self.g.pick(&candidates) else { return };
        let len = self.g.range(1, 32) as u16;
        let offset = if self.plan.friendly && slot.min_len >= len as u32 {
            self.small_u64((slot.min_len - len as u32) as usize + 1)
        } else if self.g.chance(3, 4) {
            self.small_u64(slot.min_len as usize + 24)
        } else {
            self.need(Ty::U64)
        };
        let register = self.b.read_account_bytes(reference, offset, len);
        self.define(register, Ty::Bytes(len));
    }

    fn arithmetic(&mut self) {
        let numeric = self.of(Ty::numeric);
        let Some(left) = self.g.pick(&numeric) else {
            self.constant(Ty::U64);
            return;
        };
        let ty = self.ty(left);
        let right = self.need(ty);
        let opcode = [OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MIN, OP_MAX, OP_REM][self.g.below(7)];
        let register = self.b.binary(opcode, left, right);
        self.define(register, ty);
    }

    fn compare(&mut self) {
        if self.g.chance(1, 2) {
            let numeric = self.of(Ty::numeric);
            let Some(left) = self.g.pick(&numeric) else { return };
            let ty = self.ty(left);
            let right = self.need(ty);
            let opcode = [OP_EQ, OP_NE, OP_LT, OP_LTE, OP_GT, OP_GTE][self.g.below(6)];
            let register = self.b.binary(opcode, left, right);
            self.define(register, Ty::Bool);
        } else {
            // EQ and NE take any type, bytes of different lengths included.
            let all = self.of(|_| true);
            let Some(left) = self.g.pick(&all) else { return };
            let ty = self.ty(left);
            let same = self.of(|candidate| match (candidate, ty) {
                (Ty::Bytes(_), Ty::Bytes(_)) => true,
                _ => candidate == ty,
            });
            let right = self.g.pick(&same).unwrap_or(left);
            let opcode = if self.g.chance(1, 2) { OP_EQ } else { OP_NE };
            let register = self.b.binary(opcode, left, right);
            self.define(register, Ty::Bool);
        }
    }

    fn logic(&mut self) {
        let left = self.need(Ty::Bool);
        let register = match self.g.below(3) {
            0 => self.b.not(left),
            1 => {
                let right = self.need(Ty::Bool);
                self.b.binary(OP_AND, left, right)
            }
            _ => {
                let right = self.need(Ty::Bool);
                self.b.binary(OP_OR, left, right)
            }
        };
        self.define(register, Ty::Bool);
    }

    fn select(&mut self) {
        let condition = self.need(Ty::Bool);
        let all = self.of(|_| true);
        let Some(if_true) = self.g.pick(&all) else { return };
        let ty = self.ty(if_true);
        let same = self.of(|candidate| match (candidate, ty) {
            (Ty::Bytes(_), Ty::Bytes(_)) => true,
            _ => candidate == ty,
        });
        let if_false = self.g.pick(&same).unwrap_or(if_true);
        let result = match (ty, self.ty(if_false)) {
            (Ty::Bytes(a), Ty::Bytes(b)) => Ty::Bytes(a.max(b)),
            _ => ty,
        };
        let register = self.b.select(condition, if_true, if_false);
        self.define(register, result);
    }

    fn cast(&mut self) {
        let numeric = self.of(Ty::numeric);
        let Some(value) = self.g.pick(&numeric) else { return };
        let (opcode, ty) = [(OP_CAST_U64, Ty::U64), (OP_CAST_I64, Ty::I64), (OP_CAST_U128, Ty::U128)][self.g.below(3)];
        let register = self.b.cast(opcode, value);
        self.define(register, ty);
    }

    fn math(&mut self) {
        match self.g.below(4) {
            0 => {
                // Sometimes multiply two near-maximal u128 values, so the 256-bit product the
                // program holds exceeds u128 and the exact-division path (`math::divide_wide`)
                // runs. Otherwise pick existing operands of a shared type.
                let (a, b, c, ty) = if self.g.chance(1, 2) {
                    let big_a = self.b.const_u128(u128::MAX - self.g.interesting_u128() % 7);
                    self.define(big_a, Ty::U128);
                    let big_b = self.b.const_u128(u128::MAX / (1 + self.g.below(3) as u128));
                    self.define(big_b, Ty::U128);
                    let c = self.need(Ty::U128);
                    (big_a, big_b, c, Ty::U128)
                } else {
                    let unsigned = self.of(Ty::unsigned);
                    let Some(a) = self.g.pick(&unsigned) else { return };
                    let ty = self.ty(a);
                    (a, self.need(ty), self.need(ty), ty)
                };
                let register = if self.g.chance(1, 2) { self.b.mul_div(a, b, c) } else { self.b.mul_div_ceil(a, b, c) };
                self.define(register, ty);
            }
            1 => {
                let exponent = if self.g.chance(3, 4) { self.small_u64(45) } else { self.need(Ty::U64) };
                let register = self.b.pow10(exponent);
                self.define(register, Ty::U128);
            }
            2 => {
                let unsigned = self.of(Ty::unsigned);
                let Some(value) = self.g.pick(&unsigned) else { return };
                let ty = self.ty(value);
                let bits = if self.g.chance(3, 4) { self.small_u64(140) } else { self.need(Ty::U64) };
                let opcode = if self.g.chance(1, 2) { OP_SHL } else { OP_SHR };
                let register = self.b.binary(opcode, value, bits);
                self.define(register, ty);
            }
            _ => {
                let unsigned = self.of(Ty::unsigned);
                let Some(left) = self.g.pick(&unsigned) else { return };
                let ty = self.ty(left);
                let right = self.need(ty);
                let opcode = [OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR][self.g.below(3)];
                let register = self.b.binary(opcode, left, right);
                self.define(register, ty);
            }
        }
    }

    /// One data part: a literal, or a register with an encoding its type accepts. Returns the
    /// part and the most bytes it encodes.
    fn part(&mut self, max_bytes_len: u16) -> (Segment, usize) {
        if self.g.chance(1, 4) {
            let len = self.g.range(0, 12);
            let bytes = self.g.bytes(len);
            return (Segment::Literal(self.b.blob(&bytes)), len);
        }
        let candidates = self.of(|ty| !matches!(ty, Ty::Bytes(len) if len > max_bytes_len));
        let register = match self.g.pick(&candidates) {
            Some(register) => register,
            None => self.constant(Ty::U64),
        };
        let (kind, width) = self.ty(register).encoding(self.g);
        (Segment::Register(kind, register), width)
    }

    fn pda(&mut self) {
        let programs: Vec<(u8, Slot)> = self
            .accounts()
            .into_iter()
            .filter(|(_, slot)| slot.flags & ACCOUNT_EXECUTABLE != 0)
            .collect();
        let Some((program, _)) = self.g.pick(&programs) else { return };
        let mut seeds = Vec::new();
        for _ in 0..self.g.range(1, 4) {
            loop {
                let (segment, width) = self.part(32);
                if width <= MAX_PDA_SEED_LEN {
                    seeds.push(segment);
                    break;
                }
            }
        }
        if !self.room(2) {
            return;
        }
        let register = if !self.plan.friendly && self.g.chance(1, 2) {
            // A bump in range exercises `create_program_address`; occasionally an out-of-range
            // bump exercises the rejection path instead.
            let bump = if self.g.chance(9, 10) { self.small_u64(256) } else { self.need(Ty::U64) };
            self.b.create_pda(program, bump, &seeds)
        } else {
            self.b.derive_pda(program, &seeds)
        };
        self.define(register, Ty::Pubkey);
    }

    fn introspect(&mut self) {
        let Some(sysvar) = self.slot_of_role(&Role::Sysvar) else { return };
        let sysvar = sysvar as u8;
        let index = if self.g.chance(4, 5) { self.small_u64(5) } else { self.need(Ty::U64) };
        match self.g.below(8) {
            0 => {
                let register = self.b.introspect(OP_INSTRUCTION_COUNT, sysvar, NO_INDEX, NO_INDEX);
                self.define(register, Ty::U64);
            }
            1 => {
                let register = self.b.introspect(OP_INSTRUCTION_INDEX, sysvar, NO_INDEX, NO_INDEX);
                self.define(register, Ty::U64);
            }
            2 => {
                let register = self.b.introspect(OP_INSTRUCTION_PROGRAM, sysvar, index, NO_INDEX);
                self.define(register, Ty::Pubkey);
            }
            3 => {
                let opcode = if self.g.chance(1, 2) { OP_INSTRUCTION_ACCOUNT_COUNT } else { OP_INSTRUCTION_DATA_LEN };
                let register = self.b.introspect(opcode, sysvar, index, NO_INDEX);
                self.define(register, Ty::U64);
            }
            4 | 5 => {
                let position = self.small_u64(6);
                if self.g.chance(1, 2) {
                    let register = self.b.introspect(OP_INSTRUCTION_ACCOUNT, sysvar, index, position);
                    self.define(register, Ty::Pubkey);
                } else {
                    let register = self.b.introspect(OP_INSTRUCTION_ACCOUNT_FLAGS, sysvar, index, position);
                    self.define(register, Ty::U64);
                }
            }
            6 => {
                let offset = self.small_u64(24);
                let read = READS[self.g.below(READS.len())];
                let register = self.b.read_instruction_data(read, sysvar, index, offset);
                self.define(register, Ty::from_read(read));
            }
            _ => {
                let offset = self.small_u64(24);
                let len = self.g.range(1, 16) as u16;
                let register = self.b.read_instruction_bytes(sysvar, index, offset, len);
                self.define(register, Ty::Bytes(len));
            }
        }
    }

    fn mov(&mut self) {
        let all = self.of(|_| true);
        let Some(source) = self.g.pick(&all) else { return };
        let ty = self.ty(source);
        if self.g.chance(1, 2) {
            // Overwrite an existing register, possibly with a new type. Inside a loop body this
            // writes a register the loop must restore afterwards, unless the loop carries it, in
            // which case the type has to stay.
            let targets: Vec<u8> = all
                .iter()
                .copied()
                .filter(|target| !self.carried.contains(target) || self.ty(*target) == ty)
                .collect();
            if let Some(target) = self.g.pick(&targets) {
                self.b.mov(target, source);
                self.define(target, ty);
                return;
            }
        }
        let target = self.b.register();
        self.b.mov(target, source);
        self.define(target, ty);
    }

    // ---- account groups ------------------------------------------------------------------------

    /// An expression over a declared account group: its length, or a filter (`GROUP_ANY`,
    /// `GROUP_COUNT`) whose programs, data floor, matches and excepts the plan records, so the
    /// account generator can supply members that pass every test and members that fail one. The
    /// result is usually logged at once, so the model's answer is compared byte for byte, and an
    /// `any` is sometimes required, so a filter that finds nothing fails the run where the model
    /// says it does.
    fn group(&mut self) {
        // A filter takes up to eight value registers and ten instructions with its log line.
        if self.plan.groups == 0 || !self.room(12) || self.b.register_count() + 10 > self.register_limit {
            return;
        }
        let group = self.g.below(self.plan.groups);
        if self.g.chance(1, 5) {
            let register = self.b.group_length(group as u8);
            self.define(register, Ty::U64);
            self.observe_group(register, false);
            return;
        }
        let world = self.world;
        let owners = [world.probe, SYSTEM_PROGRAM_ADDRESS, world.token, world.ballista];
        // Ballista owns the entries a run opens: a filter for its accounts may meet one an open holds.
        let weights = [4, 3, 2, if self.plan.opens.is_empty() { 1 } else { 3 }];
        let mut programs = vec![owners[self.g.weighted(&weights)]];
        if self.g.chance(1, 3) {
            programs.push(owners[self.g.weighted(&weights)]);
        }
        let mut matches = Vec::new();
        let mut planned_matches = Vec::new();
        let mut floor = 0;
        for _ in 0..=self.g.weighted(&[5, 3, 1, 1]) {
            let (ty, kind, width) = [
                (Ty::Bool, DATA_REG_BOOL, 1),
                (Ty::U64, DATA_REG_U64, 8),
                (Ty::I64, DATA_REG_I64, 8),
                (Ty::U128, DATA_REG_U128, 16),
                (Ty::Pubkey, DATA_REG_PUBKEY, 32),
            ][self.g.weighted(&[1, 4, 1, 1, 3])];
            // Mostly on a word boundary, as account layouts put their fields; sometimes anywhere.
            let offset = if self.g.chance(3, 4) { 8 * self.g.below(9) } else { self.g.below(80) };
            let (register, value) = self.filter_value(ty);
            floor = floor.max(offset + width);
            matches.push((offset as u16, kind, register));
            planned_matches.push((offset, value));
        }
        let mut excepts = Vec::new();
        let mut planned_excepts = Vec::new();
        for _ in 0..self.g.weighted(&[5, 3, 1, 1, 1]) {
            let (register, value) = self.filter_key();
            excepts.push(register);
            planned_excepts.push(value);
        }
        let min_data_len = match self.g.weighted(&[6, 3, 1]) {
            0 => floor,
            1 => floor + self.g.range(1, 40),
            // A floor no account the generator makes reaches.
            _ => floor + self.g.range(300, 1000),
        } as u32;
        let count = self.g.chance(1, 2);
        let register = self.b.group_filter(count, group as u8, &programs, &matches, &excepts, min_data_len);
        self.define(register, if count { Ty::U64 } else { Ty::Bool });
        self.plan.group_filters.push(GroupFilter {
            group,
            programs,
            min_data_len,
            matches: planned_matches,
            excepts: planned_excepts,
        });
        self.observe_group(register, !count);
    }

    /// A fresh register holding a match value of type `ty`, and where its value comes from: a
    /// constant, a fixed input of that type, a fixed account's key; or an existing register of
    /// that type, whose value only the run knows.
    fn filter_value(&mut self, ty: Ty) -> (u8, FilterValue) {
        let inputs: Vec<usize> = (0..self.plan.inputs.len())
            .filter(|&index| Ty::from_value_type(self.plan.inputs[index].value_type, 0) == ty)
            .collect();
        let existing = self.of(|candidate| candidate == ty);
        let key_weight = if ty == Ty::Pubkey && !self.plan.fixed.is_empty() { 3 } else { 0 };
        match self.g.weighted(&[5, 2, key_weight, 2]) {
            1 if !inputs.is_empty() => {
                let input = inputs[self.g.below(inputs.len())];
                let register = self.b.load_input(input as u8);
                (self.define(register, ty), FilterValue::Input(input))
            }
            2 => {
                let slot = self.g.below(self.plan.fixed.len());
                let register = self.b.account_key(slot as u8);
                (self.define(register, ty), FilterValue::SlotKey(slot))
            }
            3 if !existing.is_empty() => (existing[self.g.below(existing.len())], FilterValue::Unknown),
            _ => {
                // Zero often: friendly account data is mostly zeros, so a zero matches by chance.
                let zero = self.g.chance(1, 3);
                let friendly = self.plan.friendly;
                let (register, bytes) = match ty {
                    Ty::Bool => {
                        let value = !zero && self.g.chance(1, 2);
                        (self.b.const_bool(value), vec![u8::from(value)])
                    }
                    Ty::U64 => {
                        let value = if zero { 0 } else if friendly { self.g.below(16) as u64 } else { self.g.interesting_u64() };
                        (self.b.const_u64(value), value.to_le_bytes().to_vec())
                    }
                    Ty::I64 => {
                        let value = if zero { 0 } else if friendly { self.g.below(16) as i64 } else { self.g.interesting_i64() };
                        (self.b.const_i64(value), value.to_le_bytes().to_vec())
                    }
                    Ty::U128 => {
                        let value = if zero { 0 } else if friendly { self.g.below(16) as u128 } else { self.g.interesting_u128() };
                        (self.b.const_u128(value), value.to_le_bytes().to_vec())
                    }
                    _ => {
                        let value = if zero { [0; 32] } else { self.interesting_pubkey() };
                        (self.b.const_pubkey(value), value.to_vec())
                    }
                };
                (self.define(register, ty), FilterValue::Bytes(bytes))
            }
        }
    }

    /// A `pubkey` register naming an except key, and where it comes from: a fresh key the account
    /// generator may give a member that passes every other test, an account the world names, a
    /// fixed account's key; or an existing `pubkey` register, whose value only the run knows.
    fn filter_key(&mut self) -> (u8, FilterValue) {
        let existing = self.of(|ty| ty == Ty::Pubkey);
        let slot_weight = if self.plan.fixed.is_empty() { 0 } else { 3 };
        let key = match self.g.weighted(&[4, 2, slot_weight, 1]) {
            1 => self.interesting_pubkey(),
            2 => {
                let slot = self.g.below(self.plan.fixed.len());
                let register = self.b.account_key(slot as u8);
                return (self.define(register, Ty::Pubkey), FilterValue::SlotKey(slot));
            }
            3 if !existing.is_empty() => return (existing[self.g.below(existing.len())], FilterValue::Unknown),
            _ => {
                // Tagged apart from the world's accounts and the account generator's fresh ones.
                let mut key = [0u8; 32];
                key.copy_from_slice(&self.g.bytes(32));
                key[0] = 0xd0 | (key[0] & 0x0f);
                key[31] = 0xc3;
                key
            }
        };
        let register = self.b.const_pubkey(key);
        (self.define(register, Ty::Pubkey), FilterValue::Bytes(key.to_vec()))
    }

    /// Makes a group expression's result observable: usually logged at once, and an `any`
    /// sometimes required instead.
    fn observe_group(&mut self, register: u8, any: bool) {
        if any && self.g.chance(1, 6) {
            self.mark();
            self.b.require(register);
        } else if self.g.chance(3, 4) {
            self.emit_registers(&[register]);
        }
    }

    fn read_registry(&mut self) {
        let Some(open) = self.g.pick(&self.opened.clone()) else { return };
        let Open { entry, index, size, .. } = self.plan.opens[open].clone();
        let layout = self.layouts[index as usize].clone().expect("declared");
        let (offset, selector) = if self.g.chance(2, 3) {
            let (_, selector, offset) = layout[self.g.below(layout.len())];
            (offset, selector)
        } else {
            let selector = READS[self.g.below(READS.len())];
            let width = read_width(selector) as u16;
            if width > size {
                return;
            }
            (self.g.below((size - width + 1) as usize) as u16, selector)
        };
        let register = self.b.read_registry(entry as u8, offset, selector);
        self.define(register, Ty::from_read(selector));
    }

    fn write_registry(&mut self) {
        let Some(open) = self.g.pick(&self.opened.clone()) else {
            self.value();
            return;
        };
        let Open { entry, index, size, .. } = self.plan.opens[open].clone();
        let layout = self.layouts[index as usize].clone().expect("declared");
        let (ty, selector, offset) = if self.g.chance(3, 4) {
            layout[self.g.below(layout.len())]
        } else {
            // Any field-sized write inside the entry, overlapping declared fields or not.
            let (ty, selector, width) = FIELD_TYPES[self.g.below(FIELD_TYPES.len())];
            if width > size {
                return;
            }
            (ty, selector, self.g.below((size - width + 1) as usize) as u16)
        };
        let value = self.need(ty);
        self.mark();
        self.b.write_registry(entry as u8, offset, selector, value);
        self.plan.registry_writes.push((open, offset, read_width(selector)));
    }

    fn emit_open(&mut self, open: usize) {
        let Open { entry, payer, system, index, size, key, .. } = self.plan.opens[open].clone();
        let key_register = match key {
            KeySource::Zero => None,
            KeySource::SlotKey(slot) => {
                let register = self.b.account_key(slot as u8);
                Some(self.define(register, Ty::Pubkey))
            }
            KeySource::Input(input) => {
                let register = self.b.load_input(input as u8);
                Some(self.define(register, Ty::Pubkey))
            }
            KeySource::Const(key) => {
                let register = self.b.const_pubkey(key);
                Some(self.define(register, Ty::Pubkey))
            }
        };
        let pc = self.b.open_registry(entry as u8, key_register, payer as u8, index, size, system as u8);
        self.plan.opens[open].pc = pc;
        self.opened.push(open);
        self.mark();
    }

    // ---- invocations --------------------------------------------------------------------------

    fn program_slot(&self, program: Program) -> Option<u8> {
        self.slot_of_role(&Role::Program(program)).map(|slot| slot as u8)
    }

    /// Account records for a call: up to `count` accounts in scope, each with flags its
    /// declaration allows, and an entry only read-only.
    fn call_accounts(&mut self, count: usize) -> Vec<(u8, u8)> {
        let accounts = self.accounts();
        let mut records = Vec::new();
        for _ in 0..count {
            let Some((reference, slot)) = self.g.pick(&accounts) else { break };
            let mut flags = 0;
            if slot.flags & ACCOUNT_SIGNER != 0 && self.g.chance(2, 3) {
                flags |= ACCOUNT_SIGNER;
            }
            if slot.flags & ACCOUNT_WRITABLE != 0 && !Self::is_entry(&slot) && self.g.chance(2, 3) {
                flags |= ACCOUNT_WRITABLE;
            }
            records.push((reference, flags));
        }
        records
    }

    fn invoke(&mut self) {
        if self.cpi_budget < self.passes || !self.room(6) {
            self.value();
            return;
        }
        let mut targets = Vec::new();
        for (program, weight) in [(Program::Probe, 6), (Program::System, 3), (Program::Token, 1), (Program::Ballista, 2)] {
            if let Some(slot) = self.program_slot(program) {
                targets.push((program, slot, weight));
            }
        }
        // In a batch loop, a row may name the program to call.
        if self.scope == Scope::Rows {
            for (offset, slot) in self.plan.row.iter().enumerate() {
                if slot.role == Role::Program(Program::Probe) {
                    targets.push((Program::Probe, ITERATION_ACCOUNT_BIT | offset as u8, 8));
                }
            }
        }
        if targets.is_empty() {
            self.value();
            return;
        }
        let weights: Vec<u32> = targets.iter().map(|(_, _, weight)| *weight).collect();
        let (program, program_slot, _) = targets[self.g.weighted(&weights)];
        let group = (self.plan.groups > 0 && self.g.chance(1, 2)).then(|| self.g.below(self.plan.groups) as u8);

        let mut segments = Vec::new();
        let mut max_len = 0usize;
        let mut returns: Option<usize> = None;
        let accounts = match program {
            Program::Probe | Program::ProbeCopy => {
                let op = [
                    probe::NOOP,
                    probe::SET_RETURN,
                    probe::WRITE_FIRST,
                    probe::RESIZE_FIRST,
                    probe::FAIL,
                    probe::INVOKE,
                    probe::TRANSFER,
                ][self.g.weighted(&[4, 6, 2, 1, if self.plan.friendly { 0 } else { 1 }, 1, 2])];
                let mut prefix = vec![op];
                if matches!(op, probe::RESIZE_FIRST | probe::FAIL | probe::TRANSFER | probe::INVOKE) {
                    prefix.push(self.g.u8());
                }
                max_len += prefix.len();
                segments.push(Segment::Literal(self.b.blob(&prefix)));
                for _ in 0..self.g.range(0, 3) {
                    let (segment, width) = self.part(64);
                    segments.push(segment);
                    max_len += width;
                }
                if op == probe::SET_RETURN {
                    returns = Some(max_len - 1);
                }
                // At the limits, a quarter of the probe calls list 16 to 64 accounts.
                let count = if self.limits && self.g.chance(1, 4) {
                    self.g.range(16, MAX_CPI_ACCOUNTS)
                } else {
                    self.g.range(0, 4)
                };
                self.call_accounts(count)
            }
            Program::System => {
                let accounts = self.accounts();
                let from: Vec<u8> = accounts
                    .iter()
                    .filter(|(_, slot)| slot.flags & (ACCOUNT_SIGNER | ACCOUNT_WRITABLE) == ACCOUNT_SIGNER | ACCOUNT_WRITABLE)
                    .map(|(reference, _)| *reference)
                    .collect();
                let to: Vec<u8> = accounts
                    .iter()
                    .filter(|(_, slot)| slot.flags & ACCOUNT_WRITABLE != 0 && !Self::is_entry(slot))
                    .map(|(reference, _)| *reference)
                    .collect();
                match (self.g.pick(&from), self.g.pick(&to)) {
                    (Some(from), Some(to)) => {
                        let amount = if self.g.chance(3, 4) {
                            let value = self.g.below(5_000) as u64;
                            let register = self.b.const_u64(value);
                            self.define(register, Ty::U64)
                        } else {
                            let unsigned = self.of(Ty::unsigned);
                            match self.g.pick(&unsigned) {
                                Some(register) => register,
                                None => self.constant(Ty::U64),
                            }
                        };
                        segments.push(Segment::Literal(self.b.blob(&[2, 0, 0, 0])));
                        segments.push(Segment::Register(DATA_REG_U64, amount));
                        max_len = 12;
                        vec![(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (to, ACCOUNT_WRITABLE)]
                    }
                    _ if self.plan.friendly => {
                        self.value();
                        return;
                    }
                    _ => {
                        // No transfer fits: send the System program junk, which it refuses.
                        let len = self.g.range(1, 8);
                        let bytes = self.g.bytes(len);
                        segments.push(Segment::Literal(self.b.blob(&bytes)));
                        max_len = len;
                        let count = self.g.range(0, 2);
                        self.call_accounts(count)
                    }
                }
            }
            Program::Token => {
                if self.g.chance(2, 3) {
                    // GetAccountDataSize: returns a u64 for a mint.
                    segments.push(Segment::Literal(self.b.blob(&[21])));
                    max_len = 1;
                    returns = Some(8);
                } else {
                    let len = self.g.range(1, 10);
                    let bytes = self.g.bytes(len);
                    segments.push(Segment::Literal(self.b.blob(&bytes)));
                    max_len = len;
                }
                let count = self.g.range(1, 2);
                self.call_accounts(count)
            }
            Program::Ballista => {
                // A nested run: the first account names the template, a group (if any) supplies
                // its runtime accounts, and a bytes value from the caller its run data.
                segments.push(Segment::Literal(self.b.blob(&[ballista_common::instruction::IX_RUN])));
                max_len = 1;
                let bytes = self.of(|ty| matches!(ty, Ty::Bytes(len) if len <= 64));
                if let Some(register) = self.g.pick(&bytes) {
                    let Ty::Bytes(len) = self.ty(register) else { unreachable!() };
                    segments.push(Segment::Register(DATA_REG_BYTES, register));
                    max_len += len as usize;
                }
                returns = Some(MAX_RETURN_DATA_LEN);
                let nested = self.slot_of_role(&Role::NestedTemplate).expect("declared with the Ballista slot");
                let mut accounts = vec![(nested as u8, 0)];
                let count = self.g.range(0, 2);
                accounts.extend(self.call_accounts(count));
                accounts
            }
        };
        if !self.room(2) {
            return;
        }
        let cpi = self.b.cpi_with_group(program_slot, &accounts, &segments, group.unwrap_or(NO_INDEX));
        self.b.set_cpi_max_data_len(cpi, max_len as u16);
        let guard = self.g.chance(1, 4).then(|| self.need(Ty::Bool));
        self.b.invoke(cpi, guard);
        self.cpi_budget -= self.passes;
        if !self.plan.calls.contains(&program) {
            self.plan.calls.push(program);
        }
        self.after_invoke = None;
        if guard.is_none() {
            self.after_invoke = returns.map(|max_len| ReturnShape { max_len });
            if self.after_invoke.is_some() && self.g.chance(1, 2) {
                self.return_data();
            }
        }
    }

    fn return_data(&mut self) {
        let Some(shape) = self.after_invoke else { return };
        let selector = READS[self.g.below(READS.len())];
        let width = read_width(selector);
        let limit = shape.max_len.min(MAX_RETURN_DATA_LEN);
        // Mostly in range of what the call can return, sometimes just past it.
        let offset = if limit >= width && self.g.chance(4, 5) {
            self.g.below(limit - width + 1)
        } else {
            self.g.below(8).min(MAX_RETURN_DATA_LEN - width)
        };
        let register = self.b.return_data(selector, offset as u64);
        self.define(register, Ty::from_read(selector));
        self.after_invoke = None;
    }

    fn emit_output(&mut self, emit: bool) {
        if !self.room(5) {
            return;
        }
        let mut parts = Vec::new();
        if emit {
            let len = self.g.range(MIN_EMIT_TAG_LEN, 8);
            let mut tag = self.g.bytes(len);
            if tag.starts_with(&RUN_EVENT_TAG_FAMILY) {
                tag[0] ^= 0x80;
            }
            parts.push(Segment::Literal(self.b.blob(&tag)));
            self.plan.emit_tags.push(tag);
        }
        let mut total = 0;
        for _ in 0..self.g.range(usize::from(!emit), 3) {
            let (segment, width) = self.part(64);
            parts.push(segment);
            total += width;
        }
        debug_assert!(total <= MAX_RETURN_DATA_LEN);
        self.mark();
        if emit {
            self.b.emit_data(&parts);
        } else {
            self.b.set_return_data(&parts);
            self.plan.sets_return_data = true;
        }
    }

    // ---- loops ---------------------------------------------------------------------------------

    fn emit_loop(&mut self, foreach: bool) {
        let (passes, count) = if foreach {
            (self.plan.batch_max, None)
        } else {
            // At the limits a third of the count loops may run up to 64 passes.
            let max = if self.limits && self.g.chance(1, 3) { self.g.range(1, 64) } else { self.g.range(1, 6) };
            let count = if self.g.chance(4, 5) {
                let value = if self.plan.friendly { self.g.below(max + 1) } else { self.g.below(max + 2) } as u64;
                let register = self.b.const_u64(value);
                self.define(register, Ty::U64)
            } else {
                self.need(Ty::U64)
            };
            (max, Some((count, max as u8)))
        };
        // Carry up to two numeric registers set before the loop.
        let numeric = self.of(Ty::numeric);
        let mut carried = Vec::new();
        for _ in 0..self.g.below(3) {
            if let Some(register) = self.g.pick(&numeric) {
                if !carried.contains(&register) {
                    carried.push(register);
                }
            }
        }
        let carry_mask = carried.iter().fold(0u64, |mask, register| mask | 1 << register);
        let header = match count {
            None => record(OP_FOREACH, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, carry_mask),
            Some((count, max)) => record(OP_REPEAT, NO_INDEX, 0, count, max, 0, carry_mask),
        };
        self.mark();
        let start = self.b.emit(header);

        let saved_regs = self.regs.clone();
        let saved_scope = self.scope;
        let saved_passes = self.passes;
        self.scope = if foreach { Scope::Rows } else { Scope::Count };
        self.passes = passes;
        self.carried = carried.clone();
        self.after_invoke = None;

        // Restore observers. A pass must start from the registers the loop started with, apart
        // from the carried ones, and the code after the loop must see them too (`next_pass`).
        // Half the bodies first EMIT a few registers set before the loop and later overwrite some
        // of them, so a pass that does not restore shows a different line in the next pass; and
        // half the loops EMIT what the body overwrote, and what it carried, after the loop. The
        // model predicts every EMIT, so a wrong restore or carry is a mismatch.
        let before_loop: Vec<u8> = (0..self.b.register_count()).filter(|&r| saved_regs[r as usize].is_some()).collect();
        let observed: Vec<u8> = if !before_loop.is_empty() && self.g.chance(1, 2) {
            let mut observed = Vec::new();
            for _ in 0..self.g.range(1, 3) {
                if let Some(register) = self.g.pick(&before_loop) {
                    if !observed.contains(&register) {
                        observed.push(register);
                    }
                }
            }
            self.emit_registers(&observed);
            observed
        } else {
            Vec::new()
        };

        let statements = if self.limits { self.g.range(1, 8) } else { self.g.range(1, 5) };
        for _ in 0..statements {
            if !self.room(4) { break; }
            self.statement();
        }
        // Overwrite an observed register that the loop does not carry, with any value: the
        // restore puts its pre-loop value back, type included.
        for &register in &observed {
            if carried.contains(&register) || !self.room(3) || !self.g.chance(2, 3) {
                continue;
            }
            let all = self.of(|_| true);
            if let Some(source) = self.g.pick(&all) {
                let ty = self.ty(source);
                self.b.mov(register, source);
                self.define(register, ty);
            }
        }
        for register in &carried {
            if !self.room(2) {
                break;
            }
            let ty = self.ty(*register);
            // Mostly a small step, so the sum seldom overflows and the run gets past the loop.
            let other = if ty == Ty::U64 && self.g.chance(3, 4) { self.small_u64(10) } else { self.need(ty) };
            let sum = self.b.binary(OP_ADD, *register, other);
            self.define(sum, ty);
            self.b.mov(*register, sum);
        }
        // Every body ends with something, so it is never empty.
        let always = self.b.const_bool(true);
        self.define(always, Ty::Bool);
        self.b.require(always);

        let body = self.b.instructions_mut().len() - start - 1;
        self.b.instructions_mut()[start].a = body as u8;
        // Registers set before the loop that its body writes: the ones the restore protects.
        let written: Vec<u8> = self.b.instructions_mut()[start + 1..]
            .iter()
            .map(|record| record.dst)
            .filter(|&dst| dst != NO_INDEX && (dst as usize) < MAX_REGISTERS && saved_regs[dst as usize].is_some())
            .collect();
        self.loops += 1;
        self.regs = saved_regs;
        self.scope = saved_scope;
        self.passes = saved_passes;
        self.carried.clear();
        self.after_invoke = None;

        if self.g.chance(1, 2) {
            let mut after: Vec<u8> = Vec::new();
            for register in written.into_iter().chain(carried) {
                if !after.contains(&register) && after.len() < 4 {
                    after.push(register);
                }
            }
            self.emit_registers(&after);
        }
    }

    /// An `EMIT` of a fresh tag and `registers`, each at its type's full width, so the line holds
    /// their values exactly; a `bytes` register longer than 64 is left out.
    fn emit_registers(&mut self, registers: &[u8]) {
        if registers.is_empty() || !self.room(3) {
            return;
        }
        let len = self.g.range(MIN_EMIT_TAG_LEN, 8);
        let mut tag = self.g.bytes(len);
        if tag.starts_with(&RUN_EVENT_TAG_FAMILY) {
            tag[0] ^= 0x80;
        }
        let mut parts = vec![Segment::Literal(self.b.blob(&tag))];
        for &register in registers {
            let kind = match self.ty(register) {
                Ty::Bool => DATA_REG_BOOL,
                Ty::U64 => DATA_REG_U64,
                Ty::I64 => DATA_REG_I64,
                Ty::U128 => DATA_REG_U128,
                Ty::Pubkey => DATA_REG_PUBKEY,
                Ty::Bytes(len) if len <= 64 => DATA_REG_BYTES,
                Ty::Bytes(_) => continue,
            };
            parts.push(Segment::Register(kind, register));
        }
        self.plan.emit_tags.push(tag);
        self.mark();
        self.b.emit_data(&parts);
    }
}
