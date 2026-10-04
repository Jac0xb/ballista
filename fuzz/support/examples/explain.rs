//! Prints a payload the way the harness sees it, for triage:
//!
//!     cargo run --release --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support \
//!         --example explain -- [--structured] FILE
//!
//! The decoded sections, `verify`'s verdict, the CPI ceiling pass, and the reference checker.

use std::{env, fs};

use ballista_common::template::ProgramView;
use ballista_fuzz_support::{ceiling, checker, model::Program, structured};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let structured_input = args.iter().any(|arg| arg == "--structured");
    let path = args.iter().find(|arg| !arg.starts_with("--")).expect("FILE");
    let data = fs::read(path).expect("a file");
    let bytes = if structured_input { structured::build(&data).expect("builds").0 } else { data };
    println!("{} bytes", bytes.len());
    match Program::decode(&bytes) {
        Ok(program) => {
            let h = &program.header;
            println!(
                "header: fixed {} stride {} rows {}..={} inputs {}+{} registers {} groups {} flags {:#x}",
                h.fixed_accounts, h.batch_stride, h.min_rows, h.max_rows, h.fixed_inputs, h.row_inputs, h.registers,
                h.account_groups, h.flags
            );
            for (index, account) in program.accounts.iter().enumerate() {
                println!("  account {index}: {account:?}");
            }
            for (index, input) in program.inputs.iter().enumerate() {
                println!("  input {index}: {input:?}");
            }
            for (pc, instr) in program.instrs.iter().enumerate() {
                println!(
                    "  {pc:3}: op {:2} dst {:3} a {:3} b {:3} c {:3} flags {} imm {:#x}{}",
                    instr.op, instr.dst, instr.a, instr.b, instr.c, instr.flags, instr.imm,
                    if instr.reserved != [0; 2] { format!(" reserved {:?}", instr.reserved) } else { String::new() }
                );
            }
            for (index, cpi) in program.cpis.iter().enumerate() {
                println!("  cpi {index}: {cpi:?}");
            }
            for (index, record) in program.cpi_accounts.iter().enumerate() {
                println!("  cpi account {index}: {record:?}");
            }
            for (index, segment) in program.segments.iter().enumerate() {
                println!("  segment {index}: {segment:?}");
            }
            println!("  {} pubkeys, blob {} bytes", program.pubkeys.len(), program.blob.len());
            println!("verify:  {:?}", ProgramView::parse(&bytes).and_then(|view| view.verify()));
            println!("ceiling: {:?}", ceiling::check(&program));
            println!("checker: {:?}", checker::check(&program, bytes.len()));
        }
        Err(error) => println!("does not decode: {error:?}"),
    }
}
