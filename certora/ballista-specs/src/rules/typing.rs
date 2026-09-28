//! The verifier-executor soundness property, one instruction at a time.
//!
//! The verifier tracks a static type per register. The executor keeps a runtime value per
//! register. The invariant that makes finalize-time verification meaningful is: if every register
//! holds a value of its recorded type and the verifier accepts an instruction against that typing,
//! then executing the instruction returns either success or a value-dependent error, never a
//! structural one, and the destination register ends up holding the type the verifier recorded.
//! Induction over the instruction sequence then gives the whole-program guarantee.

use ballista::error::BallistaError;
use ballista::processor::execute::{execute_instruction, RunError, RuntimeValue, Scratch, NO_ROWS};
use ballista_common::template::*;
use cvlr::nondet::havoc::alloc_mut_ref_havoced;
use cvlr::prelude::*;
use cvlr_pinocchio::nondet_account_views;
use pinocchio::AccountView;

use super::util::{
    nondet_instruction, nondet_register_info, pick, runtime_type, runtime_value_for, spec_program,
    writes_destination, REGISTERS,
};

/// Errors an accepted instruction may raise because of the values it sees, not its shape.
///
/// `READ_ACCOUNT_BYTES` reaches the executor in the account rule, whose one account is
/// unconstrained: the account may be writable, and the offset register may point past its data.
/// The introspection opcodes never do, since neither spec program pins the Instructions sysvar,
/// but an index past the transaction is value-dependent too.
fn value_dependent(kind: BallistaError) -> bool {
    matches!(
        kind,
        BallistaError::ArithmeticOverflow
            | BallistaError::DivisionByZero
            | BallistaError::RequirementFailed
            | BallistaError::InvalidPdaDerivation
            | BallistaError::InstructionOutOfRange
            | BallistaError::WritableAccountBytesRead
    )
}

/// One past the highest pass index a loop in `scope` reaches. A FOREACH makes one pass per batch
/// row, and the rows' accounts are runtime accounts, so it makes fewer than
/// `MAX_RUNTIME_ACCOUNTS`. A REPEAT makes at most its maximum, a byte, so its last pass is 254.
fn pass_bound(scope: LoopScope) -> usize {
    if scope.in_row_loop() {
        MAX_RUNTIME_ACCOUNTS
    } else {
        usize::from(u8::MAX) + 1
    }
}

fn check_typing_preservation(with_account: bool, accounts: &[AccountView]) {
    let program = ProgramView::parse(spec_program(with_account)).expect("spec program parses");

    // The verifier's register table lives on the heap so the prover can follow its symbolic
    // indexes. Only the first `REGISTERS` entries are initialized: the verifier bounds-checks every
    // register number against the declared count before touching the table, so the rest is never
    // read.
    let typing = alloc_mut_ref_havoced::<[Option<RegisterInfo>; MAX_REGISTERS]>();
    let mut registers: Vec<RuntimeValue> = Vec::with_capacity(REGISTERS);
    for register in 0..REGISTERS {
        let info = nondet_register_info();
        typing[register] = info;
        registers.push(runtime_value_for(info));
    }

    let instruction = nondet_instruction();
    let scope = pick!(LoopScope::Root, LoopScope::Rows, LoopScope::Count);
    clog!(instruction.opcode, instruction.dst, instruction.a, instruction.b, instruction.c);

    // Only instructions the verifier accepts are of interest.
    let verdict = program.verify_single_instruction(&instruction, 0, scope, None, typing);
    cvlr_assume!(verdict.is_ok());

    let mut scratch = Scratch::new(&program);
    // A FOREACH's rows start after the fixed accounts; a REPEAT has none.
    let loop_context = if scope.in_loop() {
        let pass: usize = nondet();
        cvlr_assume!(pass < pass_bound(scope));
        let row_base = if scope.in_row_loop() {
            program.header.fixed_account_count()
        } else {
            NO_ROWS
        };
        Some((pass, row_base))
    } else {
        None
    };
    // Heap-backed so the empty slice is not a dangling pointer the pointer analysis cannot classify.
    let inputs: Vec<RuntimeValue> = Vec::with_capacity(1);
    let outcome = execute_instruction(
        &program,
        &inputs,
        accounts,
        &mut registers,
        &mut scratch,
        &instruction,
        loop_context,
    );

    match outcome {
        Ok(()) => {
            if writes_destination(instruction.opcode) {
                let dst = instruction.dst as usize;
                cvlr_assert!(dst < REGISTERS);
                let expected = typing[dst].expect("verifier recorded a type for dst");
                cvlr_assert!(runtime_type(registers[dst]) == Some(expected.value_type));
                if let RuntimeValue::Bytes(value) = registers[dst] {
                    cvlr_assert!(value.len() <= expected.bytes_max_len);
                }
            }
            // Every other register is untouched, so the invariant still holds for it.
            for register in 0..REGISTERS {
                if register != instruction.dst as usize || !writes_destination(instruction.opcode) {
                    cvlr_assert!(
                        runtime_type(registers[register]) == typing[register].map(|info| info.value_type)
                    );
                }
            }
        }
        Err(RunError::Vm(kind)) => cvlr_assert!(value_dependent(kind)),
        Err(RunError::VmAt(_, _)) => cvlr_assert!(false),
        // Clock reads go through a sysvar syscall the prover may model as failing; nothing else
        // in the pure subset touches the runtime.
        Err(RunError::Program(_)) => {
            cvlr_assert!(matches!(instruction.opcode, OP_CLOCK_SLOT | OP_CLOCK_TIMESTAMP))
        }
    }
}

