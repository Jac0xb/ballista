//! Negative mutations: take a program `verify` accepts, break exactly one rule the docs promise,
//! and require `verify` to reject the result with that rule's error.
//!
//! The proptests in `common/tests` only check that generated programs verify and that nothing
//! panics, so they stay green when a verifier rule is deleted. These breaks sample rejection.
//! Each one changes a single field, or inserts a single instruction, and names the exact error the
//! verifier must return, worked out from the order it checks things in: instructions in pc order,
//! a loop's body when its header is reached, the CPI count and the descriptors' shapes last.
//!
//! An accepted break is a finding. A rejection with another error is either a finding about the
//! verifier's error reporting or a break that changed more than it meant to; triage tells which.

use ballista_common::template::{ProgramView, TemplateError};

use crate::{
    ceiling::{self, Site},
    checker::{self, op, read_opcode, Ty, EXECUTABLE, NONE, ROW_BIT, SIGNER, WRITABLE},
    model::{Instr, Program},
};

/// One broken rule: the program with it broken, and what `verify` must say.
#[derive(Clone, Debug)]
pub struct Break {
    pub rule: &'static str,
    pub program: Program,
    pub expected: TemplateError,
}

/// How many breaks of one kind to make from one program; the rest are alike.
const PER_KIND: usize = 6;

/// Every break that applies to `program`, which `verify` accepts.
pub fn breaks(program: &Program) -> Vec<Break> {
    let mut out = Vec::new();
    privilege(program, &mut out);
    record_flags(program, &mut out);
    undeclared_account(program, &mut out);
    undeclared_program(program, &mut out);
    undeclared_group(program, &mut out);
    cpi_count(program, &mut out);
    read_before_write(program, &mut out);
    type_mismatch(program, &mut out);
    emit_tag(program, &mut out);
    return_data_placement(program, &mut out);
    entry_writable(program, &mut out);
    guarded_return_data(program, &mut out);
    read_bounds(program, &mut out);
    sysvar_pin(program, &mut out);
    ninth_loop(program, &mut out);
    ninth_open(program, &mut out);
    open_after_return_data(program, &mut out);
    open_after_invoke(program, &mut out);
    unused_field(program, &mut out);
    cpi_segment_fields(program, &mut out);
    unreferenced_segment(program, &mut out);
    uninvoked_descriptor(program, &mut out);
    out
}

/// Runs one break and panics unless `verify` rejects it with the expected error.
pub fn require_rejected(broken: &Break) {
    let bytes = broken.program.encode();
    match ProgramView::parse(&bytes).and_then(|view| view.verify()) {
        Ok(_) => panic!(
            "verify accepted a program with one rule broken ({}); expected {:?}",
            broken.rule, broken.expected
        ),
        Err(error) if error == broken.expected => {}
        Err(error) => panic!(
            "breaking {} made verify report {error:?}; expected {:?}",
            broken.rule, broken.expected
        ),
    }
}

/// The first invoke, in the order `verify` reaches them, whose descriptor lists record `index`.
fn first_invoke_listing(program: &Program, index: usize) -> Option<(usize, Site, usize)> {
    ceiling::invoke_sites(program).into_iter().find_map(|(pc, site)| {
        let descriptor = program.instrs[pc].a as usize;
        let cpi = program.cpis.get(descriptor)?;
        let start = cpi.account_start as usize;
        (start..start + cpi.account_len as usize).contains(&index).then_some((pc, site, descriptor))
    })
}

/// A CPI account record passed with a privilege its slot's declaration lacks: `InvalidCpi` at the
/// first invoke that lists it.
fn privilege(program: &Program, out: &mut Vec<Break>) {
    let mut made = 0;
    for (index, record) in program.cpi_accounts.iter().enumerate() {
        let Some((_, site, descriptor)) = first_invoke_listing(program, index) else { continue };
        let Some(declared) = ceiling::declared(program, record.account, site) else { continue };
        let missing = (SIGNER | WRITABLE) & !declared & !record.flags;
        for bit in [SIGNER, WRITABLE] {
            if missing & bit == 0 || made >= PER_KIND {
                continue;
            }
            let mut broken = program.clone();
            broken.cpi_accounts[index].flags |= bit;
            out.push(Break {
                rule: if bit == SIGNER { "privilege-signer" } else { "privilege-writable" },
                program: broken,
                expected: TemplateError::InvalidCpi(descriptor),
            });
            made += 1;
        }
    }
}

