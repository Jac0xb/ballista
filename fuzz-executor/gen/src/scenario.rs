//! Random runs of a generated template: the accounts the caller passes, with duplicates, flags and
//! owners chosen to mostly satisfy the template's declarations and sometimes not; the run data;
//! and the transaction's other instructions, which the Instructions sysvar lists.

use ballista_common::instruction::IX_RUN;
use ballista_common::template::*;

use crate::source::Gen;
use crate::template::{probe, KeySource, Program, Role, Slot, TemplatePlan, World, ALL_PROGRAMS};

/// Derives a PDA: `(seeds, program id) -> (address, bump)`. The harness supplies it.
pub type Pda<'a> = &'a dyn Fn(&[&[u8]], &[u8; 32]) -> ([u8; 32], u8);
/// The rent-exempt minimum for a data length.
pub type Rent<'a> = &'a dyn Fn(usize) -> u64;

/// How an entry account was set up before the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    /// Nothing at the address.
    Absent,
    /// System-owned and empty, with lamports someone sent.
    Prefunded,
    /// The entry this template, registry and key name, as an earlier run left it.
    Existing,
    /// Another template's entry, at its own address.
    OtherTemplate,
    /// This template's entry of another registry, at its own address.
    OtherIndex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Program(Program),
    Sysvar,
    /// The template under test. The harness supplies its account.
    Template,
    /// System-owned.
    User,
    /// Owned by the probe program.
    ProbeData,
    /// Owned by the Token program.
    TokenData,
    /// An entry for `opens[open]`, or a stand-in for one.
    Entry { open: usize, state: EntryState },
}

#[derive(Clone, Debug)]
pub struct AccountSpec {
    pub address: [u8; 32],
    pub owner: [u8; 32],
    pub lamports: u64,
    pub data: Vec<u8>,
    pub executable: bool,
    /// Whether the transaction marks it signer and writable. One flag per account: the runtime
    /// merges the flags of every meta that names it.
    pub signer: bool,
    pub writable: bool,
    pub kind: Kind,
}

/// A probe instruction elsewhere in the transaction.
#[derive(Clone, Debug)]
pub struct Extra {
    pub accounts: Vec<usize>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub pool: Vec<AccountSpec>,
    pub template: usize,
    /// Runtime accounts in order, as pool indexes. The template account comes first in the
    /// instruction and is not listed here.
    pub slots: Vec<usize>,
    /// The whole instruction data, starting with `IX_RUN`.
    pub run_data: Vec<u8>,
    pub iterations: usize,
    pub group_lengths: Vec<u8>,
    pub before: Vec<Extra>,
    pub after: Vec<Extra>,
    /// When set, a top-level probe instruction runs the template through a CPI, demoting the
    /// writable flag of forwarded account `n` when bit `n` of the policy is set.
    pub wrap: Option<u8>,
    /// What the generator broke on purpose, for diagnostics.
    pub mutation: Option<&'static str>,
    /// The entry address each open names, when the generator could derive it.
    pub entry_addresses: Vec<Option<[u8; 32]>>,
}

impl Scenario {
    pub fn address(&self, index: usize) -> [u8; 32] {
        self.pool[index].address
    }
}

/// A decoded input value, encoded as run data expects.
#[derive(Clone, Debug)]
enum Value {
    Bool(u8),
    U64(u64),
    I64(i64),
    U128(u128),
    Pubkey([u8; 32]),
    Bytes(Vec<u8>),
}

impl Value {
    fn encode(&self, output: &mut Vec<u8>) {
        match self {
            Value::Bool(byte) => output.push(*byte),
            Value::U64(value) => output.extend_from_slice(&value.to_le_bytes()),
            Value::I64(value) => output.extend_from_slice(&value.to_le_bytes()),
            Value::U128(value) => output.extend_from_slice(&value.to_le_bytes()),
            Value::Pubkey(value) => output.extend_from_slice(value),
            Value::Bytes(bytes) => {
                output.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                output.extend_from_slice(bytes);
            }
        }
    }
}

