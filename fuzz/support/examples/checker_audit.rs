//! Where the reference checker and `verify` disagree, in both directions, over a corpus:
//!
//!     cargo run --release --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support \
//!         --example checker_audit -- TARGET DIRECTORY...
//!
//! The differential target fails on one direction: `verify` accepts what the checker refuses. The
//! other direction, `verify` refusing what the checker accepts, names a verifier rule the checker
//! lacks. A rule both lack is one neither side can catch, so this lists them by the verifier's
//! error, with an example input.

use std::{collections::BTreeMap, env, fs, path::PathBuf};

use ballista_common::template::ProgramView;
use ballista_fuzz_support::{checker, model::Program, structured};

fn main() {
    let mut args = env::args().skip(1);
    let target = args.next().expect("TARGET DIRECTORY...");
    let mut agree = BTreeMap::<&str, usize>::new();
    let mut checker_only = BTreeMap::<String, (usize, PathBuf)>::new();
    let mut verify_only = BTreeMap::<String, (usize, PathBuf)>::new();
    for directory in args {
        for entry in fs::read_dir(&directory).expect("a corpus directory") {
            let path = entry.expect("an entry").path();
            let data = fs::read(&path).expect("a file");
            let bytes = if target == "structured" {
                match structured::build(&data) {
                    Some((bytes, _)) => bytes,
                    None => continue,
                }
            } else {
                data
            };
            let Ok(model) = Program::decode(&bytes) else { continue };
            let verdict = ProgramView::parse(&bytes).and_then(|view| view.verify());
            let reference = checker::check(&model, bytes.len());
            match (verdict, reference) {
                (Ok(_), Ok(report)) if report.notes.is_empty() => *agree.entry("both accept").or_default() += 1,
                (Ok(_), Ok(report)) => {
                    // Known findings: the checker notes an encoding rule `verify` lets through.
                    let slot = verify_only.entry(report.notes[0].rule.to_string()).or_insert((0, path.clone()));
                    slot.0 += 1;
                }
                (Err(_), Err(_)) => *agree.entry("both refuse").or_default() += 1,
                (Err(_), Ok(report)) if !report.notes.is_empty() => {
                    *agree.entry("both refuse, the checker by a note").or_default() += 1
                }
                (Err(error), Ok(_)) => {
                    let name = format!("{error:?}").split('(').next().unwrap_or_default().to_string();
                    let slot = checker_only.entry(name).or_insert((0, path.clone()));
                    slot.0 += 1;
                }
                (Ok(_), Err(violation)) => {
                    let slot = verify_only.entry(violation.rule.to_string()).or_insert((0, path.clone()));
                    slot.0 += 1;
                }
            }
        }
    }
    println!("{agree:?}");
    println!("verify refuses, the checker accepts (a verifier rule the checker lacks):");
    for (error, (count, example)) in &checker_only {
        println!("  {error:28} {count:6}  e.g. {}", example.display());
    }
    println!("verify accepts, the checker refuses (the differential target's findings):");
    for (rule, (count, example)) in &verify_only {
        println!("  {rule:28} {count:6}  e.g. {}", example.display());
    }
}
