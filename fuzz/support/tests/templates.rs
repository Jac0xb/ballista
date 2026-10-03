//! The fuzz harness against the repository's real templates, on stable:
//! `cargo test --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support`.
//!
//! - Every template passes every target's checks, with no note from the reference checker.
//! - Every negative mutation that applies to a template is refused by `verify` with its rule's
//!   error, and by the reference checker too, so neither side is vacuous.
//! - Every committed seed replays through its target.
//! - The generator reaches invokes and mostly verifies.

use std::{collections::BTreeMap, fs, path::PathBuf};

use arbitrary::Unstructured;
use ballista_fuzz_support::{
    ceiling,
    checker::{self, SIGNER, WRITABLE},
    gen, harness,
    model::Program,
    mutate::Input,
    negative, structured,
};

fn seeds(target: &str) -> Vec<(String, Vec<u8>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../seeds").join(target);
    let mut seeds: Vec<(String, Vec<u8>)> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}; run fuzz/scripts/seeds.py", directory.display()))
        .map(|entry| {
            let path = entry.expect("a directory entry").path();
            let name = path.file_name().expect("a file name").to_string_lossy().into_owned();
            (name, fs::read(&path).expect("a seed"))
        })
        .collect();
    seeds.sort();
    seeds
}

#[test]
fn every_template_verifies_and_passes_every_check() {
    let templates = seeds("verify");
    assert!(templates.len() >= 80, "the seeds hold the repository's templates");
    for (name, bytes) in templates {
        harness::parse(&bytes);
        assert!(harness::verify(&bytes).is_some(), "{name} verifies");
        let program = Program::decode(&bytes).expect("decodes");
        let found = ceiling::check(&program).unwrap_or_else(|violation| panic!("{name}: {violation}"));
        let report = checker::check(&program, bytes.len()).unwrap_or_else(|violation| panic!("{name}: {violation}"));
        assert_eq!(report.notes, vec![], "{name} is canonical");
        assert_eq!(found.worst_case_cpis, report.worst_case_cpis, "{name}");
        assert_eq!(harness::differential(&bytes), None, "{name}");
    }
}

#[test]
fn every_break_of_every_template_is_refused_with_its_error() {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (name, bytes) in seeds("verify") {
        let program = Program::decode(&bytes).expect("decodes");
        for broken in negative::breaks(&program) {
            if let Err(panic) = std::panic::catch_unwind(|| negative::require_rejected(&broken)) {
                let message = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| "a panic".into());
                panic!("{name}: {message}");
            }
            let encoded = broken.program.encode();
            assert!(
                checker::check(&broken.program, encoded.len()).is_err(),
                "{name}: the reference checker accepts the break {}",
                broken.rule
            );
            if broken.rule.starts_with("privilege")
                || broken.rule.starts_with("undeclared")
                || broken.rule.starts_with("cpi-count")
                || broken.rule == "record-flags"
            {
                assert!(ceiling::check(&broken.program).is_err(), "{name}: the ceiling pass accepts {}", broken.rule);
            }
            *counts.entry(broken.rule).or_default() += 1;
        }
    }
    eprintln!("breaks refused, by rule: {counts:?}");
    for rule in [
        "privilege-signer",
        "privilege-writable",
        "record-flags",
        "undeclared-account",
        "undeclared-program",
        "cpi-count-repeat",
        "cpi-count-foreach",
        "read-before-write",
        "type-mismatch",
        "emit-tag",
        "entry-writable",
        "guarded-return-data",
        "read-bounds",
        "sysvar-pin",
    ] {
        assert!(counts.contains_key(rule), "no template exercises the break {rule}");
    }
}

#[test]
fn every_seed_replays_through_its_target() {
    for (_, bytes) in seeds("parse") {
        harness::parse(&bytes);
    }
    for (_, bytes) in seeds("verify") {
        harness::verify(&bytes);
    }
    for (_, bytes) in seeds("differential") {
        assert_eq!(harness::differential(&bytes), None);
    }
    for (_, bytes) in seeds("structured") {
        structured::run(&bytes);
    }
}

/// SplitMix64: a fixed stream of bytes per seed.
fn stream(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            (z ^ (z >> 31)) as u8
        })
        .collect()
}