/// The header an entry holds, as `registry::EntryId::header` writes it.
pub fn entry_header(template: &[u8; 32], index: u8, key: &[u8; 32]) -> [u8; REGISTRY_ENTRY_HEADER_LEN] {
    let mut header = [0u8; REGISTRY_ENTRY_HEADER_LEN];
    header[..4].copy_from_slice(&REGISTRY_ENTRY_MAGIC);
    header[4] = REGISTRY_ENTRY_VERSION;
    header[5] = index;
    header[8..40].copy_from_slice(template);
    header[40..].copy_from_slice(key);
    header
}

pub fn entry_address(pda: Pda, ballista: &[u8; 32], template: &[u8; 32], index: u8, key: &[u8; 32]) -> [u8; 32] {
    pda(&[REGISTRY_SEED, template, &[index], key], ballista).0
}

struct ScenarioGen<'a, 'g> {
    g: &'a mut Gen<'g>,
    world: &'a World,
    plan: &'a TemplatePlan,
    template_address: [u8; 32],
    pda: Pda<'a>,
    rent: Rent<'a>,
    pool: Vec<AccountSpec>,
    /// Pool indexes assigned to each fixed slot.
    fixed: Vec<usize>,
}

pub fn generate_scenario(
    g: &mut Gen,
    world: &World,
    plan: &TemplatePlan,
    template_address: [u8; 32],
    pda: Pda,
    rent: Rent,
) -> Scenario {
    let mut generator = ScenarioGen {
        g,
        world,
        plan,
        template_address,
        pda,
        rent,
        pool: Vec::new(),
        fixed: Vec::new(),
    };
    generator.run()
}

impl ScenarioGen<'_, '_> {
    fn add(&mut self, spec: AccountSpec) -> usize {
        if let Some(existing) = self.find(&spec.address) {
            return existing;
        }
        self.pool.push(spec);
        self.pool.len() - 1
    }

    fn find(&self, address: &[u8; 32]) -> Option<usize> {
        self.pool.iter().position(|spec| &spec.address == address)
    }

    fn fresh_address(&mut self) -> [u8; 32] {
        let mut address = [0u8; 32];
        for byte in address.iter_mut() {
            *byte = self.g.u8();
        }
        address[0] = 0xf0 | (address[0] & 0x0f);
        address[31] = 0xe7;
        // Retry the rare collision with an existing account.
        while self.find(&address).is_some() {
            address[1] = address[1].wrapping_add(1);
        }
        address
    }

    fn lamports(&mut self) -> u64 {
        match self.g.below(6) {
            0 => 0,
            1 => self.g.below(10_000) as u64,
            2 => 1_000_000 + self.g.below(10_000_000) as u64,
            _ => 10_000_000_000,
        }
    }

    fn base_pool(&mut self) {
        for program in ALL_PROGRAMS {
            self.add(AccountSpec {
                address: self.world.program(program),
                owner: [0; 32],
                lamports: 1,
                data: Vec::new(),
                executable: true,
                signer: false,
                writable: false,
                kind: Kind::Program(program),
            });
        }
        self.add(AccountSpec {
            address: INSTRUCTIONS_SYSVAR_ID,
            owner: [0; 32],
            lamports: 0,
            data: Vec::new(),
            executable: false,
            signer: false,
            writable: false,
            kind: Kind::Sysvar,
        });
        self.add(AccountSpec {
            address: self.template_address,
            owner: self.world.ballista,
            lamports: 0,
            data: Vec::new(),
            executable: false,
            signer: false,
            writable: false,
            kind: Kind::Template,
        });
        for user in self.world.users.clone() {
            let lamports = self.lamports();
            let len = if self.g.chance(1, 5) { self.g.range(1, 100) } else { 0 };
            let data = self.g.bytes(len);
            self.add(AccountSpec {
                address: user,
                owner: SYSTEM_PROGRAM_ADDRESS,
                lamports,
                data,
                executable: false,
                signer: false,
                writable: false,
                kind: Kind::User,
            });
        }
        for account in self.world.probe_data.clone() {
            let len = self.g.range(0, 140);
            let data = self.g.bytes(len);
            let lamports = (self.rent)(len) + self.g.below(1000) as u64;
            self.add(AccountSpec {
                address: account,
                owner: self.world.probe,
                lamports,
                data,
                executable: false,
                signer: false,
                writable: false,
                kind: Kind::ProbeData,
            });
        }
    }

