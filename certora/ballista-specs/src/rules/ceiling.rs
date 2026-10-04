//! The privilege ceiling at CPI time, as finalization enforces it.
//!
//! The trust model says a declaration is a ceiling: a call can pass a declared account as a signer
//! or as writable only if its declaration requires that privilege and the transaction granted it.
//! Three links make that true for every account a CPI descriptor lists:
//!
//! 1. Finalization (`verify_cpi`): each account record asks for at most signer and writable, and
//!    only for privileges its account's declaration requires; the account is a fixed account (at
//!    the root) and the invoked program is declared executable.
//! 2. The run's account validation: an account whose declaration requires signer or writable is a
//!    signer or writable in the transaction (`rules::accounts`, blocked).
//! 3. `invoke_cpi` copies each record's two flags into the account meta it passes, unchanged:
//!    `InstructionAccount::new(address, flags & WRITABLE != 0, flags & SIGNER != 0)`. The prover
//!    cannot read those metas back (the two flags are copied as part of an eight-byte word built
//!    from two one-byte stack stores), so this link stands on inspection and the Mollusk suite.
//!
//! The rule here states link 1, the one a template's author controls, for any declarations, any
//! record and any descriptor. Forwarded account groups are outside it: their members carry the
//! transaction's writable flag and are never signers, by construction in `invoke_cpi`.
//!
//! `verify_cpi` returns `Result<usize, TemplateError>`, whose tag is one byte (`Ok` is the niche
//! value 0x21), copied out of a stack temporary with one eight-byte move. That copy was the suspected
//! blocker, and it held: in job `aac2bacd98254d8bae289bcc83067eef` the verifier refused with tag 0xf
//! (`InvalidCpi`, a nonzero `reserved1` byte) and the rule saw `accepted = 0`. The rule failed on
//! its own types instead. `Invoke` was then six bytes, returned in one register that LLVM packed
//! with shifts and ORs, and the prover's integer encoding of OR let the solver set the `accepted`
//! byte of the packed word ("Imprecision detected: BWOr(...)" in the trace). `Invoke` is now whole
//! words returned through memory, and no assert ANDs two unknowns.

use ballista_common::template::*;
use cvlr::nondet::havoc::alloc_mut_ref_havoced;
use cvlr::prelude::*;

use super::symbolic::{self, InstructionSlot, Shape};
use super::util::REGISTERS;

/// What the verifier decided about an unconditional `INVOKE` of descriptor 0, in a program with two
/// fixed accounts, one descriptor and one account record, all havoced.
///
/// Whole eight-byte words, so the struct is returned through memory and each field is stored and
/// loaded at one width. Flags hold 0 or 1.
#[repr(C)]
struct Invoke {
    accepted: u64,
    /// Whether the descriptor lists the record.
    listed: u64,
    record_flags: u64,
    /// Whether the account the record names has a declaration, as the verifier looks it up at the
    /// root, and that declaration's flags (0 without one).
    declared: u64,
    declared_flags: u64,
    /// Whether the invoked program's declaration requires it to be executable.
    program_executable: u64,
}

fn verify_invoke() -> Invoke {
    let program = symbolic::program(Shape {
        fixed_accounts: 2,
        pubkeys: 0,
        registers: REGISTERS,
        cpis: 1,
        cpi_accounts: 1,
    });
    // The verifier reads the typing only for a condition register or a register segment, and this
    // invoke has neither: `b` is NO_INDEX and the program has no data segments.
    let typing = alloc_mut_ref_havoced::<[Option<RegisterInfo>; MAX_REGISTERS]>();
    let mut record = core::mem::MaybeUninit::uninit();
    let invoke = InstructionSlot::write(&mut record, OP_INVOKE, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, 0);
    let accepted = program
        .verify_single_instruction(invoke, 0, LoopScope::Root, None, typing)
        .is_ok();

    let descriptor = &program.cpis[0];
    let account_record = &program.cpi_accounts[0];
    let start = descriptor.account_start();
    let listed = start == 0 && descriptor.account_len >= 1;
    let declared = program.account_constraint(account_record.account, false);
    let program_executable = program
        .account_constraint(descriptor.program_account, false)
        .is_some_and(|constraint| constraint.flags & ACCOUNT_EXECUTABLE != 0);
    clog!(accepted, listed, account_record.flags, account_record.account);
    Invoke {
        accepted: u64::from(accepted),
        listed: u64::from(listed),
        record_flags: u64::from(account_record.flags),
        declared: u64::from(declared.is_some()),
        declared_flags: declared.map_or(0, |constraint| u64::from(constraint.flags)),
        program_executable: u64::from(program_executable),
    }
}

const SIGNER: u64 = ACCOUNT_SIGNER as u64;
const WRITABLE: u64 = ACCOUNT_WRITABLE as u64;