/// A record carrying the executable bit, which no record may: its flags are signer and writable
/// only. The slot is one declared executable, so the privilege ceiling alone would let it through:
/// the record's own slot when it declares that, or else the descriptor's program slot, retargeted.
fn record_flags(program: &Program, out: &mut Vec<Break>) {
    let mut made = 0;
    for index in 0..program.cpi_accounts.len() {
        if made >= 2 {
            break;
        }
        let Some((pc, site, descriptor)) = first_invoke_listing(program, index) else { continue };
        let mut broken = program.clone();
        let record = &mut broken.cpi_accounts[index];
        let declared = ceiling::declared(program, record.account, site).unwrap_or(0);
        if declared & EXECUTABLE == 0 {
            record.account = program.cpis[program.instrs[pc].a as usize].program;
            record.flags = 0;
        }
        record.flags |= EXECUTABLE;
        out.push(Break { rule: "record-flags", program: broken, expected: TemplateError::InvalidCpi(descriptor) });
        made += 1;
    }
}

/// A record naming the slot one past the fixed accounts, which nothing declares.
fn undeclared_account(program: &Program, out: &mut Vec<Break>) {
    let fixed = program.header.fixed_accounts;
    if fixed >= ROW_BIT {
        return;
    }
    let mut made = 0;
    for index in 0..program.cpi_accounts.len() {
        let Some((_, _, descriptor)) = first_invoke_listing(program, index) else { continue };
        if made >= PER_KIND {
            break;
        }
        let mut broken = program.clone();
        broken.cpi_accounts[index].account = fixed;
        out.push(Break { rule: "undeclared-account", program: broken, expected: TemplateError::InvalidCpi(descriptor) });
        made += 1;
    }
}

/// An invoked descriptor whose program account is undeclared.
fn undeclared_program(program: &Program, out: &mut Vec<Break>) {
    let fixed = program.header.fixed_accounts;
    if fixed >= ROW_BIT {
        return;
    }
    let mut seen = Vec::new();
    for (pc, _) in ceiling::invoke_sites(program) {
        let descriptor = program.instrs[pc].a as usize;
        if seen.contains(&descriptor) || seen.len() >= PER_KIND {
            continue;
        }
        seen.push(descriptor);
        let mut broken = program.clone();
        broken.cpis[descriptor].program = fixed;
        out.push(Break { rule: "undeclared-program", program: broken, expected: TemplateError::InvalidCpi(descriptor) });
    }
}

/// An invoked descriptor forwarding the group one past the declared ones.
fn undeclared_group(program: &Program, out: &mut Vec<Break>) {
    let groups = program.header.account_groups;
    if groups >= NONE {
        return;
    }
    if let Some((pc, _)) = ceiling::invoke_sites(program).first() {
        let descriptor = program.instrs[*pc].a as usize;
        let mut broken = program.clone();
        broken.cpis[descriptor].group = groups;
        out.push(Break { rule: "undeclared-group", program: broken, expected: TemplateError::InvalidCpi(descriptor) });
    }
}

