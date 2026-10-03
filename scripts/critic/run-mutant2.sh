#!/bin/bash
# Usage: run-mutant2.sh NAME [suites...]  (suites: mollusk protocols host; default all)
set -u
W=$(cd "$(dirname "$0")/../.." && pwd)
SP=${MUTANTS_DIR:-/tmp/ballista-mutants}; mkdir -p $SP/mutants
NAME=$1; shift
SUITES=${*:-mollusk protocols host}
OUT=$SP/mutants/$NAME
mkdir -p $OUT; rm -f $OUT/summary.txt
cd $W
git diff --quiet -- programs common || { echo "worktree dirty, refusing"; exit 1; }
python3 $W/scripts/critic/mutate2.py $NAME > $OUT/apply.log 2>&1 || { cat $OUT/apply.log; git checkout -- programs common; exit 1; }
cargo build-sbf --manifest-path programs/ballista/Cargo.toml > $OUT/build.log 2>&1 || echo "BUILD FAILED" >> $OUT/summary.txt
for s in $SUITES; do
  case $s in
    mollusk) cargo test --no-fail-fast --manifest-path tests/ballista/Cargo.toml -- --test-threads=8 > $OUT/mollusk.log 2>&1 ;;
    protocols) cargo test --no-fail-fast --manifest-path tests/protocols/Cargo.toml > $OUT/protocols.log 2>&1 ;;
    host) cargo test --no-fail-fast -p ballista-common -p ballista --features ballista-common/proptest > $OUT/host.log 2>&1 ;;
  esac
done
git checkout -- programs/ballista/src/processor/execute.rs common/src/template/verify.rs
cargo build-sbf --manifest-path programs/ballista/Cargo.toml > $OUT/rebuild-clean.log 2>&1
{
  echo "== $NAME"
  for suite in $SUITES; do
    passed=$(grep -E '^test result' $OUT/$suite.log | sed -E 's/.* ([0-9]+) passed.*/\1/' | paste -sd+ - | bc)
    failed=$(grep -E '^test result' $OUT/$suite.log | sed -E 's/.* ([0-9]+) failed.*/\1/' | paste -sd+ - | bc)
    echo "$suite: passed=$passed failed=$failed"
    grep -E '^test .* FAILED$' $OUT/$suite.log | sed 's/^/   /'
  done
} >> $OUT/summary.txt
cat $OUT/summary.txt
