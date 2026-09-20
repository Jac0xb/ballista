//! Generated programs verify by construction. Run with `--features proptest`.

use ballista_common::template::{generate::any_program, ProgramView};
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
    }
}