/// A loop maximum raised until the worst-case CPI count passes 64.
fn cpi_count(program: &Program, out: &mut Vec<Break>) {
    let Ok(before) = ceiling::check(program) else { return };
    let instrs = &program.instrs;
    let mut foreach_invokes = 0;
    let mut pc = 0;
    while pc < instrs.len() {
        let instr = instrs[pc];
        if !matches!(instr.op, op::FOREACH | op::REPEAT) {
            pc += 1;
            continue;
        }
        let end = (pc + 1 + instr.a as usize).min(instrs.len());
        let invokes = instrs[pc + 1..end].iter().filter(|body| body.op == op::INVOKE).count();
        if instr.op == op::REPEAT && invokes > 0 && (instr.c as usize) < 255 {
            let mut broken = program.clone();
            broken.instrs[pc].c = 255;
            out.push(Break { rule: "cpi-count-repeat", program: broken, expected: TemplateError::ExcessiveCpiExpansion });
        }
        if instr.op == op::FOREACH {
            foreach_invokes += invokes;
        }
        pc = end;
    }
    // Every FOREACH makes the header's maximum rows: raise it as little as passes 64, within the
    // account and input-value limits, so nothing else breaks.
    let header = &program.header;
    let max = header.max_rows as usize;
    if foreach_invokes == 0 || max == 0 {
        return;
    }
    let others = before.worst_case_cpis - foreach_invokes * max;
    let needed = (64 - others) / foreach_invokes + 1;
    let fits = header.fixed_accounts as usize + header.batch_stride as usize * needed <= checker::limit::RUNTIME_ACCOUNTS
        && header.fixed_inputs as usize + header.row_inputs as usize * needed <= checker::limit::INPUT_VALUES;
    if needed > max && needed <= 255 && fits {
        let mut broken = program.clone();
        broken.header.max_rows = needed as u8;
        out.push(Break { rule: "cpi-count-foreach", program: broken, expected: TemplateError::ExcessiveCpiExpansion });
    }
}

/// A `REQUIRE` of a register at pc 0, before anything writes it.
fn read_before_write(program: &Program, out: &mut Vec<Break>) {
    let registers = program.header.registers;
    if registers == 0 || program.instrs.len() >= checker::limit::INSTRUCTIONS {
        return;
    }
    // A register some instruction reads, so the break reads a live one.
    let register = program
        .instrs
        .iter()
        .find(|instr| instr.op == op::REQUIRE || instr.op == op::MOVE)
        .map_or(0, |instr| instr.a)
        .min(registers - 1);
    let mut broken = program.clone();
    broken.instrs.insert(0, Instr::new(op::REQUIRE, NONE, register, NONE, NONE, 0));
    broken.sync_counts();
    out.push(Break {
        rule: "read-before-write",
        program: broken,
        expected: TemplateError::RegisterNotInitialized(register),
    });
}

/// A root `REQUIRE` pointed at a register that holds something other than a `bool` there.
fn type_mismatch(program: &Program, out: &mut Vec<Break>) {
    let mut made = 0;
    for (pc, instr) in program.instrs.iter().enumerate() {
        if instr.op != op::REQUIRE || made >= 2 {
            continue;
        }
        let Some(state) = checker::root_state_before(program, pc) else { continue };
        let Some(register) = state.iter().position(|ty| matches!(ty, Ty::U64 | Ty::I64 | Ty::U128 | Ty::Pubkey | Ty::Bytes(_)))
        else {
            continue;
        };
        let mut broken = program.clone();
        broken.instrs[pc].a = register as u8;
        out.push(Break { rule: "type-mismatch", program: broken, expected: TemplateError::TypeMismatch });
        made += 1;
    }
}

/// The first `EMIT`'s tag rewritten to start with the run event's `BEV`.
fn emit_tag(program: &Program, out: &mut Vec<Break>) {
    let Some(pc) = program.instrs.iter().position(|instr| instr.op == op::EMIT) else { return };
    let (start, _) = program.instrs[pc].range();
    let Some(tag) = program.segments.get(start) else { return };
    if tag.kind != 0 || tag.len < 4 {
        return;
    }
    let mut broken = program.clone();
    let at = tag.offset as usize;
    broken.blob[at..at + 3].copy_from_slice(b"BEV");
    out.push(Break { rule: "emit-tag", program: broken, expected: TemplateError::InvalidOutput(pc) });
}

