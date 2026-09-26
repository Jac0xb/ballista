//! Generated programs verify by construction. Run with `--features proptest`.

use ballista_common::template::{
    generate::any_program, ProgramView, TemplateAccount, TemplateAccountHeader,
};
use proptest::prelude::*;

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