    fn fresh(&mut self, owner: [u8; 32], min_len: usize) -> usize {
        let address = self.fresh_address();
        let len = min_len + if self.g.chance(1, 2) { 0 } else { self.g.below(48) };
        let data = self.g.bytes(len);
        let kind = if owner == self.world.probe {
            Kind::ProbeData
        } else if owner == self.world.token {
            Kind::TokenData
        } else {
            Kind::User
        };
        let lamports = if len == 0 { self.lamports() } else { (self.rent)(len) + self.g.below(5000) as u64 };
        self.add(AccountSpec {
            address,
            owner,
            lamports,
            data,
            executable: false,
            signer: false,
            writable: false,
            kind,
        })
    }

    fn program_index(&self, program: Program) -> usize {
        self.find(&self.world.program(program)).expect("in the base pool")
    }

    fn is_ballista_pda(&self, index: usize) -> bool {
        matches!(self.pool[index].kind, Kind::Template | Kind::Entry { .. })
            || self.pool[index].owner == self.world.ballista
    }

    /// Marks the flags a slot declares on the account, where the account can carry them.
    fn grant(&mut self, index: usize, flags: u8) {
        let spec = &self.pool[index];
        let program_or_sysvar = matches!(spec.kind, Kind::Program(_) | Kind::Sysvar);
        if flags & ACCOUNT_SIGNER != 0 && !program_or_sysvar && !self.is_ballista_pda(index) {
            self.pool[index].signer = true;
        }
        if flags & ACCOUNT_WRITABLE != 0 && !program_or_sysvar {
            self.pool[index].writable = true;
        }
    }

    /// An account that satisfies `slot`, usually.
    fn satisfy(&mut self, slot: &Slot, assigned: &[usize]) -> usize {
        if !assigned.is_empty() && self.g.chance(1, 8) {
            // Alias an account another slot already holds.
            let index = assigned[self.g.below(assigned.len())];
            self.grant(index, slot.flags);
            return index;
        }
        if self.g.chance(1, 20) {
            // Anything at all.
            let index = self.g.below(self.pool.len());
            self.grant(index, slot.flags);
            return index;
        }
        let index = if let Some(address) = slot.address {
            match self.find(&address) {
                Some(index) => index,
                None => self.fresh(slot.owner.unwrap_or(SYSTEM_PROGRAM_ADDRESS), slot.min_len as usize),
            }
        } else if slot.flags & ACCOUNT_EXECUTABLE != 0 {
            let program = ALL_PROGRAMS[self.g.below(ALL_PROGRAMS.len())];
            self.program_index(program)
        } else {
            let min_len = slot.min_len as usize;
            let owner = slot.owner;
            let candidates: Vec<usize> = (0..self.pool.len())
                .filter(|index| {
                    let spec = &self.pool[*index];
                    !spec.executable
                        && spec.kind != Kind::Sysvar
                        && owner.is_none_or(|owner| spec.owner == owner)
                        && spec.data.len() >= min_len
                        && !(slot.flags & ACCOUNT_SIGNER != 0 && self.is_ballista_pda(*index))
                })
                .collect();
            if !candidates.is_empty() && self.g.chance(1, 2) {
                candidates[self.g.below(candidates.len())]
            } else {
                let owner = owner.unwrap_or_else(|| match self.g.below(4) {
                    0 => self.world.probe,
                    _ => SYSTEM_PROGRAM_ADDRESS,
                });
                if owner == self.world.ballista {
                    // The template account is the one Ballista-owned account always at hand.
                    self.find(&self.template_address).expect("in the base pool")
                } else {
                    self.fresh(owner, min_len)
                }
            }
        };
        self.grant(index, slot.flags);
        index
    }