/// A `SET_RETURN_DATA` of one empty literal, inserted where none may go: before the first root
/// invoke, since an invoke clears return data, and as the first step of the first loop body. The
/// literal is a new segment, so nothing else changes. Programs that already set return data are
/// left alone, so the "once" rule never fires first.
fn return_data_placement(program: &Program, out: &mut Vec<Break>) {
    if program.instrs.len() >= checker::limit::INSTRUCTIONS
        || program.instrs.iter().any(|instr| instr.op == op::SET_RETURN_DATA)
    {
        return;
    }
    let with_output = |at: usize| {
        let mut broken = program.clone();
        let segment = broken.segments.len() as u64;
        broken.segments.push(crate::model::Segment { kind: 0, register: NONE, offset: 0, len: 0, reserved: [0; 2] });
        broken.instrs.insert(at, Instr::new(op::SET_RETURN_DATA, NONE, NONE, NONE, NONE, segment | 1 << 32));
        broken
    };
    if let Some((pc, _)) = ceiling::invoke_sites(program).into_iter().find(|(_, site)| *site == Site::Root) {
        let mut broken = with_output(pc);
        broken.sync_counts();
        out.push(Break { rule: "return-data-before-invoke", program: broken, expected: TemplateError::InvalidOutput(pc) });
    }
    if let Some(header) = program.instrs.iter().position(|instr| matches!(instr.op, op::FOREACH | op::REPEAT)) {
        if program.instrs[header].a < u8::MAX {
            let mut broken = with_output(header + 1);
            broken.instrs[header].a += 1;
            broken.sync_counts();
            out.push(Break { rule: "return-data-in-loop", program: broken, expected: TemplateError::InvalidOutput(header + 1) });
        }
    }
}

/// A CPI account record turned into the first opened entry, passed writable. The entry's slot is
/// declared writable, so the privilege ceiling allows it; the registry rule does not.
fn entry_writable(program: &Program, out: &mut Vec<Break>) {
    let Some(open) = program.instrs.iter().position(|instr| instr.op == op::OPEN_REGISTRY) else { return };
    let entry = program.instrs[open].a;
    for index in 0..program.cpi_accounts.len().min(3) {
        let mut broken = program.clone();
        broken.cpi_accounts[index].account = entry;
        broken.cpi_accounts[index].flags = WRITABLE;
        out.push(Break { rule: "entry-writable", program: broken, expected: TemplateError::InvalidRegistry(open) });
    }
}

/// The invoke a root `RETURN_DATA` reads, given a guard: return data is read only after an invoke
/// that always runs. The guard is a `bool` already set there, or else a new register that a
/// `CONST_BOOL` inserted just before the invoke sets; either way the guard itself is valid.
fn guarded_return_data(program: &Program, out: &mut Vec<Break>) {
    for (pc, instr) in program.instrs.iter().enumerate() {
        if instr.op != op::RETURN_DATA || pc == 0 || program.instrs[pc - 1].op != op::INVOKE {
            continue;
        }
        let invoke = pc - 1;
        let Some(state) = checker::root_state_before(program, invoke) else { continue };
        let mut broken = program.clone();
        let read = match state.iter().position(|ty| *ty == Ty::Bool) {
            Some(guard) => {
                broken.instrs[invoke].b = guard as u8;
                pc
            }
            None => {
                let guard = program.header.registers;
                if guard as usize >= checker::limit::REGISTERS || program.instrs.len() >= checker::limit::INSTRUCTIONS {
                    continue;
                }
                broken.header.registers += 1;
                broken.instrs[invoke].b = guard;
                broken.instrs.insert(invoke, Instr::new(op::CONST_BOOL, guard, 1, NONE, NONE, 0));
                broken.sync_counts();
                pc + 1
            }
        };
        out.push(Break { rule: "guarded-return-data", program: broken, expected: TemplateError::InvalidReturnData(read) });
        return;
    }
}

/// A fixed-offset read moved one byte past its account's declared minimum length.
fn read_bounds(program: &Program, out: &mut Vec<Break>) {
    let fixed = program.header.fixed_accounts as usize;
    let mut made = 0;
    for (pc, instr) in program.instrs.iter().enumerate() {
        let Some((width, _)) = read_opcode(instr.op) else { continue };
        if instr.flags != 0 || made >= 2 {
            continue;
        }
        let slot = if instr.a & ROW_BIT == 0 {
            instr.a as usize
        } else {
            fixed + (instr.a & !ROW_BIT) as usize
        };
        let Some(account) = program.accounts.get(slot) else { continue };
        let min_len = account.min_len as u64;
        if min_len < width as u64 {
            continue;
        }
        let mut broken = program.clone();
        broken.instrs[pc].imm = min_len - width as u64 + 1;
        out.push(Break { rule: "read-bounds", program: broken, expected: TemplateError::ReadOutOfBounds(pc) });
        made += 1;
    }
}

