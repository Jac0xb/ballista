//! Critic (test methodology, second pass): the loop-register guard P49 still lacks.
//!
//! P49 says each pass starts from the registers as they were before the loop, plus the carried
//! values. The guard the property review added (`a_loop_restores_the_registers_it_does_not_carry`)
//! checks only the registers after the loop, with a one-instruction body. These executor mutants
//! pass it and every other semantic test in the Mollusk, protocol and host suites:
//!
//! - `next_pass` restoring once, when the loop ends, instead of after every pass
//!   (`loop-restore-moved-to-exit`): it also makes runs cheaper, so no compute-unit ceiling fails
//!   and it survives every suite;
//! - the same, gated per pass (`loop-restore-between-passes`): only the compute-unit ceilings fail;
//! - the restore shortcut computed with `all` instead of `any` (`restore-shortcut-all`): only the
//!   protocol suite's ninth-row compute boundary fails.
//!
//! The mutants are in the critic's `mutate2.py`. This test passes today and fails under each.

use super::*;
use ballista_common::template::{record, DATA_REG_U64, OP_CONST_U64};

/// The body reads `x` before overwriting it without carrying it. Every pass must see the pre-loop
/// value: three passes of `total += x + 1` with `x == 5` return 18, and `x` is 5 after the loop.
#[test]
fn every_pass_starts_from_the_registers_before_the_loop() {
    let mut builder = ProgramBuilder::new();
    let x = builder.const_u64(5);
    let one = builder.const_u64(1);
    let total = builder.const_u64(0);
    let count = builder.const_u64(3);
    builder.repeat(count, 3, 1 << total, |body| {
        let seen = body.binary(OP_ADD, x, one);
        // Overwrite `x`, which the loop does not carry.
        body.emit(record(OP_CONST_U64, x, NO_INDEX, NO_INDEX, NO_INDEX, 0, 7));
        let sum = body.binary(OP_ADD, total, seen);
        body.mov(total, sum);
    });
    let five = builder.const_u64(5);
    let restored = builder.binary(OP_EQ, x, five);
    builder.require(restored);
    builder.set_return_data(&[Segment::Register(DATA_REG_U64, total)]);
    let payload = builder.build().expect("builds");
    ProgramView::parse(&payload)
        .and_then(|program| program.verify())
        .expect("a body may read, then overwrite, a register it does not carry");

    let creator = Pubkey::new_unique();
    let context = context(funded_accounts([creator], 10_000_000_000));
    let created = context.process_instruction(&create_template_instruction(creator, 9, &payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    let template = find_template_pda(&creator, 9).0;
    let run = context.process_instruction(&run_instruction(template, vec![], &[]));
    assert!(run.program_result.is_ok(), "{run:#?}");
    assert_eq!(
        run.return_data,
        18u64.to_le_bytes(),
        "each of the three passes must read x == 5; 22 means a pass saw the previous pass's x"
    );
}
