use std::str::FromStr;

use solana_program::{pubkey, pubkey::Pubkey};
use solana_program_test::ProgramTest;

// const JUPITER_V4_ADDRESS: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
const JUPITER_V4_PUBKEY: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const DEX1_PUBKEY: Pubkey = pubkey!("PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY");
// const DEX2_PUBKEY: Pubkey = pubkey!("8g4Z9d6PqGkgH31tMW6FwxGhwYJrXpxZHQrkikpLJKrG");
const DEXYosS6oEGvk8uCDayvwEZz4qEyDJRf9nFgYCaqPMTm: Pubkey =
    pubkey!("DEXYosS6oEGvk8uCDayvwEZz4qEyDJRf9nFgYCaqPMTm");

const OBRiQD1zbpyLz95G5n7nJe6a4DPjpFwa5XYPoNm113y: Pubkey =
    pubkey!("obriQD1zbpyLz95G5n7nJe6a4DPjpFwa5XYPoNm113y");

pub fn program_test() -> ProgramTest {
    let mut test = ProgramTest::new("ballista", ballista_sdk::ID, None);

    // test.add_program(
    //     "spl_account_compression",
    //     spl_account_compression::id(),
    //     None,
    // );
    // test.add_program("spl_noop", spl_noop::id(), None);
    // test.add_program("mpl_token_metadata", mpl_token_metadata::ID, None);
    // test.add_program("mpl_bubblegum", mpl_bubblegum::ID, None);

    test.set_compute_max_units(1_400_000);

    test.prefer_bpf(true);

    test
}
