#!/bin/bash
# Compares the SHA-256 of the built program, target/deploy/ballista.so, with the hash recorded in
# fixtures/program-sha256.txt.
#
#   scripts/program-hash.sh            check; exits 1 when the hashes differ
#   scripts/program-hash.sh --update   record the current build's hash
#
# The hash changes with the program's code, its dependencies and the toolchain, and also with code
# that only moves: an item added in the middle of execute.rs shifts the line numbers in panic
# messages, and with them the binary. What was formally checked or measured was checked on one
# binary, so a new one should be a decision. Updating the fixture is deliberate, like raising a
# compute-unit ceiling: do it in the commit that changes the binary, so the change is reviewed.
#
# Build first with `pnpm build:program` (cargo build-sbf, platform tools v1.54).
set -euo pipefail
cd "$(dirname "$0")/.."
so=target/deploy/ballista.so
fixture=fixtures/program-sha256.txt
if [ ! -f "$so" ]; then
  echo "error: $so is not built; run pnpm build:program" >&2
  exit 2
fi
if command -v sha256sum > /dev/null; then
  actual=$(sha256sum "$so" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$so" | cut -d' ' -f1)
fi
if [ "${1:-}" = "--update" ]; then
  { grep '^#' "$fixture" 2> /dev/null || true; echo "$actual  ballista.so"; } > "$fixture.new"
  mv "$fixture.new" "$fixture"
  echo "recorded $actual in $fixture"
  exit 0
fi
expected=$(grep -v '^#' "$fixture" | awk 'NF { print $1; exit }')
if [ "$actual" != "$expected" ]; then
  echo "ballista.so differs from the recorded build:" >&2
  echo "  built    $actual" >&2
  echo "  recorded $expected ($fixture)" >&2
  echo "If the change is intended, run scripts/program-hash.sh --update and commit the fixture with it." >&2
  exit 1
fi
echo "ballista.so matches $fixture: $actual"