    fn value(&mut self, value_type: u8, max_len: u16) -> Value {
        match value_type {
            VALUE_BOOL => Value::Bool(if self.g.chance(1, 40) { 2 } else { self.g.below(2) as u8 }),
            VALUE_U64 => Value::U64(self.g.interesting_u64()),
            VALUE_I64 => Value::I64(self.g.interesting_i64()),
            VALUE_U128 => Value::U128(self.g.interesting_u128()),
            VALUE_PUBKEY => {
                if self.g.chance(2, 3) {
                    let index = self.g.below(self.pool.len());
                    Value::Pubkey(self.pool[index].address)
                } else {
                    let mut key = [0u8; 32];
                    key[0] = self.g.u8();
                    Value::Pubkey(key)
                }
            }
            _ => {
                let len = if self.g.chance(1, 40) { max_len as usize + 1 } else { self.g.below(max_len as usize + 1) };
                Value::Bytes(self.g.bytes(len))
            }
        }
    }

    fn entry(&mut self, open: usize, inputs: &[Value]) -> (usize, Option<[u8; 32]>) {
        let spec = self.plan.opens[open].clone();
        let key = match &spec.key {
            KeySource::Zero => Some([0; 32]),
            KeySource::SlotKey(slot) => self.fixed.get(*slot).map(|index| self.pool[*index].address),
            KeySource::Input(input) => match inputs.get(*input) {
                Some(Value::Pubkey(key)) => Some(*key),
                _ => None,
            },
            KeySource::Const(key) => Some(*key),
        };
        let ballista = self.world.ballista;
        let size = spec.size as usize;
        let address = key.map(|key| entry_address(self.pda, &ballista, &self.template_address, spec.index, &key));
        let choice = self.g.weighted(&[30, 12, 30, 5, 4, 5, 3]);
        let index = match (choice, address, key) {
            (0, Some(address), _) => self.add(AccountSpec {
                address,
                owner: SYSTEM_PROGRAM_ADDRESS,
                lamports: 0,
                data: Vec::new(),
                executable: false,
                signer: false,
                writable: false,
                kind: Kind::Entry { open, state: EntryState::Absent },
            }),
            (1, Some(address), _) => {
                let lamports = 1 + self.g.below((self.rent)(REGISTRY_ENTRY_HEADER_LEN + size) as usize * 2) as u64;
                self.add(AccountSpec {
                    address,
                    owner: SYSTEM_PROGRAM_ADDRESS,
                    lamports,
                    data: Vec::new(),
                    executable: false,
                    signer: false,
                    writable: false,
                    kind: Kind::Entry { open, state: EntryState::Prefunded },
                })
            }
            (2, Some(address), Some(key)) => {
                let mut data = entry_header(&self.template_address, spec.index, &key).to_vec();
                data.extend(self.g.bytes(size));
                let lamports = (self.rent)(data.len()) + self.g.below(3) as u64 * 1000;
                self.add(AccountSpec {
                    address,
                    owner: ballista,
                    lamports,
                    data,
                    executable: false,
                    signer: false,
                    writable: false,
                    kind: Kind::Entry { open, state: EntryState::Existing },
                })
            }
            (3, _, _) | (4, _, _) => {
                let (template, index) = if choice == 3 {
                    let mut other = [0u8; 32];
                    other[0] = self.g.u8();
                    other[1] = 0x99;
                    (other, spec.index)
                } else {
                    (self.template_address, (spec.index + 1 + self.g.below(7) as u8) % MAX_REGISTRIES as u8)
                };
                let key = key.unwrap_or([0; 32]);
                let address = entry_address(self.pda, &ballista, &template, index, &key);
                let mut data = entry_header(&template, index, &key).to_vec();
                data.extend(self.g.bytes(size));
                let lamports = (self.rent)(data.len());
                let state = if choice == 3 { EntryState::OtherTemplate } else { EntryState::OtherIndex };
                self.add(AccountSpec {
                    address,
                    owner: ballista,
                    lamports,
                    data,
                    executable: false,
                    signer: false,
                    writable: false,
                    kind: Kind::Entry { open, state },
                })
            }
            (6, _, _) if !self.fixed.is_empty() => {
                // The account of an earlier entry slot, or any slot.
                let earlier: Vec<usize> = self
                    .plan
                    .opens
                    .iter()
                    .take(open)
                    .filter_map(|other| self.fixed.get(other.entry).copied())
                    .collect();
                match self.g.pick(&earlier) {
                    Some(index) => index,
                    None => self.fixed[self.g.below(self.fixed.len())],
                }
            }
            _ => match address {
                Some(address) => self.add(AccountSpec {
                    address,
                    owner: SYSTEM_PROGRAM_ADDRESS,
                    lamports: 0,
                    data: Vec::new(),
                    executable: false,
                    signer: false,
                    writable: false,
                    kind: Kind::Entry { open, state: EntryState::Absent },
                }),
                None => self.g.below(self.pool.len()),
            },
        };
        if self.g.chance(19, 20) {
            self.grant(index, ACCOUNT_WRITABLE);
        }
        (index, address)
    }

