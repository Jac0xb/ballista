//! Raw bytes as a payload, verified as `CreateTemplate` and `FinalizeTemplate` verify one: nothing
//! panics, both reach the same verdict, and an accepted payload runs. See
//! `ballista_fuzz_support::harness::verify`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    ballista_fuzz_support::harness::verify(data);
});
