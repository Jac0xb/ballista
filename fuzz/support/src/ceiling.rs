//! The CPI guarantees only finalization enforces, checked on their own.
//!
//! The run checks most of what the verifier promises again: register types, read bounds, the
//! Instructions sysvar's address. A hole in the verifier there makes a run revert. A CPI's
//! account flags are different: `invoke_cpi` in `programs/ballista/src/processor/execute.rs`
//! copies each record's signer and writable bits into the instruction it builds and checks
//! nothing. So for every invoke a verified program holds, this pass re-derives:
//!
//! 1. **The privilege ceiling, per slot.** Each account record's flags are signer or writable
//!    only, and only ones the declaration of the slot it names requires. The run checks that the
//!    transaction granted each declared privilege, so a callee never gets a privilege on a slot
//!    beyond its declaration.
//! 2. **Every account an invoke passes is declared.** A record names a fixed account below the
//!    fixed count, or, in a `FOREACH` body only, a row account below the stride; the program
//!    account too, and it is declared executable. The record range lies inside the table, and a
//!    forwarded group is a declared one.
//! 3. **The loop-expanded CPI count.** Invokes at the root, plus each loop body's invokes times
//!    that loop's maximum, plus three per registry open, is at most 64, and is the number the
//!    verifier reports.
//!
//! Per address the ceiling is wider, by design, and finalization cannot see it: the caller picks
//! the address in each slot and the members of each account group. One address in a slot declared
//! read-only and in a slot declared writable is writable to a CPI that passes the second slot
//! writable; one in a read-only slot and in a group the CPI forwards is writable to it whenever
//! the transaction marked it writable, since group members take the transaction's flag and the
//! runtime merges the privileges of an account listed twice. Never signer: group members are never
//! passed as signers, so an address is a signer to a callee only through a slot declared signer.
//! [`address_ceiling`] states that bound for one assignment of addresses.
//!
//! The pass reads the [`Program`] model and nothing of `verify.rs`, so it can disagree with it.

use crate::{
    checker::{op, Violation, EXECUTABLE, NONE, ROW_BIT, SIGNER, WRITABLE},
    model::Program,
};

/// What the pass found about a program it accepts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ceiling {
    /// CPIs in the worst case.
    pub worst_case_cpis: usize,
    /// Invoke sites checked, counting each once however many passes its loop makes.
    pub invoke_sites: usize,
    /// Account records checked across those sites.
    pub records: usize,
}

fn fail<T>(rule: &'static str, pc: usize, detail: String) -> Result<T, Violation> {
    Err(Violation { rule, pc: Some(pc), detail })
}

/// Where an invoke sits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Site {
    Root,
    Foreach,
    Repeat,
}

/// Every invoke in `program`, in the order `verify` reaches them: pc order, a loop's body when
/// its header is reached. Each with where it sits.
pub fn invoke_sites(program: &Program) -> Vec<(usize, Site)> {
    let instrs = &program.instrs;
    let mut sites = Vec::new();
    let mut pc = 0;
    while pc < instrs.len() {
        let instr = instrs[pc];
        if matches!(instr.op, op::FOREACH | op::REPEAT) {
            let site = if instr.op == op::FOREACH { Site::Foreach } else { Site::Repeat };
            let end = (pc + 1 + instr.a as usize).min(instrs.len());
            sites.extend((pc + 1..end).filter(|&body| instrs[body].op == op::INVOKE).map(|body| (body, site)));
            pc = end;
        } else {
            if instr.op == op::INVOKE {
                sites.push((pc, Site::Root));
            }
            pc += 1;
        }
    }
    sites
}

/// Checks every invoke in `program`.
pub fn check(program: &Program) -> Result<Ceiling, Violation> {
    let instrs = &program.instrs;
    let mut ceiling = Ceiling::default();
    let mut pc = 0;
    while pc < instrs.len() {
        let instr = instrs[pc];
        match instr.op {
            op::FOREACH | op::REPEAT => {
                let end = pc + 1 + instr.a as usize;
                if end > instrs.len() {
                    return fail("ceiling.loop-body", pc, format!("body ends at {end} of {}", instrs.len()));
                }
                let (site, passes) = if instr.op == op::FOREACH {
                    (Site::Foreach, program.header.max_rows as usize)
                } else {
                    (Site::Repeat, instr.c as usize)
                };
                for body_pc in pc + 1..end {
                    match instrs[body_pc].op {
                        op::INVOKE => {
                            invoke(program, body_pc, site, &mut ceiling)?;
                            ceiling.worst_case_cpis += passes;
                        }
                        op::OPEN_REGISTRY | op::FOREACH | op::REPEAT => {
                            return fail("ceiling.in-body", body_pc, format!("opcode {}", instrs[body_pc].op));
                        }
                        _ => {}
                    }
                }
                pc = end;
            }
            op::INVOKE => {
                invoke(program, pc, Site::Root, &mut ceiling)?;
                ceiling.worst_case_cpis += 1;
                pc += 1;
            }
            op::OPEN_REGISTRY => {
                ceiling.worst_case_cpis += 3;
                pc += 1;
            }
            _ => pc += 1,
        }
    }
    if ceiling.worst_case_cpis > 64 {
        return fail("ceiling.cpi-count", instrs.len(), format!("{} CPIs in the worst case", ceiling.worst_case_cpis));
    }
    Ok(ceiling)
}

