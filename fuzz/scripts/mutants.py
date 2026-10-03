#!/usr/bin/env python3
"""Mutation score for the verifier fuzz harness: does it notice a deleted verifier rule?

    python3 fuzz/scripts/mutants.py [SECONDS] [MUTANT ...]

For each mutant, in a throwaway `git worktree` of HEAD (this checkout is never touched): delete one
rule from common/src/template/verify.rs, then

  1. run the support crate's stable tests (`cargo test -p ballista-fuzz-support`), which replay
     every template, every negative mutation of each, and every seed;
  2. run the `structured` fuzz target from an empty corpus for up to SECONDS (default 120), and
     record how long it takes to crash.

A mutant that survives both is a rule the harness does not test; the script then exits 1 (and 2
when a mutant does not build), so CI can run it nightly. `verify-cpi-privilege` is the critic's
mutant (scripts/critic/mutate.py on branch claude/critic-tests); the existing proptests in
common/tests pass under it. Needs cargo-fuzz and a nightly toolchain, as fuzz/scripts/run.sh does.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VERIFY = "common/src/template/verify.rs"

# name: (snippet in verify.rs, replacement). Each snippet must appear exactly once.
MUTANTS = {
    # The CPI privilege ceiling: a record's flags within its slot's declaration.
    "verify-cpi-privilege": (
        "            if cpi_account.flags & !constraint.flags != 0 {\n                return Err(TemplateError::InvalidCpi(index));",
        "            if false && cpi_account.flags & !constraint.flags != 0 {\n                return Err(TemplateError::InvalidCpi(index));",
    ),
    # A record's flags are signer and writable only.
    "verify-cpi-record-flags": (
        "            if cpi_account.flags & !(ACCOUNT_SIGNER | ACCOUNT_WRITABLE) != 0 {",
        "            if false && cpi_account.flags & !(ACCOUNT_SIGNER | ACCOUNT_WRITABLE) != 0 {",
    ),
    # A record names a declared slot.
    "verify-cpi-record-declared": (
        "                .account_constraint(cpi_account.account, in_row_loop)\n                .ok_or(TemplateError::InvalidCpi(index))?;",
        "                .account_constraint(cpi_account.account, in_row_loop)\n                .unwrap_or(&AccountConstraint { flags: 3, address_index: 255, owner_index: 255, reserved: 0, min_data_len_le: [0; 4] });",
    ),
    # A CPI forwards a declared account group, if any.
    "verify-cpi-group": (
        "            if group >= self.header.account_group_count() {",
        "            if false && group >= self.header.account_group_count() {",
    ),
    # The program a CPI calls is declared executable.
    "verify-cpi-program-executable": (
        "        if program.flags & ACCOUNT_EXECUTABLE == 0 {\n            return Err(TemplateError::InvalidCpi(index));",
        "        if false && program.flags & ACCOUNT_EXECUTABLE == 0 {\n            return Err(TemplateError::InvalidCpi(index));",
    ),
    # The worst-case CPI count is at most 64.
    "verify-cpi-count": (
        "        if root_cpis > MAX_EXPANDED_CPIS {",
        "        if false && root_cpis > MAX_EXPANDED_CPIS {",
    ),
    # A loop body's invokes count once per pass.
    "verify-loop-cpi-passes": (
        "                        body_cpis\n                            .checked_mul(max_passes)",
        "                        body_cpis\n                            .checked_mul(1)",
    ),
    # A register is written before it is read.
    "verify-read-before-write": (
        "        registers[index as usize].ok_or(TemplateError::RegisterNotInitialized(index))",
        "        Ok(registers[index as usize].unwrap_or(RegisterInfo::scalar(VALUE_BOOL)))",
    ),
    # A fixed-offset read stays inside the declared minimum length.
    "verify-read-bounds": (
        "        if end > constraint.min_data_len() {",
        "        if false && end > constraint.min_data_len() {",
    ),
    # Return data is read only after an unguarded invoke.
    "verify-return-data-guard": (
        ".is_some_and(|record| record.opcode == OP_INVOKE && record.b == NO_INDEX);",
        ".is_some_and(|record| record.opcode == OP_INVOKE);",
    ),
    # An EMIT's tag does not start with the run event's family.
    "verify-emit-tag": (
        "if tag.len() >= MIN_EMIT_TAG_LEN && !tag.starts_with(&RUN_EVENT_TAG_FAMILY) =>",
        "if tag.len() >= MIN_EMIT_TAG_LEN =>",
    ),
    # No CPI lists a registry entry writable.
    "verify-registry-entry-writable": (
        ".any(|meta| meta.account == instruction.a && meta.flags & ACCOUNT_WRITABLE != 0)",
        ".any(|meta| false && meta.account == instruction.a && meta.flags & ACCOUNT_WRITABLE != 0)",
    ),
    # Introspection reads only the Instructions sysvar.
    "verify-introspection-pin": (
        ".is_some_and(|address| address.bytes == INSTRUCTIONS_SYSVAR_ID);",
        ".is_some_and(|_| true);",
    ),
    # Return data is set before no invoke.
    "verify-return-data-last": (
        ".any(|record| matches!(record.opcode, OP_INVOKE | OP_SET_RETURN_DATA))",
        ".any(|record| matches!(record.opcode, OP_SET_RETURN_DATA))",
    ),
}


def run(command, cwd, log, timeout=None, env=None):
    with open(log, "w") as out:
        try:
            return subprocess.run(command, cwd=cwd, stdout=out, stderr=subprocess.STDOUT, timeout=timeout, env=env).returncode
        except subprocess.TimeoutExpired:
            return None


def main():
    args = sys.argv[1:]
    seconds = int(args.pop(0)) if args and args[0].isdigit() else 120
    names = args or list(MUTANTS)
    toolchain = os.environ.get("FUZZ_TOOLCHAIN", "nightly")
    scratch = Path(os.environ.get("MUTANTS_DIR") or tempfile.mkdtemp(prefix="ballista-mutants-"))
    tree = scratch / "tree"
    logs = scratch / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "worktree", "add", "--detach", str(tree), "HEAD"], cwd=ROOT, check=True, capture_output=True)
    rows = []
    try:
        source = (tree / VERIFY).read_text()
        for name in names:
            old, new = MUTANTS[name]
            assert source.count(old) == 1, f"{name}: the snippet appears {source.count(old)} times"
            (tree / VERIFY).write_text(source.replace(old, new))
            built = run(["cargo", f"+{toolchain}", "fuzz", "build", "-s", "none", "structured"], tree, logs / f"{name}.build.log")
            if built != 0:
                rows.append((name, "does not build", "-"))
                continue
            tests = run(["cargo", "test", "--release", "-p", "ballista-fuzz-support"], tree / "fuzz", logs / f"{name}.tests.log")
            failed = re.findall(r"^test (\S+) \.\.\. FAILED", (logs / f"{name}.tests.log").read_text(), re.M)
            corpus = scratch / "corpus" / name
            shutil.rmtree(corpus, ignore_errors=True)
            corpus.mkdir(parents=True)
            binary = tree / "fuzz/target/aarch64-apple-darwin/release/structured"
            if not binary.exists():
                binary = next((tree / "fuzz/target").glob("*/release/structured"))
            start = time.time()
            fuzzed = run(
                [str(binary), str(corpus), f"-max_total_time={seconds}", "-max_len=11000", "-timeout=5",
                 f"-artifact_prefix={scratch}/{name}-"],
                tree, logs / f"{name}.fuzz.log", timeout=seconds + 60,
            )
            elapsed = time.time() - start
            log = (logs / f"{name}.fuzz.log").read_text()
            lines = log.splitlines()
            panic = next(
                (lines[i + 1] for i, line in enumerate(lines[:-1]) if "panicked at" in line),
                "",
            )
            found = f"crash in {elapsed:.0f} s: {panic[:90]}" if fuzzed not in (0, None) else f"survived {seconds} s"
            rows.append((name, f"{len(failed)} failing" if tests != 0 else "all pass", found))
            print(f"{name:32} tests: {rows[-1][1]:12} fuzz: {found}", flush=True)
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", str(tree)], cwd=ROOT, capture_output=True)
    print()
    print(f"{'mutant':32} {'support tests':14} structured target")
    for name, tests, found in rows:
        print(f"{name:32} {tests:14} {found}")
    print(f"logs: {logs}")
    survivors = [name for name, tests, found in rows if tests == "all pass" and found.startswith("survived")]
    if any(tests == "does not build" for _, tests, _ in rows):
        return 2
    if survivors:
        print(f"survived every check: {', '.join(survivors)}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
