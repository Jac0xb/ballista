//! Generated programs verify by construction. Run with `--features proptest`.

use ballista_common::template::{
    generate::any_program, InstructionRecord, ProgramView, TemplateAccount, TemplateAccountHeader,
    OP_EMIT, OP_FOREACH, OP_OPEN_REGISTRY, OP_READ_REGISTRY, OP_REPEAT, OP_SET_RETURN_DATA,
    OP_WRITE_REGISTRY,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn generated_programs_parse_and_verify(program in any_program()) {
        let parsed = ProgramView::parse(&program.bytes).expect("generated program parses");
        let stats = parsed.verify().expect("generated program verifies");
        prop_assert_eq!(stats.batch_stride as usize, program.row_accounts);
        prop_assert_eq!(stats.batch_max_iterations as usize, program.max_iterations);
        prop_assert_eq!(parsed.header.batch_min_iterations(), program.min_iterations);
        prop_assert_eq!(stats.fixed_accounts as usize, program.fixed_accounts);
        prop_assert_eq!(parsed.header.row_input_count(), program.row_inputs);
        prop_assert_eq!(parsed.header.account_group_count(), program.account_groups);
        prop_assert_eq!(
            program.run_inputs(program.max_iterations, &vec![0; program.account_groups]).len(),
            program.account_groups + program.fixed_inputs.len() + program.max_iterations * program.row_input_bytes.len()
        );

        // The run path reads a finalized account without the full parse. It must find the same
        // program, and must not take an account that is still uploading.
        let mut header =
            TemplateAccountHeader::new_uploading([3; 32], 7, 254, program.bytes.len(), [9; 32])
                .expect("header");
        let mut account = header.as_bytes().to_vec();
        account.extend_from_slice(&program.bytes);
        prop_assert!(TemplateAccount::finalized_program_unchecked(&account).is_none());
        header.set_written_len(program.bytes.len()).expect("written");
        header.finalize().expect("finalize");
        account[..header.as_bytes().len()].copy_from_slice(header.as_bytes());
        let slow = TemplateAccount::parse(&account)
            .expect("account parses")
            .finalized_program()
            .expect("finalized program parses");
        let fast = TemplateAccount::finalized_program_unchecked(&account)
            .expect("the fast path reads a finalized account");
        prop_assert!(core::ptr::eq(slow.header, fast.header));
        prop_assert!(core::ptr::eq(slow.accounts, fast.accounts));
        prop_assert!(core::ptr::eq(slow.inputs, fast.inputs));
        prop_assert!(core::ptr::eq(slow.instructions, fast.instructions));
        prop_assert!(core::ptr::eq(slow.cpis, fast.cpis));
        prop_assert!(core::ptr::eq(slow.cpi_accounts, fast.cpi_accounts));
        prop_assert!(core::ptr::eq(slow.data_segments, fast.data_segments));
        prop_assert!(core::ptr::eq(slow.pubkeys, fast.pubkeys));
        prop_assert!(core::ptr::eq(slow.blob, fast.blob));
    }
}

/// Each loop of a program, in order: its opcode and its body.
fn loops(instructions: &[InstructionRecord]) -> Vec<(u8, &[InstructionRecord])> {
    let mut loops = Vec::new();
    let mut pc = 0;
    while pc < instructions.len() {
        let record = &instructions[pc];
        if matches!(record.opcode, OP_FOREACH | OP_REPEAT) {
            let end = pc + 1 + record.a as usize;
            loops.push((record.opcode, &instructions[pc + 1..end]));
            pc = end;
        } else {
            pc += 1;
        }
    }
    loops
}

/// The generator reaches what the loop, output and registry rules allow, so the properties cover
/// it: count loops, more than one loop, a count loop before a FOREACH, logs at the root and in loop
/// bodies, return data, and registry opens, field reads and writes, in loop bodies too.
#[test]
fn generated_programs_reach_every_loop_output_and_registry_shape() {
    let shapes: [(&str, usize, fn(&[InstructionRecord]) -> bool); 10] = [
        ("hold a count loop", 32, |program| {
            loops(program).iter().any(|(opcode, _)| *opcode == OP_REPEAT)
        }),
        ("hold more than one loop", 32, |program| loops(program).len() > 1),
        ("run a count loop before their first FOREACH", 8, |program| {
            let loops = loops(program);
            let first = |kind: u8| loops.iter().position(|(opcode, _)| *opcode == kind);
            matches!(
                (first(OP_REPEAT), first(OP_FOREACH)),
                (Some(repeat), Some(foreach)) if repeat < foreach
            )
        }),
        ("log", 32, |program| program.iter().any(|record| record.opcode == OP_EMIT)),
        ("log in a loop body", 16, |program| {
            loops(program)
                .iter()
                .any(|(_, body)| body.iter().any(|record| record.opcode == OP_EMIT))
        }),
        ("set return data", 96, |program| {
            program.iter().any(|record| record.opcode == OP_SET_RETURN_DATA)
        }),
        ("open a registry entry", 32, |program| {
            program.iter().any(|record| record.opcode == OP_OPEN_REGISTRY)
        }),
        ("read a registry field", 8, |program| {
            program.iter().any(|record| record.opcode == OP_READ_REGISTRY)
        }),
        ("write a registry field", 3, |program| {
            program.iter().any(|record| record.opcode == OP_WRITE_REGISTRY)
        }),
        ("read or write a registry field in a loop body", 3, |program| {
            loops(program).iter().any(|(_, body)| {
                body.iter()
                    .any(|record| matches!(record.opcode, OP_READ_REGISTRY | OP_WRITE_REGISTRY))
            })
        }),
    ];
    let mut runner = TestRunner::deterministic();
    let mut seen = [0usize; 10];
    for _ in 0..256 {
        let program = any_program().new_tree(&mut runner).unwrap().current();
        let parsed = ProgramView::parse(&program.bytes).unwrap();
        for (count, (_, _, reaches)) in seen.iter_mut().zip(&shapes) {
            *count += usize::from(reaches(parsed.instructions));
        }
    }
    let short: Vec<String> = seen
        .iter()
        .zip(&shapes)
        .filter(|(count, (_, floor, _))| *count < floor)
        .map(|(count, (what, floor, _))| format!("{count} of 256 programs {what}, under {floor}"))
        .collect();
    assert!(short.is_empty(), "{}", short.join("\n"));
}
