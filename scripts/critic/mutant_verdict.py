#!/usr/bin/env python3
"""Summarizes one mutant's suite logs and decides whether the suites killed it.

    scripts/critic/mutant_verdict.py OUT_DIR SUITE...

OUT_DIR is run-mutant2.sh's directory for the mutant; it holds `<suite>.log` for each suite and
`summary.txt` (which already says if the build failed). Prints each suite's counts and failing
tests, then one `verdict:` line, and exits with:

  0  KILLED: some test other than a compute-unit ceiling failed;
  3  SURVIVED: every test passed, or the only failures were compute-unit ceilings
     (fixtures/cu-ceilings.json, fixtures/example-ceilings.json). A mutant that only costs more is
     a change detector, not a test of behaviour, so it counts as a survivor;
  2  ERROR: the mutant did not build, or a suite produced no test results.
"""
import pathlib
import re
import sys

# The panic messages of the two compute-unit ratchets: tests/ballista/src/ceilings.rs and
# check_example_ceilings in tests/ballista/src/benchmarks.rs.
CU_CEILING = re.compile(r"compute units regressed against fixtures/(cu-ceilings|example-ceilings)\.json")


def failures(log):
    """Failing test names, each with whether its output shows only a compute-unit ceiling."""
    names = re.findall(r"^test (\S+) \.\.\. FAILED$", log, re.M)
    result = []
    for name in names:
        # libtest prints each failure's captured output under `---- NAME stdout ----`.
        block = re.search(r"^---- " + re.escape(name) + r" stdout ----\n(.*?)(?=^---- |^failures:$)", log, re.M | re.S)
        text = block.group(1) if block else ""
        result.append((name, bool(CU_CEILING.search(text))))
    return result


def main():
    out = pathlib.Path(sys.argv[1])
    suites = sys.argv[2:]
    summary = (out / "summary.txt").read_text() if (out / "summary.txt").exists() else ""
    if "BUILD FAILED" in summary:
        print("verdict: ERROR (the mutant does not build)")
        return 2
    behaviour, ceilings, missing = [], [], []
    for suite in suites:
        path = out / f"{suite}.log"
        log = path.read_text(errors="replace") if path.exists() else ""
        results = re.findall(r"^test result: \w+\. (\d+) passed; (\d+) failed", log, re.M)
        if not results:
            missing.append(suite)
            print(f"{suite}: no test results")
            continue
        passed = sum(int(p) for p, _ in results)
        failed = sum(int(f) for _, f in results)
        print(f"{suite}: passed={passed} failed={failed}")
        for name, cu_only in failures(log):
            print(f"   {name}{'  (compute-unit ceiling only)' if cu_only else ''}")
            (ceilings if cu_only else behaviour).append(f"{suite}:{name}")
        # A test binary that aborted (a stack overflow, say) prints no result line of its own.
        for crash in re.findall(r"process didn't exit successfully: `([^`]+)` \((signal[^)]*)\)", log):
            print(f"   crashed: {crash[0].split('/')[-1]} ({crash[1]})")
            behaviour.append(f"{suite}:crash")
    if behaviour:
        print(f"verdict: KILLED by {', '.join(behaviour)}")
        return 0
    if missing:
        print(f"verdict: ERROR (no test results from {', '.join(missing)})")
        return 2
    if ceilings:
        print(f"verdict: SURVIVED (only compute-unit ceilings failed: {', '.join(ceilings)})")
    else:
        print("verdict: SURVIVED (every test passed)")
    return 3


if __name__ == "__main__":
    sys.exit(main())
