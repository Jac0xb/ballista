//! The typing rules' property, checked by exhaustive enumeration on the host instead of the
//! prover. Brought in from the critic's branch (`claude/critic-formal`, d5ec4fe).
//!
//! `rules::typing::check_typing_preservation` states: for any register typing, any register values
//! consistent with it, and any instruction the verifier accepts against it, executing the
//! instruction returns success or a value-dependent error and leaves every register with the type
//! the verifier recorded. The rules are blocked on the prover's memory model. Their input space is
//! small and mostly structural, though: an opcode, four operand fields, flags, an immediate, a loop
//! scope, and a typing of four registers. This test enumerates that structure exhaustively (operand
//! fields up to register renaming, a fixed menu of immediates) and samples values, using the same
//! two spec programs and the same error split (`rules::oracle`) as the rules.
//!
//! It also reports which opcodes the rules can reach at all: an opcode the verifier never accepts
//! against the spec programs is outside the rules however long the prover runs. At d5ec4fe it found
//! the rules reach 55 of the 76 opcodes; never `INVOKE`, either loop, either PDA opcode,
//! `LOAD_INPUT`, `RETURN_DATA`, `EMIT`, `SET_RETURN_DATA`, the 9 introspection opcodes or the 3
//! registry opcodes. Its only violations were dynamic-offset reads past the data, which the first
//! oracle left out (`typing_oracle_gap.rs`). `SIGNATURES=1` prints each opcode's accepted operand
//! types.
//!
//! About 565M verifier calls; seconds in release on 8 threads, much longer in debug. Run:
//! `cargo test --release -p ballista-specs --features rt --test typing_enumeration -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::time::Instant;

use ballista::error::BallistaError;
use ballista::processor::execute::{execute_instruction, RunError, RuntimeValue, Scratch, NO_ROWS};
use ballista_common::template::*;
use ballista_specs::rules::oracle;
use ballista_specs::rules::util::{runtime_type, spec_program, writes_destination, REGISTERS};
use cvlr_pinocchio::{heap_views, AccountSlot};

/// Operand fields up to renaming of the four registers: each of dst, a, b, c is NO_INDEX, a
/// register already used, or the next unused register. Small numbers double as account, input,
/// pubkey and boolean operands, so the out-of-range value 4 is added too.
fn operand_patterns() -> Vec<[u8; 4]> {
    let mut out = Vec::new();
    fn go(pos: usize, used: u8, cur: &mut [u8; 4], out: &mut Vec<[u8; 4]>) {
        if pos == 4 {
            out.push(*cur);
            return;
        }
        for choice in [NO_INDEX, 4] {
            cur[pos] = choice;
            go(pos + 1, used, cur, out);
        }
        for register in 0..used {
            cur[pos] = register;
            go(pos + 1, used, cur, out);
        }
        if (used as usize) < REGISTERS {
            cur[pos] = used;
            go(pos + 1, used + 1, cur, out);
        }
    }
    go(0, 0, &mut [0; 4], &mut out);
    out
}

/// The typings the rule's `nondet_register_info` can produce, with `bytes` at both ends of its
/// length bound.
fn register_infos() -> Vec<Option<RegisterInfo>> {
    vec![
        None,
        Some(RegisterInfo::scalar(VALUE_BOOL)),
        Some(RegisterInfo::scalar(VALUE_U64)),
        Some(RegisterInfo::scalar(VALUE_I64)),
        Some(RegisterInfo::scalar(VALUE_U128)),
        Some(RegisterInfo::scalar(VALUE_PUBKEY)),
        Some(RegisterInfo::bytes(0)),
        Some(RegisterInfo::bytes(8)),
    ]
}

static BYTES: [u8; 8] = [0xff, 0, 1, 2, 3, 4, 5, 6];