#[test]
fn the_generator_reaches_invokes_and_mostly_verifies() {
    let (mut built, mut accepted, mut invoking, mut looping_invokes) = (0, 0, 0, 0);
    for seed in 0..1_500 {
        let bytes = stream(seed, 700);
        let mut unstructured = Unstructured::new(&bytes);
        let Some(program) = gen::program(&mut Input(&mut unstructured)) else { continue };
        built += 1;
        if harness::verify(&program).is_none() {
            continue;
        }
        accepted += 1;
        assert!(harness::differential(&program).map_or(true, |note| !harness::is_fatal(note.rule)));
        let decoded = Program::decode(&program).expect("decodes");
        let sites = ceiling::invoke_sites(&decoded);
        invoking += usize::from(!sites.is_empty());
        looping_invokes += usize::from(sites.iter().any(|(_, site)| *site != ceiling::Site::Root));
        // And the structured target end to end, from the same stream.
        let mut input = vec![1u8];
        input.extend_from_slice(&bytes);
        structured::run(&input);
    }
    eprintln!("built {built}, verified {accepted}, with an invoke {invoking}, with one in a loop {looping_invokes}");
    assert!(accepted * 2 >= built, "at least half the generated programs verify");
    assert!(invoking * 3 >= accepted, "a third of the verified ones invoke");
    assert!(looping_invokes * 10 >= accepted, "a tenth invoke inside a loop");
}

/// The ceiling per address: an address can be writable to a callee through a read-only slot's
/// alias in a writable slot, or through a forwarded group the transaction marked writable, but a
/// signer only through a slot declared signer.
#[test]
fn per_address_the_ceiling_is_every_slot_holding_it_plus_the_group() {
    // Address 1: a read-only slot, passed read-only, and a member of the forwarded group that the
    // transaction marked writable. The callee gets it writable, which only the group allows.
    assert_eq!(ceiling::address_ceiling(&[(1, 0, 0)], &[(1, true)]), vec![(1, WRITABLE, WRITABLE)]);
    // Address 2 in a read-only slot and, aliased, in a writable one passed writable.
    assert_eq!(
        ceiling::address_ceiling(&[(2, 0, 0), (2, WRITABLE, WRITABLE)], &[]),
        vec![(2, WRITABLE, WRITABLE)]
    );
    // A group never makes a signer, whatever the transaction says of the account.
    assert_eq!(ceiling::address_ceiling(&[(3, 0, 0)], &[(3, true)]), vec![(3, WRITABLE, WRITABLE)]);
    assert_eq!(ceiling::address_ceiling(&[(4, SIGNER, SIGNER)], &[]), vec![(4, SIGNER, SIGNER)]);
    // Whatever the aliasing, what a callee gets stays within what the slots and the group allow.
    for (address, granted, allowed) in
        ceiling::address_ceiling(&[(5, SIGNER, SIGNER | WRITABLE), (5, WRITABLE, WRITABLE), (6, 0, SIGNER)], &[(6, false)])
    {
        assert_eq!(granted & !allowed, 0, "address {address}");
    }
}

/// The known findings as the `differential` target found them, minimized with `cargo fuzz tmin`
/// under `BALLISTA_FUZZ_STRICT=<rule>`. Each file is named for its rule.
fn reproducers() -> Vec<(String, Vec<u8>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../regressions/differential");
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(directory)
        .expect("fuzz/regressions/differential")
        .map(|entry| {
            let path = entry.expect("an entry").path();
            (path.file_name().unwrap().to_string_lossy().into_owned(), fs::read(&path).unwrap())
        })
        .collect();
    files.sort();
    files
}

#[test]
fn each_reproducer_breaks_the_rule_it_is_named_for() {
    let files = reproducers();
    assert_eq!(files.len(), harness::KNOWN_FINDINGS.len(), "one reproducer per known finding");
    for (rule, bytes) in files {
        assert!(harness::KNOWN_FINDINGS.contains(&rule.as_str()), "{rule} is a known finding");
        let program = Program::decode(&bytes).expect("decodes");
        let report = checker::check(&program, bytes.len()).expect("only an encoding rule is broken");
        assert!(report.notes.iter().any(|note| note.rule == rule), "{rule}: {:?}", report.notes);
    }
}

#[test]
#[ignore = "known findings: verify accepts every reproducer; see common/tests/fuzz_findings.rs"]
fn verify_refuses_each_reproducer() {
    for (rule, bytes) in reproducers() {
        let verdict = ballista_common::template::ProgramView::parse(&bytes).and_then(|view| view.verify());
        assert!(verdict.is_err(), "{rule}: verify accepts it");
    }
}
