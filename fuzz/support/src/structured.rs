//! The `structured` target's input: a near-valid program, then structure-aware mutations.
//!
//! The first byte picks where the program comes from:
//!
//! - `0`: `GeneratedProgram::from_choices` in `ballista-common`, which verifies by construction;
//! - `1`: this crate's [`gen`](crate::gen), which reaches CPIs, PDAs, introspection, byte reads and
//!   several registry entries;
//! - `2`: a real template, as a little-endian `u16` length and that many bytes. The seed corpus
//!   holds every fixture this way.
//!
//! The bytes after that drive zero to four rounds of [`mutate`](crate::mutate). The result goes
//! through the `parse` and `verify` checks, then, if it verifies, the CPI ceiling pass and the
//! reference checker, and finally one [negative mutation](crate::negative): a single rule broken,
//! which `verify` must refuse with that rule's error.

use arbitrary::Unstructured;
use ballista_common::template::generate::GeneratedProgram;

use crate::{
    gen, harness,
    model::Program,
    mutate::{self, Input, Source},
};

/// Builds the program `data` describes, and a number that picks which rule to break if it
/// verifies. Public so tests can see what a corpus entry turns into.
pub fn build(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    let (&mode, rest) = data.split_first()?;
    let (base, rest) = match mode % 3 {
        2 => {
            let (len, rest) = rest.split_first_chunk::<2>()?;
            let len = (u16::from_le_bytes(*len) as usize).min(rest.len());
            let (template, rest) = rest.split_at(len);
            (Some(template.to_vec()), rest)
        }
        _ => (None, rest),
    };
    let mut unstructured = Unstructured::new(rest);
    let mut source = Input(&mut unstructured);
    let mut bytes = match (mode % 3, base) {
        (0, _) => {
            let count = source.below(96);
            let choices: Vec<u32> = (0..count).map(|_| source.word() as u32).collect();
            GeneratedProgram::from_choices(&choices).bytes
        }
        (1, _) => gen::program(&mut source)?,
        (_, base) => base?,
    };
    let rounds = source.below(5);
    if rounds > 0 {
        if let Ok(mut program) = Program::decode(&bytes) {
            for _ in 0..rounds {
                mutate::mutate(&mut program, &mut source);
            }
            bytes = program.encode();
        }
    }
    let choice = source.word() as usize;
    Some((bytes, choice))
}

pub fn run(data: &[u8]) {
    let Some((bytes, choice)) = build(data) else { return };
    harness::parse(&bytes);
    if harness::verify(&bytes).is_none() {
        return;
    }
    if let Some(violation) = harness::differential(&bytes) {
        harness::note_known(&violation);
    }
    // Then break one rule of the accepted program, which `verify` must refuse.
    harness::negative(&bytes, choice);
}
