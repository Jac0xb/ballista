#!/usr/bin/env python3
"""Checks a Certora Solana Prover log against what its conf promises.

    certora/check-results.py CONF LOG --expect verified   # run.conf and the candidate confs
    certora/check-results.py CONF LOG --expect violated   # the twin confs: every twin must fail
    certora/check-results.py CONF LOG --report-only       # run-blocked.conf: print, never fail

LOG is the output of `certoraSolanaProver CONF --wait_for_results all`. The prover's own exit
status cannot gate a twin conf, where failing is the point, and does not say which rules ran, so
this reads the per-rule lines the prover prints (format of certora-cli 8.19.2):

    Verified: <rule>-Assertions                  an assert rule held
    Verified: <rule>-rule_not_vacuous_cvlr       its sanity check (rule_sanity basic) found a path
    Violated: <rule>-Assertions                  a counterexample to an assert rule
    Verified: <rule> / Violated: <rule>          a satisfy rule reached, or not
    Error: Unknown error ... ruleIdentifier=<rule>, ...   the rule could not be encoded

and the "Results for all" table, where each violated row's second line says `(sat)` when the
solver produced a model, that is, a counterexample.

--expect verified: every rule in the conf has a verdict and none failed. An assert rule needs
Verified Assertions and, with rule_sanity on, a Verified sanity check, so a vacuous pass fails;
a satisfy rule needs Verified.

--expect violated: every rule in the conf is an assert rule whose Assertions came back Violated,
backed by a counterexample. A twin that passes, times out, errors, or is missing means its rule
proves less than it claims.

Exit status 0 when the expectation holds (always, with --report-only), 1 otherwise.
"""

import argparse
import json
import re
import sys
from pathlib import Path

VERDICT = re.compile(r"^(?P<verdict>[A-Z][A-Za-z ]*?): (?P<rule>rule_\w+?)(?P<part>-Assertions|-rule_not_vacuous_cvlr)?$")
ERROR = re.compile(r"^Error: .*ruleIdentifier=(?P<rule>rule_\w+?)(?P<part>-rule_not_vacuous_cvlr)?,")


def conf_rules(path):
    """The conf's rule list and whether it asks for sanity checks. Confs are JSON with // comments."""
    text = "\n".join(re.sub(r"^\s*//.*$", "", line) for line in Path(path).read_text().splitlines())
    conf = json.loads(text)
    return conf["rule"], conf.get("rule_sanity", "none") != "none"


def parse_log(path):
    """Per rule: {'assert': verdict, 'sanity': verdict, 'satisfy': verdict, 'error': message}."""
    rules = {}
    sat_violations = 0
    lines = Path(path).read_text(errors="replace").splitlines()
    for index, raw in enumerate(lines):
        line = re.sub(r"\x1b\[[0-9;]*m", "", raw).rstrip()
        match = VERDICT.match(line)
        if match:
            part = {"-Assertions": "assert", "-rule_not_vacuous_cvlr": "sanity", None: "satisfy"}[match["part"]]
            rules.setdefault(match["rule"], {})[part] = match["verdict"]
            continue
        match = ERROR.match(line)
        if match:
            rules.setdefault(match["rule"], {})["error"] = line[:200]
            continue
        # A table row: `|name|Violated |...` then `|cont|(sat) |...`.
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) > 3 and cells[2] == "Violated" and index + 1 < len(lines):
            following = [cell.strip() for cell in lines[index + 1].split("|")]
            if len(following) > 3 and following[2] == "(sat)":
                sat_violations += 1
    url = next((line.split("report url: ")[1] for line in lines if "report url: " in line), None)
    if url is None:
        url = next((line.split("results at ")[1] for line in lines if "verification results at " in line), None)
    finished = any("Finished verification request" in line for line in lines)
    return rules, sat_violations, url, finished


def describe(result):
    return ", ".join(f"{part} {verdict}" for part, verdict in sorted(result.items())) or "no verdict"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("conf")
    parser.add_argument("log")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--expect", choices=["verified", "violated"])
    mode.add_argument("--report-only", action="store_true")
    args = parser.parse_args()

    expected, sanity = conf_rules(args.conf)
    rules, sat_violations, url, finished = parse_log(args.log)
    print(f"{args.conf}: {len(expected)} rules; report {url or 'not found'}")
    if not finished:
        print("the prover did not report a finished verification request")

    problems = []
    violated_asserts = 0
    for rule in expected:
        result = rules.get(rule, {})
        if "error" in result:
            problems.append(f"{rule}: {result['error']}")
            continue
        if args.report_only:
            continue
        if args.expect == "verified":
            if "assert" in result:
                if result["assert"] != "Verified":
                    problems.append(f"{rule}: assertions {result['assert']}")
                elif sanity and result.get("sanity") != "Verified":
                    problems.append(f"{rule}: sanity check {result.get('sanity', 'missing')} (the rule may be vacuous)")
            elif "satisfy" in result:
                if result["satisfy"] != "Verified":
                    problems.append(f"{rule}: satisfy {result['satisfy']} (the run it describes was not reached)")
            else:
                problems.append(f"{rule}: no verdict in the log")
        else:
            if result.get("assert") == "Violated":
                violated_asserts += 1
            elif "assert" in result:
                problems.append(f"{rule}: twin assertions {result['assert']}, but a twin must be violated")
            elif "satisfy" in result:
                problems.append(f"{rule}: a satisfy verdict ({result['satisfy']}) is no counterexample")
            else:
                problems.append(f"{rule}: no verdict in the log")
    if args.expect == "violated" and violated_asserts and sat_violations < violated_asserts:
        problems.append(
            f"{violated_asserts} twins violated, but the results table shows only {sat_violations} "
            "violations with a counterexample (sat)"
        )
    extra = sorted(set(rules) - set(expected))
    if extra:
        problems.append(f"verdicts for rules the conf does not list: {', '.join(extra)}")

    for rule in expected:
        print(f"  {rule}: {describe(rules.get(rule, {}))}")
    if args.report_only:
        print(f"report only: {len(problems)} rules errored")
        for problem in problems:
            print(f"  {problem}")
        return 0
    if problems:
        print(f"FAILED: {len(problems)} problems")
        for problem in problems:
            print(f"  {problem}")
        return 1
    print(f"OK: every rule {'verified' if args.expect == 'verified' else 'violated with a counterexample'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