/// The `sample`-th value of a type, rotating through boundary values.
fn sample_value(info: Option<RegisterInfo>, sample: usize) -> RuntimeValue<'static> {
    let Some(info) = info else {
        return RuntimeValue::Unset;
    };
    match info.value_type {
        VALUE_BOOL => RuntimeValue::Bool(sample % 2 == 1),
        VALUE_U64 => {
            let values = [0u64, 1, 2, 7, 63, 64, u64::MAX, 1 << 63, 10, 255];
            RuntimeValue::U64(values[sample % values.len()])
        }
        VALUE_I64 => {
            let values = [0i64, 1, -1, i64::MIN, i64::MAX, 63, 64, -64, 2];
            RuntimeValue::I64(values[sample % values.len()])
        }
        VALUE_U128 => {
            let values = [0u128, 1, u64::MAX as u128 + 1, u128::MAX, 1 << 127, 2];
            RuntimeValue::U128(values[sample % values.len()].to_le_bytes())
        }
        VALUE_PUBKEY => {
            let values = [[0u8; 32], [1; 32], [2; 32]];
            RuntimeValue::Pubkey(values[sample % values.len()])
        }
        _ => {
            let len = if info.bytes_max_len == 0 {
                0
            } else {
                sample % (info.bytes_max_len + 1)
            };
            RuntimeValue::Bytes(&BYTES[..len])
        }
    }
}

const SAMPLES: usize = 10;

/// Results of one shard of opcodes.
#[derive(Default)]
struct Tally {
    verify_calls: u64,
    executions: u64,
    violations: Vec<String>,
    violation_kinds: BTreeMap<String, (u64, String)>,
    accepted: BTreeMap<(bool, u8), u64>,
    frame_value_changes: u64,
    /// Per opcode: accepted (types of a, b, c when they name registers) -> recorded dst type.
    signatures: BTreeMap<u8, std::collections::BTreeSet<String>>,
}