/// The declared flags of the slot `reference` names at `site`, or `None` if it names no slot.
pub fn declared(program: &Program, reference: u8, site: Site) -> Option<u8> {
    let fixed = program.header.fixed_accounts as usize;
    let stride = program.header.batch_stride as usize;
    if reference & ROW_BIT == 0 {
        return ((reference as usize) < fixed).then(|| program.accounts[reference as usize].flags);
    }
    let offset = (reference & !ROW_BIT) as usize;
    (site == Site::Foreach && offset < stride).then(|| program.accounts[fixed + offset].flags)
}

fn invoke(program: &Program, pc: usize, site: Site, ceiling: &mut Ceiling) -> Result<(), Violation> {
    let index = program.instrs[pc].a as usize;
    let Some(cpi) = program.cpis.get(index) else {
        return fail("ceiling.descriptor", pc, format!("descriptor {index} of {}", program.cpis.len()));
    };
    ceiling.invoke_sites += 1;
    match declared(program, cpi.program, site) {
        Some(flags) if flags & EXECUTABLE != 0 => {}
        Some(flags) => {
            return fail("ceiling.program-not-executable", pc, format!("program slot {:#x} declares {flags:#x}", cpi.program))
        }
        None => return fail("ceiling.undeclared-program", pc, format!("program slot {:#x} at {site:?}", cpi.program)),
    }
    if cpi.group != NONE && cpi.group >= program.header.account_groups {
        return fail("ceiling.undeclared-group", pc, format!("group {} of {}", cpi.group, program.header.account_groups));
    }
    let start = cpi.account_start as usize;
    let end = start + cpi.account_len as usize;
    if end > program.cpi_accounts.len() || cpi.account_len > 64 {
        return fail("ceiling.record-range", pc, format!("records {start}..{end} of {}", program.cpi_accounts.len()));
    }
    for (offset, record) in program.cpi_accounts[start..end].iter().enumerate() {
        ceiling.records += 1;
        if record.flags & !(SIGNER | WRITABLE) != 0 {
            return fail("ceiling.record-flags", pc, format!("record {} flags {:#x}", start + offset, record.flags));
        }
        let Some(flags) = declared(program, record.account, site) else {
            return fail(
                "ceiling.undeclared-account",
                pc,
                format!("record {} names slot {:#x} at {site:?}", start + offset, record.account),
            );
        };
        if record.flags & !flags != 0 {
            return fail(
                "ceiling.privilege",
                pc,
                format!(
                    "record {} passes slot {:#x} with {:#x}, declared {flags:#x}",
                    start + offset,
                    record.account,
                    record.flags
                ),
            );
        }
    }
    Ok(())
}

/// The most a callee can hold on each address, for one assignment of addresses to the slots of an
/// invoke: `slots[i]` is the address the caller put in the `i`th listed record's slot, with that
/// slot's declared flags, and `group` the members of the forwarded group with the transaction's
/// writable flag for each. Returns each address's merged privileges, as the runtime merges an
/// account listed twice in one instruction.
///
/// The bound: an address is a signer only if some slot holding it is declared signer, and
/// writable only if some slot holding it is declared writable or it is a group member the
/// transaction marked writable.
pub fn address_ceiling(slots: &[(u32, u8, u8)], group: &[(u32, bool)]) -> Vec<(u32, u8, u8)> {
    // (address, privileges the callee gets, privileges the declarations and the group allow)
    let mut merged: Vec<(u32, u8, u8)> = Vec::new();
    let mut add = |address: u32, granted: u8, allowed: u8| match merged.iter_mut().find(|(a, _, _)| *a == address) {
        Some(entry) => {
            entry.1 |= granted;
            entry.2 |= allowed;
        }
        None => merged.push((address, granted, allowed)),
    };
    for &(address, record_flags, declared_flags) in slots {
        add(address, record_flags, declared_flags & (SIGNER | WRITABLE));
    }
    for &(address, writable) in group {
        let flag = if writable { WRITABLE } else { 0 };
        add(address, flag, flag);
    }
    merged
}