/// The Instructions sysvar's pinned address changed by one bit.
fn sysvar_pin(program: &Program, out: &mut Vec<Break>) {
    let Some(pc) = program
        .instrs
        .iter()
        .position(|instr| (op::INSTRUCTION_COUNT..=op::READ_INSTRUCTION_BYTES).contains(&instr.op))
    else {
        return;
    };
    let account = program.instrs[pc].a as usize;
    let Some(pin) = program.accounts.get(account).map(|account| account.address as usize) else { return };
    if pin >= program.pubkeys.len() {
        return;
    }
    let mut broken = program.clone();
    broken.pubkeys[pin][31] ^= 1;
    out.push(Break { rule: "sysvar-pin", program: broken, expected: TemplateError::InvalidIntrospection(pc) });
}

/// Loops appended at the end of the root until there are nine: each a `REPEAT` of one pass over a
/// one-instruction body, counted by a new `u64` register. A template holds at most eight loops,
/// so the ninth header is refused with `InvalidLoop`.
fn ninth_loop(program: &Program, out: &mut Vec<Break>) {
    let loops = program.instrs.iter().filter(|instr| matches!(instr.op, op::FOREACH | op::REPEAT)).count();
    let missing = 9usize.saturating_sub(loops);
    let registers = program.header.registers as usize;
    if missing == 0 || program.instrs.len() + 1 + 2 * missing > checker::limit::INSTRUCTIONS || registers + 2 > 64 {
        return;
    }
    let mut broken = program.clone();
    let (count, scratch) = (registers as u8, registers as u8 + 1);
    broken.header.registers += 2;
    broken.instrs.push(Instr::new(op::CONST_U64, count, NONE, NONE, NONE, 1));
    let mut ninth = 0;
    for _ in 0..missing {
        ninth = broken.instrs.len();
        broken.instrs.push(Instr::new(op::REPEAT, NONE, 1, count, 1, 0));
        broken.instrs.push(Instr::new(op::CONST_BOOL, scratch, 1, NONE, NONE, 0));
    }
    broken.sync_counts();
    out.push(Break { rule: "ninth-loop", program: broken, expected: TemplateError::InvalidLoop(ninth) });
}

/// Opens added after the first one until there are nine, each on a new entry account of the
/// same registry and the same payer and System program. A template opens at most eight entries, so
/// the ninth open is refused with `InvalidRegistry`, before the CPI count is checked.
fn ninth_open(program: &Program, out: &mut Vec<Break>) {
    let Some(first) = program.instrs.iter().position(|instr| instr.op == op::OPEN_REGISTRY) else { return };
    let opens = program.instrs.iter().filter(|instr| instr.op == op::OPEN_REGISTRY).count();
    let missing = 9usize.saturating_sub(opens);
    let fixed = program.header.fixed_accounts as usize;
    if missing == 0 || fixed + missing >= ROW_BIT as usize || program.instrs.len() + missing > checker::limit::INSTRUCTIONS {
        return;
    }
    // A CPI account record nothing invokes can name a slot past the fixed accounts, since such
    // records go unchecked. Declared as an entry, that slot would turn the record into an entry
    // passed writable, a second broken rule, so such programs are left alone.
    if program.cpi_accounts.iter().any(|record| (fixed..fixed + missing).contains(&(record.account as usize))) {
        return;
    }
    let mut broken = program.clone();
    let template = program.instrs[first];
    for added in 0..missing {
        // A new fixed account, declared writable and nothing else, goes after the fixed accounts;
        // row accounts follow and are named by their offset, so no reference moves.
        let entry = (fixed + added) as u8;
        broken.accounts.insert(fixed + added, crate::model::Account { flags: WRITABLE, address: NONE, owner: NONE, reserved: 0, min_len: 0 });
        broken.header.fixed_accounts += 1;
        broken.instrs.insert(first + 1 + added, Instr { a: entry, b: NONE, ..template });
    }
    broken.sync_counts();
    // The ninth in pc order: opens the program already had after the first now sit after the
    // added ones.
    let Some(ninth) = broken.instrs.iter().enumerate().filter(|(_, instr)| instr.op == op::OPEN_REGISTRY).nth(8) else {
        return;
    };
    let ninth = ninth.0;
    out.push(Break { rule: "ninth-open", program: broken, expected: TemplateError::InvalidRegistry(ninth) });
}