    fn run(&mut self) -> Scenario {
        self.base_pool();
        let plan = self.plan;

        // Inputs first: an open can take its key from one.
        let inputs: Vec<Value> = plan.inputs.iter().map(|input| self.value(input.value_type, input.max_len)).collect();

        // Fixed slots, then the entries, whose addresses depend on the others.
        let mut slots: Vec<Option<usize>> = vec![None; plan.fixed.len()];
        for (position, slot) in plan.fixed.iter().enumerate() {
            let assigned: Vec<usize> = slots.iter().flatten().copied().collect();
            let index = match &slot.role {
                Role::Entry(_) => continue,
                Role::Program(program) => {
                    let mut index = self.program_index(*program);
                    if slot.address.is_none() && self.g.chance(1, 6) {
                        let other = ALL_PROGRAMS[self.g.below(ALL_PROGRAMS.len())];
                        index = self.program_index(other);
                    }
                    index
                }
                Role::Sysvar => self.find(&INSTRUCTIONS_SYSVAR_ID).expect("in the base pool"),
                Role::Payer => {
                    let index = self.fresh(SYSTEM_PROGRAM_ADDRESS, 0);
                    self.pool[index].data.clear();
                    self.pool[index].lamports = if self.g.chance(1, 20) { self.g.below(5000) as u64 } else { 10_000_000_000 };
                    self.grant(index, slot.flags);
                    index
                }
                Role::NestedTemplate => {
                    if self.g.chance(2, 3) {
                        self.find(&self.template_address).expect("in the base pool")
                    } else {
                        self.satisfy(slot, &assigned)
                    }
                }
                Role::Plain => self.satisfy(slot, &assigned),
            };
            slots[position] = Some(index);
        }
        // `entry` reads `self.fixed` for keys and aliases; entries not yet assigned read as the
        // System program, which no key or alias uses.
        let system = self.program_index(Program::System);
        self.fixed = slots.iter().map(|slot| slot.unwrap_or(system)).collect();
        let mut entry_addresses = vec![None; plan.opens.len()];
        for (open, spec) in plan.opens.iter().enumerate() {
            let (index, address) = self.entry(open, &inputs);
            slots[spec.entry] = Some(index);
            self.fixed[spec.entry] = index;
            entry_addresses[open] = address;
        }
        let mut runtime: Vec<usize> = slots.into_iter().map(|slot| slot.expect("every slot assigned")).collect();

        // Rows.
        let iterations = if plan.batch_max == 0 {
            0
        } else if self.g.chance(9, 10) {
            self.g.range(plan.batch_min, plan.batch_max)
        } else {
            self.g.below(plan.batch_max + 2)
        };
        let mut row_values = Vec::new();
        for _ in 0..iterations {
            for slot in &plan.row {
                let assigned = runtime.clone();
                let index = self.satisfy(slot, &assigned);
                runtime.push(index);
            }
            for input in &plan.row_inputs {
                row_values.push(self.value(input.value_type, input.max_len));
            }
        }

        // Account groups: anything, entries and the template included, with random flags.
        let mut group_lengths = Vec::new();
        for _ in 0..plan.groups {
            let len = if self.g.chance(1, 30) { self.g.range(20, 66) } else { self.g.below(4) };
            group_lengths.push(len as u8);
            for _ in 0..len {
                let index = self.g.below(self.pool.len());
                let flags = if self.g.chance(1, 2) { ACCOUNT_WRITABLE } else { 0 }
                    | if self.g.chance(1, 6) { ACCOUNT_SIGNER } else { 0 };
                self.grant(index, flags);
                runtime.push(index);
            }
        }

        let mut run_data = vec![IX_RUN];
        run_data.extend_from_slice(&group_lengths);
        for value in inputs.iter().chain(row_values.iter()) {
            value.encode(&mut run_data);
        }

        // Stray privileges: the transaction may grant more than the template needs.
        for _ in 0..self.g.below(3) {
            let index = self.g.below(self.pool.len());
            let flags = if self.g.chance(1, 2) { ACCOUNT_WRITABLE } else { ACCOUNT_SIGNER };
            self.grant(index, flags);
        }

        let template = self.find(&self.template_address).expect("in the base pool");
        if self.g.chance(1, 10) {
            self.pool[template].writable = true;
        }

        let mut mutation = None;
        if self.g.chance(1, 8) {
            mutation = Some(self.mutate(&mut runtime, &mut run_data));
        }

        let before = (0..self.g.below(3)).map(|_| self.extra()).collect();
        let after = (0..self.g.below(3)).map(|_| self.extra()).collect();
        let wrap = self.g.chance(1, 10).then(|| if self.g.chance(1, 2) { 0 } else { self.g.u8() });

        Scenario {
            pool: std::mem::take(&mut self.pool),
            template,
            slots: runtime,
            run_data,
            iterations,
            group_lengths,
            before,
            after,
            wrap,
            mutation,
            entry_addresses,
        }
    }

