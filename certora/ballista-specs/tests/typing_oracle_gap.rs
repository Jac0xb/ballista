//! The case that showed the typing rules' first error oracle was wrong, kept as a regression test
//! for the split in `rules::oracle`.
//!
//! The verifier accepts a dynamic-offset `READ_U64` from the account spec program's one account
//! (offset in a `u64` register, immediate zero). `docs/reference/language.md` says such a read
//! "fails the run if it extends past the end of the data", and the executor fails it with
//! `InvalidRuntimeAccount`. That depends only on the offset's value and the account's data length,
//! so it is value-dependent. The first oracle left it out, so
//! `rule_verified_account_reads_preserve_register_typing` had a real counterexample that its
//! "blocked" label hid. Found by the critic's host enumeration (`typing_enumeration.rs`).
//!
//! Run: `cargo test -p ballista-specs --features rt --test typing_oracle_gap`

use ballista::error::BallistaError;
use ballista::processor::execute::{execute_instruction, RunError, RuntimeValue, Scratch};
use ballista_common::template::*;
use ballista_specs::rules::oracle;
use ballista_specs::rules::util::spec_program;
use cvlr_pinocchio::{heap_views, AccountSlot};

#[test]
fn a_dynamic_offset_read_past_the_data_is_a_value_error() {
    // The rule's account program and typing: register 0 holds a u64, the rest are unset.
    let program = ProgramView::parse(spec_program(true)).expect("spec program parses");
    let mut typing = [None; MAX_REGISTERS];
    typing[0] = Some(RegisterInfo::scalar(VALUE_U64));
    let read = record(OP_READ_U64, 0, 0, 0, NO_INDEX, INSTRUCTION_FLAG_DYNAMIC_OFFSET, 0);
    assert!(
        program
            .verify_single_instruction(&read, 0, LoopScope::Root, None, &mut typing)
            .is_ok(),
        "the verifier accepts the read, so the rule's assumption holds"
    );

    // An account with no data, which the rule's nondeterministic account may be.
    let slot = AccountSlot::<64>::nondet();
    slot.header.data_len = 0;
    let views = heap_views([slot.view()]);
    let mut registers = vec![
        RuntimeValue::U64(0),
        RuntimeValue::Unset,
        RuntimeValue::Unset,
        RuntimeValue::Unset,
    ];
    let mut scratch = Scratch::new(&program);
    let inputs: Vec<RuntimeValue> = Vec::with_capacity(1);
    let outcome =
        execute_instruction(&program, &inputs, &views[..], &mut registers, &mut scratch, &read, None);
    assert_eq!(outcome, Err(RunError::Vm(BallistaError::InvalidRuntimeAccount)));

    // What the typing rules now assert for this outcome.
    assert!(oracle::value_dependent(BallistaError::InvalidRuntimeAccount, &read));
}
