//! The fuzz harness against the repository's real templates, on stable:
//! `cargo test --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support`.
//!
//! - Every template passes every target's checks, with no note from the reference checker.
//! - Every negative mutation that applies to a template is refused by `verify` with its rule's
//!   error, and by the reference checker too, so neither side is vacuous.
//! - Every committed seed replays through its target.
//! - The generator reaches invokes and mostly verifies.
//! - Every group break, of a template or a generated program, is refused by `verify` and by the
//!   reference checker with the same error at the same instruction.

use std::{collections::BTreeMap, fs, path::PathBuf};

use arbitrary::Unstructured;
use ballista_common::template::TemplateError;
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

/// Whether the reference checker flags `program`: a violation, or a broken encoding rule.
fn flagged(program: &Program) -> bool {
    let encoded = program.encode();
    checker::check(program, encoded.len()).map_or(true, |report| !report.notes.is_empty())
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
            assert!(flagged(&broken.program), "{name}: the reference checker accepts the break {}", broken.rule);
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
        "undeclared-group",
        "cpi-count-repeat",
        "cpi-count-foreach",
        "read-before-write",
        "type-mismatch",
        "emit-tag",
        "return-data-before-invoke",
        "return-data-in-loop",
        "entry-writable",
        "guarded-return-data",
        "read-bounds",
        "sysvar-pin",
        "ninth-loop",
        "ninth-open",
        "open-after-return-data",
        "open-after-invoke",
        "unused-operand",
        "unused-immediate",
        "cpi-segment-literal-register",
        "cpi-segment-register-fields",
        "unreferenced-segment",
        "uninvoked-descriptor",
    ]
    .into_iter()
    .chain(GROUP_BREAKS)
    {
        assert!(counts.contains_key(rule), "no template exercises the break {rule}");
    }
}

/// The breaks `negative.rs` makes of the group opcodes.
const GROUP_BREAKS: [&str; 19] = [
    "group-undeclared",
    "group-length-stray-b",
    "group-length-stray-c",
    "group-length-stray-immediate",
    "group-program-past-table",
    "group-second-program-past-table",
    "group-no-match",
    "group-five-matches",
    "group-five-excepts",
    "group-segments-past-table",
    "group-floor-short",
    "group-segment-length",
    "group-narrow-match",
    "group-except-offset",
    "group-except-kind",
    "group-match-type",
    "group-unset-register",
    "group-register-out-of-range",
    "group-spare-segment",
];

/// The reference checker refuses a group break with the rule that names `verify`'s error, at the
/// same instruction: a structural rule (`groups.*`, or `GROUP_LENGTH`'s unused operands) for
/// `InvalidAccountGroup`, the typing rules for a filter entry's register, and an unused segment
/// for `InvalidDataSegment`.
fn checker_agrees(broken: &negative::Break) {
    let encoded = broken.program.encode();
    let verdict = checker::check(&broken.program, encoded.len());
    let walk_rule = |pc: usize, rules: &dyn Fn(&str) -> bool| match &verdict {
        Err(violation) => assert!(
            rules(violation.rule) && violation.pc == Some(pc),
            "{}: verify says {:?}, the checker {violation}",
            broken.rule,
            broken.expected
        ),
        Ok(report) => panic!("{}: the checker accepts it, with notes {:?}", broken.rule, report.notes),
    };
    let first_group = broken.program.instrs.iter().position(|instr| {
        matches!(instr.op, checker::op::GROUP_LENGTH | checker::op::GROUP_ANY | checker::op::GROUP_COUNT)
    });
    match broken.expected {
        TemplateError::InvalidAccountGroup(pc) => walk_rule(pc, &|rule| {
            rule.starts_with("groups.") || rule == "format.group-length-operands"
        }),
        TemplateError::TypeMismatch | TemplateError::RegisterNotInitialized(_) | TemplateError::InvalidRegister(_) => {
            let rule = match broken.expected {
                TemplateError::TypeMismatch => "types.mismatch",
                TemplateError::RegisterNotInitialized(_) => "types.read-before-write",
                _ => "types.register-out-of-range",
            };
            let Err(violation) = &verdict else { panic!("{}: the checker accepts it", broken.rule) };
            assert_eq!(violation.rule, rule, "{}: {violation}", broken.rule);
            let at = violation.pc.expect("a typing rule names its instruction");
            assert!(first_group.is_some_and(|first| at >= first), "{}: {violation} before any group opcode", broken.rule);
            assert!(
                matches!(broken.program.instrs[at].op, checker::op::GROUP_ANY | checker::op::GROUP_COUNT),
                "{}: {violation} is not at a filter",
                broken.rule
            );
        }
        TemplateError::InvalidDataSegment(index) => {
            let report = verdict.unwrap_or_else(|violation| panic!("{}: {violation}", broken.rule));
            assert!(
                report.notes.iter().any(|note| {
                    note.rule == "unreferenced.segment" && note.detail.starts_with(&format!("segment {index}:"))
                }),
                "{}: {:?}",
                broken.rule,
                report.notes
            );
        }
        ref other => panic!("{}: no group break expects {other:?}", broken.rule),
    }
}