    fn mutate(&mut self, runtime: &mut Vec<usize>, run_data: &mut Vec<u8>) -> &'static str {
        match self.g.below(8) {
            0 if run_data.len() > 1 => {
                let at = 1 + self.g.below(run_data.len() - 1);
                run_data[at] ^= 1 << self.g.below(8);
                "flipped a bit of the run data"
            }
            1 if run_data.len() > 1 => {
                let cut = 1 + self.g.below((run_data.len() - 1).min(4));
                run_data.truncate(run_data.len() - cut);
                "truncated the run data"
            }
            2 => {
                let extra = self.g.range(1, 3);
                run_data.extend(self.g.bytes(extra));
                "appended to the run data"
            }
            3 if !runtime.is_empty() => {
                runtime.pop();
                "dropped the last runtime account"
            }
            4 => {
                let index = self.g.below(self.pool.len());
                runtime.push(index);
                "added a runtime account"
            }
            5 if !runtime.is_empty() => {
                let at = self.g.below(runtime.len());
                let index = self.g.below(self.pool.len());
                runtime[at] = index;
                "replaced a runtime account"
            }
            6 => {
                let signers: Vec<usize> = (0..self.pool.len()).filter(|index| self.pool[*index].signer).collect();
                match self.g.pick(&signers) {
                    Some(index) => {
                        self.pool[index].signer = false;
                        "withheld a signature"
                    }
                    None => "nothing to withhold",
                }
            }
            _ => {
                let writable: Vec<usize> = (0..self.pool.len()).filter(|index| self.pool[*index].writable).collect();
                match self.g.pick(&writable) {
                    Some(index) => {
                        self.pool[index].writable = false;
                        "made a writable account read-only"
                    }
                    None => "nothing to demote",
                }
            }
        }
    }

    /// A probe instruction elsewhere in the transaction, for the Instructions sysvar to list. Its
    /// metas can grant privileges the run then sees, as any other instruction's can.
    fn extra(&mut self) -> Extra {
        let mut accounts = Vec::new();
        for _ in 0..self.g.below(4) {
            let index = self.g.below(self.pool.len());
            if matches!(self.pool[index].kind, Kind::Program(_) | Kind::Sysvar) {
                continue;
            }
            if self.g.chance(1, 4) {
                self.grant(index, ACCOUNT_WRITABLE);
            }
            accounts.push(index);
        }
        let op = [probe::NOOP, probe::WRITE_FIRST, probe::RESIZE_FIRST, probe::TRANSFER][self.g.weighted(&[6, 1, 1, 1])];
        let mut data = vec![op];
        let len = self.g.below(12);
        data.extend(self.g.bytes(len));
        Extra { accounts, data }
    }
}