/// Pure instructions: constants, arithmetic, comparisons, booleans, select, casts, move, require,
/// loop index. Account, input, CPI, PDA, and return-data opcodes are rejected by the verifier
/// against this program and therefore fall outside the assumed set.
#[rule]
pub fn rule_verified_pure_instructions_preserve_register_typing() {
    // The pure program declares no accounts, so the verifier rejects every account reference and
    // this view is never reached. It exists so the account slice is heap memory rather than a
    // dangling empty-slice pointer, which the prover's pointer analysis cannot classify.
    let accounts = nondet_account_views::<1, 0>();
    check_typing_preservation(false, &accounts[..]);
}

/// The same property with one unconstrained runtime account, which admits the account header
/// reads: key, owner, lamports, data length, and emptiness.
#[rule]
pub fn rule_verified_account_reads_preserve_register_typing() {
    let accounts = nondet_account_views::<1, 64>();
    check_typing_preservation(true, &accounts[..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program with one loop of the most passes its kind allows, which must verify.
    fn verifies(build: impl FnOnce(&mut ProgramBuilder)) -> bool {
        let mut builder = ProgramBuilder::new();
        build(&mut builder);
        let bytes = builder.build().expect("builds");
        ProgramView::parse(&bytes).expect("parses").verify().is_ok()
    }

    /// The rule's pass index reaches the last pass of every loop the verifier accepts: a REPEAT
    /// at the largest maximum, and a FOREACH over the most rows a batch holds.
    #[test]
    fn the_pass_bound_covers_the_last_pass_of_every_verified_loop() {
        let longest_repeat = u8::MAX;
        assert!(verifies(|builder| {
            let count = builder.const_u64(u64::from(longest_repeat));
            builder.repeat(count, longest_repeat, 0, |body| {
                body.loop_index();
            });
        }));
        assert!(usize::from(longest_repeat) - 1 < pass_bound(LoopScope::Count));

        let most_rows = MAX_RUNTIME_ACCOUNTS as u8;
        assert!(verifies(|builder| {
            builder.row_account(0, None, None, 0);
            builder.batch(most_rows, 0);
            builder.for_each(0, |body| {
                body.loop_index();
            });
        }));
        assert!(!verifies(|builder| {
            builder.row_account(0, None, None, 0);
            builder.batch(most_rows + 1, 0);
            builder.for_each(0, |body| {
                body.loop_index();
            });
        }));
        assert!(usize::from(most_rows) - 1 < pass_bound(LoopScope::Rows));
    }
}