/// A `SET_RETURN_DATA` of one empty literal before the first registry open, which may not follow
/// one, since creating an entry calls the System program. If an invoke comes later, the output's
/// own placement check refuses it first, with `InvalidOutput`.
fn open_after_return_data(program: &Program, out: &mut Vec<Break>) {
    let Some(open) = program.instrs.iter().position(|instr| instr.op == op::OPEN_REGISTRY) else { return };
    if program.instrs.len() >= checker::limit::INSTRUCTIONS
        || program.instrs.iter().any(|instr| instr.op == op::SET_RETURN_DATA)
    {
        return;
    }
    let mut broken = program.clone();
    let segment = broken.segments.len() as u64;
    broken.segments.push(crate::model::Segment { kind: 0, register: NONE, offset: 0, len: 0, reserved: [0; 2] });
    broken.instrs.insert(open, Instr::new(op::SET_RETURN_DATA, NONE, NONE, NONE, NONE, segment | 1 << 32));
    broken.sync_counts();
    let invoke_later = broken.instrs[open + 1..].iter().any(|instr| instr.op == op::INVOKE);
    let expected = if invoke_later { TemplateError::InvalidOutput(open) } else { TemplateError::InvalidRegistry(open + 1) };
    out.push(Break { rule: "open-after-return-data", program: broken, expected });
}

/// An `INVOKE` inserted just before the first registry open: a call of a new descriptor that passes
/// no account and no data to a fixed account declared executable, valid on its own. Until an open
/// marks its entry, a CPI could reach the entry through another slot, so every open precedes every
/// invoke: the open, now one later, is refused with `InvalidRegistry`.
fn open_after_invoke(program: &Program, out: &mut Vec<Break>) {
    let Some(open) = program.instrs.iter().position(|instr| instr.op == op::OPEN_REGISTRY) else { return };
    let fixed = program.header.fixed_accounts as usize;
    let Some(executable) = program.accounts[..fixed].iter().position(|account| account.flags & EXECUTABLE != 0) else {
        return;
    };
    if program.instrs.len() >= checker::limit::INSTRUCTIONS || program.cpis.len() >= u8::MAX as usize {
        return;
    }
    let mut broken = program.clone();
    let descriptor = broken.cpis.len() as u8;
    broken.cpis.push(crate::model::Cpi {
        program: executable as u8,
        group: NONE,
        account_start: 0,
        account_len: 0,
        segment_len: 0,
        segment_start: 0,
        max_data_len: 0,
        reserved: [0; 2],
    });
    broken.instrs.insert(open, Instr::new(op::INVOKE, NONE, descriptor, NONE, NONE, 0));
    broken.sync_counts();
    out.push(Break { rule: "open-after-invoke", program: broken, expected: TemplateError::InvalidRegistry(open + 1) });
}

/// The error `verify` refuses a record with when a field its opcode leaves unused is set
/// (`docs/reference/wire-format.md`, "Instruction record"): a FOREACH's `InvalidBatch`, a
/// REPEAT's `InvalidLoop`, the registry opcodes' `InvalidRegistry`, and otherwise
/// `InvalidInstruction` at the record.
fn unused_field_error(opcode: u8, pc: usize) -> TemplateError {
    match opcode {
        op::FOREACH => TemplateError::InvalidBatch,
        op::REPEAT => TemplateError::InvalidLoop(pc),
        op::OPEN_REGISTRY | op::READ_REGISTRY | op::WRITE_REGISTRY => TemplateError::InvalidRegistry(pc),
        _ => TemplateError::InvalidInstruction(pc),
    }
}