/// If finalization accepts an `INVOKE`, every account its CPI lists names a fixed account, asks for
/// nothing but signer and writable, and asks for no privilege that account's declaration does not
/// require; and the invoked program is declared executable.
///
/// "No privilege the declaration does not require" is one assert per privilege, each a test of one
/// constant bit: `record_flags & !declared_flags == 0` would AND two unknowns, which the prover
/// models imprecisely. With the assert before it, the two say the same.
#[rule]
pub fn rule_cpi_requests_stay_within_declared_privileges() {
    let invoke = verify_invoke();
    if invoke.accepted != 0 {
        cvlr_assert!(invoke.program_executable != 0);
        if invoke.listed != 0 {
            cvlr_assert!(invoke.record_flags & !(SIGNER | WRITABLE) == 0);
            cvlr_assert!(invoke.declared != 0);
            if invoke.record_flags & SIGNER != 0 {
                cvlr_assert!(invoke.declared_flags & SIGNER != 0);
            }
            if invoke.record_flags & WRITABLE != 0 {
                cvlr_assert!(invoke.declared_flags & WRITABLE != 0);
            }
        }
    }
}

/// Reachability of both asserting branches: an accepted invoke that lists the record.
#[rule]
pub fn rule_cpi_requests_reach_an_accepted_listed_account() {
    let invoke = verify_invoke();
    cvlr_satisfy!(invoke.accepted != 0 && invoke.listed != 0);
}

/// Reachability of a privileged request: an accepted invoke whose record asks for signer.
#[rule]
pub fn rule_cpi_requests_reach_a_signer_request() {
    let invoke = verify_invoke();
    cvlr_satisfy!(invoke.accepted != 0 && invoke.listed != 0 && invoke.record_flags & SIGNER != 0);
}

/// Twin that must fail: it claims no accepted invoke asks for a signer, which a declared signer
/// passed as one refutes.
#[rule]
pub fn rule_cpi_requests_twin_never_ask_for_a_signer() {
    let invoke = verify_invoke();
    if invoke.accepted != 0 && invoke.listed != 0 {
        cvlr_assert!(invoke.record_flags & SIGNER == 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verifier's half of the ceiling on concrete programs: a request within the declaration is
    /// accepted, and one past it is refused.
    #[test]
    fn requests_past_the_declaration_are_refused() {
        let build = |declared: u8, requested: u8| {
            let mut builder = ProgramBuilder::new();
            let account = builder.account(declared, None, None, 0);
            let callee = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
            let cpi = builder.cpi(callee, &[(account, requested)], &[]);
            builder.invoke(cpi, None);
            let bytes = builder.build().expect("builds");
            ProgramView::parse(&bytes).expect("parses").verify().is_ok()
        };
        assert!(build(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, ACCOUNT_SIGNER | ACCOUNT_WRITABLE));
        assert!(build(ACCOUNT_SIGNER, ACCOUNT_SIGNER));
        assert!(!build(ACCOUNT_WRITABLE, ACCOUNT_SIGNER));
        assert!(!build(0, ACCOUNT_WRITABLE));
    }

    /// The program in the counterexample of job `aac2bacd98254d8bae289bcc83067eef`: the trace took
    /// `verify_cpi_shape`'s first refusal, a descriptor whose `reserved1[0]` is 1. The verifier
    /// refuses it, as the trace's result tag (0xf, `InvalidCpi`) says, so the violation was the
    /// rule's packed `Invoke`, not an accepted request.
    #[test]
    fn the_counterexample_descriptor_is_refused() {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let callee = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
        let cpi = builder.cpi(callee, &[(account, ACCOUNT_SIGNER)], &[]);
        builder.invoke(cpi, None);
        let mut bytes = builder.build().expect("builds");
        let descriptor = {
            let view = ProgramView::parse(&bytes).expect("parses");
            assert!(view.verify().is_ok(), "the program before the change is accepted");
            view.cpis.as_ptr() as usize - bytes.as_ptr() as usize
        };
        let reserved1 = descriptor + core::mem::offset_of!(CpiDescriptor, reserved1);
        bytes[reserved1] = 1;
        let view = ProgramView::parse(&bytes).expect("still parses");
        assert_eq!(view.verify().err(), Some(TemplateError::InvalidCpi(0)));
    }

    /// The record `verify_invoke` writes passes `verify_single_instruction` on the host, for a
    /// program that lists a declared signer. At f1acfa1 the prover's front end dropped that path
    /// (`assume(r5 != r0)` after `verify_record_header`'s operand test), so `accepted` was the
    /// constant 0 and the slicer removed every assert in the ceiling rule and its twin.
    #[test]
    fn the_rules_invoke_record_is_accepted() {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(ACCOUNT_SIGNER, None, None, 0);
        let callee = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
        let cpi = builder.cpi(callee, &[(account, ACCOUNT_SIGNER)], &[]);
        builder.invoke(cpi, None);
        let bytes = builder.build().expect("builds");
        let view = ProgramView::parse(&bytes).expect("parses");
        let mut slot = core::mem::MaybeUninit::uninit();
        let record =
            InstructionSlot::write(&mut slot, OP_INVOKE, NO_INDEX, 0, NO_INDEX, NO_INDEX, 0, 0);
        let mut typing: [Option<RegisterInfo>; MAX_REGISTERS] = core::array::from_fn(|_| None);
        let verdict = view.verify_single_instruction(record, 0, LoopScope::Root, None, &mut typing);
        assert_eq!(verdict, Ok((1, 0)));
    }
}