fn run_shard(opcodes: Vec<u8>) -> Tally {
    let infos = register_infos();
    let patterns = operand_patterns();
    let immediates: [u64; 12] = [
        0,
        1,
        2,
        8,
        16,
        32,
        255,
        u64::MAX,
        range_immediate(0, 8),
        range_immediate(0, 16),
        range_immediate(0, 32),
        range_immediate(16, 16),
    ];
    let scopes = [LoopScope::Root, LoopScope::Rows, LoopScope::Count];
    // One account for the WITH_ACCOUNT program, as the rule passes; its header varies per sample.
    let slot: &'static mut AccountSlot<64> = AccountSlot::<64>::nondet();
    let slot_ptr: *mut AccountSlot<64> = slot;
    let views: &'static [pinocchio::AccountView; 1] =
        heap_views([unsafe { &mut *slot_ptr }.view()]);
    let mut tally = Tally::default();

    for with_account in [false, true] {
        let program = ProgramView::parse(spec_program(with_account)).expect("spec program parses");
        let accounts: &'static [pinocchio::AccountView] = if with_account {
            &views[..]
        } else {
            &views[..0]
        };
        for &opcode in &opcodes {
            for pattern in &patterns {
                let [dst, a, b, c] = *pattern;
                let mut named: Vec<usize> = Vec::new();
                for field in [dst, a, b, c] {
                    if (field as usize) < REGISTERS && !named.contains(&(field as usize)) {
                        named.push(field as usize);
                    }
                }
                let typings = infos.len().pow(named.len() as u32);
                for flags in [0u8, INSTRUCTION_FLAG_DYNAMIC_OFFSET] {
                    for &immediate in &immediates {
                        let instruction = record(opcode, dst, a, b, c, flags, immediate);
                        for &scope in &scopes {
                            for typing_index in 0..typings {
                                let mut typing = [None; MAX_REGISTERS];
                                let mut rest = typing_index;
                                for &register in &named {
                                    typing[register] = infos[rest % infos.len()];
                                    rest /= infos.len();
                                }
                                let before = typing;
                                tally.verify_calls += 1;
                                if program
                                    .verify_single_instruction(
                                        &instruction,
                                        0,
                                        scope,
                                        None,
                                        &mut typing,
                                    )
                                    .is_err()
                                {
                                    continue;
                                }
                                *tally.accepted.entry((with_account, opcode)).or_default() += 1;
                                {
                                    let name = |info: Option<RegisterInfo>| match info {
                                        None => "unset".to_string(),
                                        Some(info) => match info.value_type {
                                            VALUE_BOOL => "bool".into(),
                                            VALUE_U64 => "u64".into(),
                                            VALUE_I64 => "i64".into(),
                                            VALUE_U128 => "u128".into(),
                                            VALUE_PUBKEY => "pubkey".into(),
                                            _ => format!("bytes{}", info.bytes_max_len),
                                        },
                                    };
                                    let operand = |field: u8| {
                                        if (field as usize) < REGISTERS {
                                            name(before[field as usize])
                                        } else {
                                            "-".into()
                                        }
                                    };
                                    let result = if (dst as usize) < REGISTERS
                                        && writes_destination(opcode)
                                    {
                                        name(typing[dst as usize])
                                    } else {
                                        "none".into()
                                    };
                                    tally.signatures.entry(opcode).or_default().insert(format!(
                                        "({},{},{})->{}",
                                        operand(a),
                                        operand(b),
                                        operand(c),
                                        result
                                    ));
                                }
                                for sample in 0..SAMPLES {
                                    let mut registers: Vec<RuntimeValue<'static>> = (0..REGISTERS)
                                        .map(|register| {
                                            sample_value(before[register], sample + register)
                                        })
                                        .collect();
                                    let registers_before = registers.clone();
                                    {
                                        let slot = unsafe { &mut *slot_ptr };
                                        slot.header.data_len = [0u64, 8, 64][sample % 3];
                                        slot.header.is_writable = (sample % 2) as u8;
                                        slot.header.lamports = [0u64, 1, u64::MAX][sample % 3];
                                    }
                                    let loop_context = match scope {
                                        LoopScope::Root => None,
                                        LoopScope::Rows => Some((
                                            [0usize, MAX_RUNTIME_ACCOUNTS - 1][sample % 2],
                                            program.header.fixed_account_count(),
                                        )),
                                        LoopScope::Count => {
                                            Some(([0usize, 254][sample % 2], NO_ROWS))
                                        }
                                    };
                                    let mut scratch = Scratch::new(&program);
                                    let inputs: Vec<RuntimeValue<'static>> = Vec::new();
                                    tally.executions += 1;
                                    let outcome = execute_instruction(
                                        &program,
                                        &inputs,
                                        accounts,
                                        &mut registers,
                                        &mut scratch,
                                        &instruction,
                                        loop_context,
                                    );
                                    let mut problems: Vec<String> = Vec::new();
                                    match outcome {
                                        Ok(()) => {
                                            for register in 0..REGISTERS {
                                                let writes = writes_destination(opcode)
                                                    && register == dst as usize;
                                                let expected = typing[register];
                                                if runtime_type(registers[register])
                                                    != expected.map(|info| info.value_type)
                                                {
                                                    problems.push(format!("register {register} holds {:?}, verifier recorded {:?}", registers[register], expected));
                                                }
                                                if let (RuntimeValue::Bytes(value), Some(info)) =
                                                    (registers[register], expected)
                                                {
                                                    if value.len() > info.bytes_max_len {
                                                        problems.push(format!("register {register} holds {} bytes, bound {}", value.len(), info.bytes_max_len));
                                                    }
                                                }
                                                if !writes
                                                    && registers[register]
                                                        != registers_before[register]
                                                {
                                                    tally.frame_value_changes += 1;
                                                }
                                            }
                                            if writes_destination(opcode)
                                                && dst as usize >= REGISTERS
                                            {
                                                problems.push("accepted a destination outside the register file".into());
                                            }
                                        }
                                        Err(RunError::Vm(kind))
                                            if oracle::value_dependent(kind, &instruction) => {}
                                        Err(RunError::VmAt(kind, _))
                                            if oracle::may_fail_at_an_index(kind, &instruction) => {}
                                        Err(RunError::Program(_))
                                            if oracle::may_return_program_errors(&instruction) => {}
                                        Err(other) => {
                                            problems.push(format!("structural error {other:?}"))
                                        }
                                    }
                                    for why in problems {
                                        let kind =
                                            why.split(" holds ").next().unwrap_or(&why).to_string();
                                        let kind = if why.starts_with("structural") {
                                            why.clone()
                                        } else {
                                            kind
                                        };
                                        let key = format!("op={opcode} flags={flags} {kind}");
                                        let example = format!(
                                            "account={with_account} dst={dst} a={a} b={b} c={c} imm={immediate:#x} scope={scope:?} typing={:?} regs={:?}: {why}",
                                            &before[..REGISTERS], registers_before
                                        );
                                        let entry = tally
                                            .violation_kinds
                                            .entry(key)
                                            .or_insert((0, example));
                                        entry.0 += 1;
                                        if tally.violations.len() < 1 {
                                            tally.violations.push(why);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    tally
}

#[test]
#[ignore = "expensive: run in release, see the module comment"]
fn typing_preservation_by_enumeration() {
    let started = Instant::now();
    let threads = 8u8;
    let tallies: Vec<Tally> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|shard| {
                let opcodes: Vec<u8> = (0..=u8::MAX)
                    .filter(|opcode| opcode % threads == shard)
                    .collect();
                scope.spawn(move || run_shard(opcodes))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("shard"))
            .collect()
    });
    let elapsed = started.elapsed();
    let mut total = Tally::default();
    for tally in tallies {
        total.verify_calls += tally.verify_calls;
        total.executions += tally.executions;
        total.frame_value_changes += tally.frame_value_changes;
        total.violations.extend(tally.violations);
        for (key, (count, example)) in tally.violation_kinds {
            let entry = total.violation_kinds.entry(key).or_insert((0, example));
            entry.0 += count;
        }
        for (opcode, set) in tally.signatures {
            total.signatures.entry(opcode).or_default().extend(set);
        }
        for (key, count) in tally.accepted {
            *total.accepted.entry(key).or_default() += count;
        }
    }
    let reachable: Vec<String> = total
        .accepted
        .iter()
        .map(|((with_account, opcode), count)| {
            format!("{}{opcode}:{count}", if *with_account { "A" } else { "P" })
        })
        .collect();
    let distinct: std::collections::BTreeSet<u8> =
        total.accepted.keys().map(|(_, opcode)| *opcode).collect();
    println!("operand patterns: {}", operand_patterns().len());
    println!(
        "verify calls: {}, executions: {}, time: {elapsed:?}",
        total.verify_calls, total.executions
    );
    println!(
        "opcodes the rule can reach ({} of 76 defined): {:?}",
        distinct.len(),
        distinct
    );
    println!(
        "accepted cases per (program, opcode): {}",
        reachable.join(" ")
    );
    println!(
        "non-destination registers whose value changed on success: {}",
        total.frame_value_changes
    );
    if std::env::var("SIGNATURES").is_ok() {
        for (opcode, set) in &total.signatures {
            // Keep only tuples whose operands are all typed or not registers, to stay readable.
            let shown: Vec<&String> = set.iter().filter(|s| !s.contains("unset")).collect();
            println!(
                "SIG {opcode}: {}",
                shown
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
    for (key, (count, example)) in &total.violation_kinds {
        println!("VIOLATION x{count} [{key}] e.g. {example}");
    }
    let count: u64 = total.violation_kinds.values().map(|(count, _)| count).sum();
    assert!(count == 0, "{count} violating executions");
}
