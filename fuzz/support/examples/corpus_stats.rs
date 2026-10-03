//! What a corpus exercises on the CPI side, the part only finalization checks:
//!
//!     cargo run --release --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support \
//!         --example corpus_stats -- TARGET DIRECTORY...
//!
//! TARGET says how to read each file: `structured` builds it as that target does; any other name
//! reads it as a payload. Counts the accepted programs, their invoke sites by scope, the account
//! records the ceiling pass checked, and the worst-case CPI counts.

use std::{collections::BTreeMap, env, fs};

use ballista_fuzz_support::{
    ceiling::{self, Site},
    harness,
    model::Program,
    structured,
};

fn main() {
    let mut args = env::args().skip(1);
    let target = args.next().expect("TARGET DIRECTORY...");
    let (mut files, mut accepted, mut invoking, mut records) = (0usize, 0usize, 0usize, 0usize);
    let mut sites: BTreeMap<&str, usize> = BTreeMap::new();
    let mut worst: BTreeMap<usize, usize> = BTreeMap::new();
    let (mut groups, mut aliased, mut at_ceiling) = (0usize, 0usize, 0usize);
    for directory in args {
        for entry in fs::read_dir(&directory).expect("a corpus directory") {
            let data = fs::read(entry.expect("an entry").path()).expect("a file");
            files += 1;
            let bytes = if target == "structured" {
                match structured::build(&data) {
                    Some((bytes, _)) => bytes,
                    None => continue,
                }
            } else {
                data
            };
            if harness::verify(&bytes).is_none() {
                continue;
            }
            accepted += 1;
            let program = Program::decode(&bytes).expect("decodes");
            let found = ceiling::check(&program).expect("the ceiling holds for every accepted program");
            *worst.entry(found.worst_case_cpis).or_default() += 1;
            records += found.records;
            let invoke_sites = ceiling::invoke_sites(&program);
            invoking += usize::from(!invoke_sites.is_empty());
            for (pc, site) in &invoke_sites {
                let name = match site {
                    Site::Root => "root",
                    Site::Foreach => "FOREACH body",
                    Site::Repeat => "REPEAT body",
                };
                *sites.entry(name).or_default() += 1;
                let cpi = &program.cpis[program.instrs[*pc].a as usize];
                groups += usize::from(cpi.group != 0xff);
                let start = cpi.account_start as usize;
                let listed = &program.cpi_accounts[start..start + cpi.account_len as usize];
                aliased += usize::from(listed.iter().enumerate().any(|(i, a)| listed[..i].iter().any(|b| b.account == a.account)));
                // A record that uses every privilege its slot declares.
                at_ceiling += listed
                    .iter()
                    .filter(|record| {
                        ceiling::declared(&program, record.account, *site)
                            .is_some_and(|declared| record.flags != 0 && record.flags == declared & 3)
                    })
                    .count();
            }
        }
    }
    println!("{target}: {files} inputs, {accepted} verify, {invoking} of those invoke");
    println!("invoke sites: {sites:?}; account records checked: {records}");
    println!("invokes forwarding a group: {groups}; listing one slot twice: {aliased}; records at their slot's full privilege: {at_ceiling}");
    let top: Vec<_> = worst.iter().rev().take(6).collect();
    println!("worst-case CPI counts, highest first (count: programs): {top:?}");
}
