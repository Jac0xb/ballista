//! `RETURN_DATA` reads only data the program the last CPI invoked returned.
//!
//! The runtime keeps one return-data slot per transaction, and any program a CPI reaches can set
//! it, including programs that program calls in turn. Ballista records the program each successful
//! `INVOKE` called and refuses return data whose setter is another program
//! (`ReturnDataMismatch`), or that is missing.
//!
//! The syscall is the mock in `crate::mocks`: any setter, any size, and data whose first 32 bytes
//! are the setter's address. The rules read a pubkey at offset 0, so a successful read leaves the
//! setter's address in the destination register; a refused read leaves the register as it was.
//! They observe the register's payload, which the executor writes with eight-byte stores, rather
//! than the `RunResult`, whose error tag the prover cannot follow (see `rules::registry`).
//!
//! The program a rule says was invoked is set with `Scratch::set_last_invoked_for_spec`, a
//! `spec-api` hook, instead of running an `INVOKE`: a successful `invoke_cpi` stores exactly that
//! address, and modelling the CPI would add nothing the property depends on.

use ballista::processor::execute::{execute_instruction, RuntimeValue, Scratch};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{heap_views, AccountSlot};

use super::symbolic::{
    self, address_words, empty, payload_words, u64_registers, InstructionSlot, Shape,
};
use super::util::REGISTERS;

/// Runs `RETURN_DATA`, reading a pubkey at offset 0 into register 1, after a CPI that invoked
/// `invoked` (or no CPI), and returns register 1's payload before and after.
fn read_pubkey_return_data(invoked: bool) -> ([u64; 4], [u64; 4], [u64; 4]) {
    let program = symbolic::program(Shape {
        registers: REGISTERS,
        ..Shape::accounts(1)
    });
    let callee = AccountSlot::<0>::nondet();
    let views = heap_views([callee.view()]);
    let callee_address = address_words(views[0].address().as_array());
    let registers = u64_registers();
    let before = payload_words(&registers[1]);

    let mut scratch = Scratch::new(&program);
    if invoked {
        scratch.set_last_invoked_for_spec(views[0].address());
    }
    let mut record = core::mem::MaybeUninit::uninit();
    let instruction =
        InstructionSlot::write(&mut record, OP_RETURN_DATA, 1, OP_READ_PUBKEY, NO_INDEX, NO_INDEX, 0, 0);
    let inputs: &[RuntimeValue] = empty();
    let _ = execute_instruction(&program, inputs, &views[..], registers, &mut scratch, instruction, None);
    (before, payload_words(&registers[1]), callee_address)
}

/// After a CPI to a program, a return-data read either is refused or returns that program's data:
/// the destination ends up holding the invoked program's address (the mock's setter) or is left as
/// it was. Data another program set is never read.
#[rule]
pub fn rule_return_data_comes_from_the_invoked_program() {
    let (before, after, invoked) = read_pubkey_return_data(true);
    clog!(before[0], after[0], invoked[0]);
    cvlr_assert!(after == before || after == invoked);
}

/// Reachability: a read can succeed, and then changes the destination. A satisfy rule, which passes
/// when the prover finds such a run.
#[rule]
pub fn rule_return_data_from_the_invoked_program_is_read() {
    let (before, after, invoked) = read_pubkey_return_data(true);
    cvlr_satisfy!(after != before && after == invoked);
}

/// Reachability: a read can be refused, when another program set the data or it is too short.
#[rule]
pub fn rule_return_data_reaches_a_refusal() {
    let (before, after, _) = read_pubkey_return_data(true);
    cvlr_satisfy!(after == before);
}

/// Twin that must fail: it claims a successful read never returns the invoked program's data.
#[rule]
pub fn rule_return_data_twin_reads_another_program() {
    let (before, after, invoked) = read_pubkey_return_data(true);
    cvlr_assert!(after == before || after != invoked);
}

/// With no successful CPI in the run, a return-data read is always refused.
#[rule]
pub fn rule_return_data_needs_an_invoke() {
    let (before, after, _) = read_pubkey_return_data(false);
    cvlr_assert!(after == before);
}

/// Twin that must fail: it claims a read is refused even after a CPI.
#[rule]
pub fn rule_return_data_twin_refuses_after_an_invoke() {
    let (before, after, _) = read_pubkey_return_data(true);
    cvlr_assert!(after == before);
}
