//! Raw bytes as a payload, a template account and instruction data: nothing panics, and the run
//! path's fast parse and the independent model split a payload exactly as `ProgramView::parse`
//! does. See `ballista_fuzz_support::harness::parse`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| ballista_fuzz_support::harness::parse(data));