#[test]
fn group_breaks_are_refused_alike_by_verify_and_the_checker() {
    let mut programs: Vec<(String, Vec<u8>)> = seeds("verify");
    let mut generated_filters = 0;
    for seed in 0..1_500 {
        let bytes = stream(seed, 700);
        let mut unstructured = Unstructured::new(&bytes);
        let Some(program) = gen::program(&mut Input(&mut unstructured)) else { continue };
        if harness::verify(&program).is_none() {
            continue;
        }
        let decoded = Program::decode(&program).expect("decodes");
        if decoded.instrs.iter().any(|instr| matches!(instr.op, checker::op::GROUP_ANY | checker::op::GROUP_COUNT)) {
            generated_filters += 1;
        }
        programs.push((format!("generated {seed}"), program));
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (name, bytes) in programs {
        let program = Program::decode(&bytes).expect("decodes");
        for broken in negative::breaks(&program).into_iter().filter(|broken| broken.rule.starts_with("group-")) {
            if let Err(panic) = std::panic::catch_unwind(|| {
                negative::require_rejected(&broken);
                checker_agrees(&broken);
            }) {
                let message = panic.downcast_ref::<String>().cloned().unwrap_or_else(|| "a panic".into());
                panic!("{name}: {message}");
            }
            *counts.entry(broken.rule).or_default() += 1;
        }
    }
    eprintln!("verified generated programs with a group filter: {generated_filters}; group breaks: {counts:?}");
    assert!(generated_filters >= 50, "the generator reaches group filters");
    for rule in GROUP_BREAKS {
        assert!(counts.get(rule).is_some_and(|count| *count >= 10), "few programs exercise the break {rule}");
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

/// The findings the `differential` target made, minimized with `cargo fuzz tmin` under
/// `BALLISTA_FUZZ_STRICT=<rule>`, each file named for its rule. The verifier now refuses each
/// shape (`common/tests/fuzz_findings.rs`), so they stay as regressions.
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
    assert_eq!(files.len(), 4, "one reproducer per finding");
    for (rule, bytes) in files {
        assert!(!harness::KNOWN_FINDINGS.contains(&rule.as_str()), "{rule} is fixed, not known");
        let program = Program::decode(&bytes).expect("decodes");
        let report = checker::check(&program, bytes.len()).expect("only encoding rules are broken");
        assert!(report.notes.iter().any(|note| note.rule == rule), "{rule}: {:?}", report.notes);
    }
}

/// Fuzzed, then minimized for one rule, each also breaks others, such as an unused field, so the
/// verifier may refuse it at an earlier check than the rule it is named for.
#[test]
fn verify_refuses_each_reproducer() {
    for (rule, bytes) in reproducers() {
        let verdict = ballista_common::template::ProgramView::parse(&bytes).and_then(|view| view.verify());
        assert!(verdict.is_err(), "{rule}: verify accepts it");
        assert_eq!(harness::differential(&bytes), None, "{rule}");
    }
}
