#!/usr/bin/env python3
"""Critic tooling: apply one named executor mutant to this checkout (restore with git checkout).

Usage: scripts/critic/mutate2.py NAME, or `--list` for every name. `scripts/critic/run-mutant2.sh
NAME` applies it, rebuilds, runs the Mollusk, protocol and host suites (or just the executor fuzz
loop), and restores the source. Results of the second pass: only `loop-restore-moved-to-exit`
survives every suite; `critic_loops.rs` kills the four restore mutants.
"""
import pathlib
import sys
W = str(pathlib.Path(__file__).resolve().parents[2]) + '/'
EX = W + 'programs/ballista/src/processor/execute.rs'
MUTANTS = {
    # Hunt: restore non-carried registers only after the last pass, not between passes.
    'loop-restore-between-passes': (EX,
        "    if active.restore {\n        machine.registers.copy_from_slice(snapshot);\n    }",
        "    if active.restore && active.pass + 1 >= active.passes {\n        machine.registers.copy_from_slice(snapshot);\n    }"),
    # Hunt, CU-neutral form: the same semantics, nested so loops that skip the restore pay nothing.
    'loop-restore-at-exit-only': (EX,
        "    if active.restore {\n        machine.registers.copy_from_slice(snapshot);\n    }",
        "    if active.restore {\n        if active.pass + 1 >= active.passes {\n            machine.registers.copy_from_slice(snapshot);\n        }\n    }"),
    # Hunt, cheaper form: restore once, when the loop ends; no per-pass work added.
    'loop-restore-moved-to-exit': (EX,
        "    if active.restore {\n        machine.registers.copy_from_slice(snapshot);\n    }\n    // Neither add wraps: a loop makes at most 255 passes, a FOREACH's row base stays below the\n    // runtime account count, and a REPEAT's stays `NO_ROWS`, since its stride is zero.\n    active.pass = active.pass.wrapping_add(1);\n    if active.pass < active.passes {\n        // Row `n` starts at `fixed + n * stride`; stepping by the stride avoids a checked\n        // multiplication, which SBF implements with a 128-bit multiply routine of about fifty\n        // instructions. A REPEAT's stride is zero.\n        active.row_base = active.row_base.wrapping_add(active.stride);\n        return Some((active.pass, active.row_base));\n    }\n    finish_loop(machine);\n    None\n}",
        "    // Neither add wraps: a loop makes at most 255 passes, a FOREACH's row base stays below the\n    // runtime account count, and a REPEAT's stays `NO_ROWS`, since its stride is zero.\n    active.pass = active.pass.wrapping_add(1);\n    if active.pass < active.passes {\n        // Row `n` starts at `fixed + n * stride`; stepping by the stride avoids a checked\n        // multiplication, which SBF implements with a 128-bit multiply routine of about fifty\n        // instructions. A REPEAT's stride is zero.\n        active.row_base = active.row_base.wrapping_add(active.stride);\n        return Some((active.pass, active.row_base));\n    }\n    if active.restore {\n        machine.registers.copy_from_slice(snapshot);\n    }\n    finish_loop(machine);\n    None\n}"),
    # Hunt: the restore shortcut fires only when every body record writes a non-carried register.
    'restore-shortcut-all': (EX,
        "    let restore = program.instructions[body_start..body_end].iter().any(|record| {",
        "    let restore = program.instructions[body_start..body_end].iter().all(|record| {"),
    # Hunt: only the lowest carried register flows between passes and out of the loop.
    'carry-first-only': (EX,
        "    for &register in &active.carried[..active.carried_len] {",
        "    for &register in &active.carried[..active.carried_len.min(1)] {"),
    # Claim (FV1): no restore at all is killed by a_loop_restores_the_registers_it_does_not_carry.
    'loop-restore-none': (EX,
        "    if active.restore {\n        machine.registers.copy_from_slice(snapshot);",
        "    if false && active.restore {\n        machine.registers.copy_from_slice(snapshot);"),
    # Claim (FV1): only a protocol test kills the deleted provenance check.
    'return-provenance': (EX,
        "    if &program_id != expected_program.as_array() {\n        return Err(BallistaError::ReturnDataMismatch.into());",
        "    if false && &program_id != expected_program.as_array() {\n        return Err(BallistaError::ReturnDataMismatch.into());"),
    # Value mutants for FV3's reference-model oracle.
    'min-max-swap': (EX,
        "                OP_MIN => Some($left.min($right)),\n                OP_MAX => Some($left.max($right)),",
        "                OP_MIN => Some($left.max($right)),\n                OP_MAX => Some($left.min($right)),"),
    'exec-add-wraps': (EX,
        "                OP_ADD => $left.checked_add($right),",
        "                OP_ADD => Some($left.wrapping_add($right)),"),
    # Value mutant: a u16 data segment (CPI data, outputs, seeds) encoded big-endian.
    'cpi-u16-big-endian': (EX,
        "            let value = u16::try_from(as_u128(get(registers, segment.register)?)?)\n                .map_err(|_| BallistaError::ArithmeticOverflow)?;\n            sink.push_bytes(&value.to_le_bytes())",
        "            let value = u16::try_from(as_u128(get(registers, segment.register)?)?)\n                .map_err(|_| BallistaError::ArithmeticOverflow)?;\n            sink.push_bytes(&value.to_be_bytes())"),
    # Structural mutant: invoke_cpi refuses data exactly as long as the descriptor's maximum (6016).
    'cpi-data-len-ge': (EX,
        "        if scratch.data.len() > max_data_len || scratch.data.len() > MAX_CPI_DATA_LEN {",
        "        if scratch.data.len() >= max_data_len || scratch.data.len() > MAX_CPI_DATA_LEN {"),
    # Privilege mutant: a CPI asks for every account the template may sign with as writable too,
    # so Ballista itself requests a privilege the transaction did not grant (PrivilegeEscalation).
    'cpi-signer-implies-writable': (EX,
        "                    record.flags & ACCOUNT_WRITABLE != 0,\n                    record.flags & ACCOUNT_SIGNER != 0,\n                ));",
        "                    record.flags & (ACCOUNT_WRITABLE | ACCOUNT_SIGNER) != 0,\n                    record.flags & ACCOUNT_SIGNER != 0,\n                ));"),
    # Flag mutant: a CPI never passes a signature on, which only the callee's view of its flags
    # shows when the call succeeds anyway.
    'cpi-signer-dropped': (EX,
        "                    record.flags & ACCOUNT_WRITABLE != 0,\n                    record.flags & ACCOUNT_SIGNER != 0,\n                ));",
        "                    record.flags & ACCOUNT_WRITABLE != 0,\n                    false,\n                ));"),
    # Limits mutant: a batch's repeat pass reuses the first row's program even when the program is
    # a row account.
    'row-program-cached': (EX,
        "            program: (program_reference & ITERATION_ACCOUNT_BIT == 0).then_some(program_account),",
        "            program: Some(program_account),"),
}
name = sys.argv[1]
if name == '--list':
    print('\n'.join(MUTANTS))
    sys.exit(0)
path, old, new = MUTANTS[name]
text = open(path).read()
assert text.count(old) == 1, f'{name}: snippet found {text.count(old)} times'
open(path, 'w').write(text.replace(old, new))
print(f'applied {name} to {path}')
