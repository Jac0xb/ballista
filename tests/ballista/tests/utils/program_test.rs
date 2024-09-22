use solana_program_test::ProgramTest;

pub fn program_test() -> ProgramTest {
    let mut test = ProgramTest::new("ballista", ballista_sdk::ID, None);
    test.add_program(
        "spl_account_compression",
        spl_account_compression::id(),
        None,
    );
    test.add_program("spl_noop", spl_noop::id(), None);
    test.add_program("mpl_token_metadata", mpl_token_metadata::ID, None);
    test.add_program("mpl_bubblegum", mpl_bubblegum::ID, None);

    test.set_compute_max_units(1_400_000);

    test.prefer_bpf(true);

    test
}
