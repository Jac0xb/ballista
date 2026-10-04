//! Every payload `verify` accepts must pass the CPI ceiling pass and the independent reference
//! checker, both counting the worst case as `verify` does, and must be refused once one of its
//! rules is broken. Inputs are raw payloads, seeded with real templates; a custom mutator
//! edits them by structure two times in three, so most stay well-formed enough to verify. See
//! `ballista_fuzz_support::harness::differential`.
#![no_main]

use ballista_fuzz_support::{
    harness,
    model::Program,
    mutate::{self, Rng, Source},
};
use libfuzzer_sys::{fuzz_mutator, fuzz_target};

fuzz_target!(|data: &[u8]| {
    if let Some(violation) = harness::differential(data) {
        harness::note_known(&violation);
    }
    // An accepted payload also gets one rule broken, which `verify` must refuse. The bytes pick
    // which, so libFuzzer's coverage steers between the breaks too.
    if ballista_common::template::ProgramView::parse(data).and_then(|view| view.verify()).is_ok() {
        let choice = data.iter().fold(0usize, |hash, byte| hash.wrapping_mul(31).wrapping_add(*byte as usize));
        harness::negative(data, choice);
    }
});

fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    let mut rng = Rng::new(seed as u64);
    if rng.chance(1, 3) {
        return libfuzzer_sys::fuzzer_mutate(data, size, max_size);
    }
    let Ok(mut program) = Program::decode(&data[..size]) else {
        return libfuzzer_sys::fuzzer_mutate(data, size, max_size);
    };
    for _ in 0..1 + rng.below(3) {
        mutate::mutate(&mut program, &mut rng);
    }
    let bytes = program.encode();
    if bytes.len() > max_size {
        return libfuzzer_sys::fuzzer_mutate(data, size, max_size);
    }
    data[..bytes.len()].copy_from_slice(&bytes);
    bytes.len()
});
