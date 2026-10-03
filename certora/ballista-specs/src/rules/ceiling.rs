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
//! The rule here proves link 1, the one a template's author controls, for any declarations, any
//! record and any descriptor. Forwarded account groups are outside it: their members carry the
//! transaction's writable flag and are never signers, by construction in `invoke_cpi`.

use ballista_common::template::*;
use cvlr::nondet::havoc::alloc_mut_ref_havoced;
use cvlr::prelude::*;

use super::symbolic::{self, InstructionSlot, Shape};
use super::util::REGISTERS;

/// What the verifier decided about an unconditional `INVOKE` of descriptor 0, in a program with two
/// fixed accounts, one descriptor and one account record, all havoced.
struct Invoke {
    accepted: bool,
    /// Whether the descriptor lists the record.
    listed: bool,
    record_flags: u8,
    /// The declaration of the account the record names, as the verifier looks it up at the root.
    declared_flags: Option<u8>,
    /// Whether the invoked program's declaration requires it to be executable.
    program_executable: bool,
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
    let declared_flags = program
        .account_constraint(account_record.account, false)
        .map(|constraint| constraint.flags);
    let program_executable = program
        .account_constraint(descriptor.program_account, false)
        .is_some_and(|constraint| constraint.flags & ACCOUNT_EXECUTABLE != 0);
    clog!(accepted, listed, account_record.flags, account_record.account);
    Invoke {
        accepted,
        listed,
        record_flags: account_record.flags,
        declared_flags,
        program_executable,
    }
}

/// If finalization accepts an `INVOKE`, every account its CPI lists names a fixed account, asks for
/// nothing but signer and writable, and asks for no privilege that account's declaration does not
/// require; and the invoked program is declared executable.
#[rule]
pub fn rule_cpi_requests_stay_within_declared_privileges() {
    let invoke = verify_invoke();
    if invoke.accepted {
        cvlr_assert!(invoke.program_executable);
        if invoke.listed {
            cvlr_assert!(invoke.record_flags & !(ACCOUNT_SIGNER | ACCOUNT_WRITABLE) == 0);
            match invoke.declared_flags {
                Some(declared) => cvlr_assert!(invoke.record_flags & !declared == 0),
                None => cvlr_assert!(false),
            }
        }
    }
}

/// Reachability of both asserting branches: an accepted invoke that lists the record.
#[rule]
pub fn rule_cpi_requests_reach_an_accepted_listed_account() {
    let invoke = verify_invoke();
    cvlr_satisfy!(invoke.accepted && invoke.listed);
}

/// Reachability of a privileged request: an accepted invoke whose record asks for signer.
#[rule]
pub fn rule_cpi_requests_reach_a_signer_request() {
    let invoke = verify_invoke();
    cvlr_satisfy!(invoke.accepted && invoke.listed && invoke.record_flags & ACCOUNT_SIGNER != 0);
}

/// Twin that must fail: it claims no accepted invoke asks for a signer, which a declared signer
/// passed as one refutes.
#[rule]
pub fn rule_cpi_requests_twin_never_ask_for_a_signer() {
    let invoke = verify_invoke();
    if invoke.accepted && invoke.listed {
        cvlr_assert!(invoke.record_flags & ACCOUNT_SIGNER == 0);
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
}