/// A field its opcode leaves unused, set: the first unused destination or operand to 0, or an
/// unused immediate to 1. Every other record stays canonical, so the record is refused where
/// `verify` reaches it, with its opcode's error.
fn unused_field(program: &Program, out: &mut Vec<Break>) {
    let (mut operands, mut immediates) = (0, 0);
    for (pc, instr) in program.instrs.iter().enumerate() {
        let Some(used) = checker::used_fields(instr.op, instr.flags) else { continue };
        if operands < 3 {
            let mut broken = program.clone();
            let record = &mut broken.instrs[pc];
            let field = if !used.dst {
                Some(&mut record.dst)
            } else if !used.a {
                Some(&mut record.a)
            } else if !used.b {
                Some(&mut record.b)
            } else if !used.c {
                Some(&mut record.c)
            } else {
                None
            };
            if let Some(field) = field {
                *field = 0;
                out.push(Break { rule: "unused-operand", program: broken, expected: unused_field_error(instr.op, pc) });
                operands += 1;
            }
        }
        if !used.imm && immediates < 2 {
            let mut broken = program.clone();
            broken.instrs[pc].imm = 1;
            out.push(Break { rule: "unused-immediate", program: broken, expected: unused_field_error(instr.op, pc) });
            immediates += 1;
        }
    }
}

/// A data segment of an invoked descriptor, given the field its kind leaves unused: a literal a
/// source register, a register segment a blob offset. Every use checks a segment alike and names
/// it by its index in the table, so wherever `verify` first reaches it, the error is
/// `InvalidDataSegment` with that index.
fn cpi_segment_fields(program: &Program, out: &mut Vec<Break>) {
    let (mut literal, mut register) = (false, false);
    for (pc, _) in ceiling::invoke_sites(program) {
        let Some(cpi) = program.cpis.get(program.instrs[pc].a as usize) else { continue };
        let start = cpi.segment_start as usize;
        for index in start..start + cpi.segment_len as usize {
            let mut broken = program.clone();
            if broken.segments[index].kind == 0 && !literal {
                broken.segments[index].register = 0;
                out.push(Break {
                    rule: "cpi-segment-literal-register",
                    program: broken,
                    expected: TemplateError::InvalidDataSegment(index),
                });
                literal = true;
            } else if broken.segments[index].kind != 0 && !register {
                broken.segments[index].offset = 1;
                out.push(Break {
                    rule: "cpi-segment-register-fields",
                    program: broken,
                    expected: TemplateError::InvalidDataSegment(index),
                });
                register = true;
            }
        }
    }
}

/// A well-formed, empty literal segment appended to the table, which nothing names. It is refused
/// once the walk is over, after the descriptors, with `InvalidDataSegment` and its index.
fn unreferenced_segment(program: &Program, out: &mut Vec<Break>) {
    if program.segments.len() >= u16::MAX as usize || program.encode().len() + 8 > checker::limit::PAYLOAD {
        return;
    }
    let mut broken = program.clone();
    let index = broken.segments.len();
    broken.segments.push(crate::model::Segment { kind: 0, register: NONE, offset: 0, len: 0, reserved: [0; 2] });
    broken.sync_counts();
    out.push(Break { rule: "unreferenced-segment", program: broken, expected: TemplateError::InvalidDataSegment(index) });
}

/// A well-formed descriptor appended to the table, which nothing invokes: a call of a fixed
/// account declared executable, with no account and no data. It is refused once the walk is over,
/// with `InvalidCpi` and its index.
fn uninvoked_descriptor(program: &Program, out: &mut Vec<Break>) {
    let fixed = program.header.fixed_accounts as usize;
    let Some(executable) = program.accounts[..fixed].iter().position(|account| account.flags & EXECUTABLE != 0) else {
        return;
    };
    if program.cpis.len() >= u8::MAX as usize || program.encode().len() + 12 > checker::limit::PAYLOAD {
        return;
    }
    let mut broken = program.clone();
    let descriptor = broken.cpis.len();
    broken.cpis.push(crate::model::Cpi {
        program: executable as u8,
        group: NONE,
        account_start: 0,
        account_len: 0,
        segment_len: 0,
        segment_start: 0,
        max_data_len: 0,
        reserved: [0; 2],
    });
    broken.sync_counts();
    out.push(Break { rule: "uninvoked-descriptor", program: broken, expected: TemplateError::InvalidCpi(descriptor) });
}
