//! Generated near-valid programs, mutated by structure, through `verify` and the reference
//! checker. See `ballista_fuzz_support::structured`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| ballista_fuzz_support::structured::run(data));
