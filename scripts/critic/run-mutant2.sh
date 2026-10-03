#!/bin/bash
# Usage: run-mutant2.sh NAME [suites...]
#   suites: mollusk protocols host fuzz (default: mollusk protocols host)
#
# Applies one mutant from mutate2.py, rebuilds the program, runs the suites, restores the source,
# rebuilds, and checks that the restored target/deploy/ballista.so is byte-identical to the one
# before the mutant. `fuzz` runs only the executor fuzz loop (FV_CASES, default 1500; FV_LIMITS and
# BALLISTA_MAINNET_FEATURES pass through). The summary's last line is mutant_verdict.py's verdict,
# under which a mutant that only a compute-unit ceiling notices survives.
#
# Exit status: 0 killed, 3 survived, 2 error (no build, no results, or a restored binary that
# differs from the original).
set -u
W=$(cd "$(dirname "$0")/../.." && pwd)
SP=${MUTANTS_DIR:-/tmp/ballista-mutants}; mkdir -p "$SP/mutants"
NAME=$1; shift
SUITES=${*:-mollusk protocols host}
OUT=$SP/mutants/$NAME
mkdir -p "$OUT"; rm -f "$OUT"/summary.txt "$OUT"/*.log
cd "$W" || exit 2
git diff --quiet -- programs common || { echo "worktree dirty, refusing"; exit 2; }
build() { cargo build-sbf --manifest-path programs/ballista/Cargo.toml > "$1" 2>&1; }
[ -f target/deploy/ballista.so ] || build "$OUT/build-original.log"
BEFORE=$(shasum -a 256 target/deploy/ballista.so | cut -d' ' -f1)
python3 "$W/scripts/critic/mutate2.py" "$NAME" > "$OUT/apply.log" 2>&1 || { cat "$OUT/apply.log"; git checkout -- programs common; exit 2; }
build "$OUT/build.log" || echo "BUILD FAILED" >> "$OUT/summary.txt"
for s in $SUITES; do
  case $s in
    mollusk) cargo test --no-fail-fast --manifest-path tests/ballista/Cargo.toml -- --test-threads=8 > "$OUT/mollusk.log" 2>&1 ;;
    protocols) cargo test --no-fail-fast --manifest-path tests/protocols/Cargo.toml > "$OUT/protocols.log" 2>&1 ;;
    host) cargo test --no-fail-fast -p ballista-common -p ballista --features ballista-common/proptest > "$OUT/host.log" 2>&1 ;;
    fuzz) FV_CASES=${FV_CASES:-1500} cargo test --no-fail-fast --manifest-path tests/ballista/Cargo.toml \
            fuzz::executor::fuzz_executor_differential -- --nocapture > "$OUT/fuzz.log" 2>&1 ;;
    *) echo "unknown suite $s"; exit 2 ;;
  esac
done
git checkout -- programs/ballista/src/processor/execute.rs common/src/template/verify.rs
build "$OUT/rebuild-clean.log"
AFTER=$(shasum -a 256 target/deploy/ballista.so | cut -d' ' -f1)
{
  echo "== $NAME"
  if [ "$BEFORE" = "$AFTER" ]; then
    echo "restored ballista.so is byte-identical: $AFTER"
  else
    echo "RESTORED BALLISTA.SO DIFFERS: before $BEFORE, after $AFTER"
  fi
} >> "$OUT/summary.txt"
# shellcheck disable=SC2086 # one argument per suite
python3 "$W/scripts/critic/mutant_verdict.py" "$OUT" $SUITES >> "$OUT/summary.txt"
status=$?
cat "$OUT/summary.txt"
[ "$BEFORE" = "$AFTER" ] || exit 2
exit $status
