#!/bin/bash
# Line and region coverage of the verifier's sources by one target's corpus and seeds:
#
#   fuzz/scripts/coverage.sh TARGET
#
# Needs the nightly's llvm-tools (`rustup component add llvm-tools --toolchain nightly`). Builds
# into fuzz/coverage (ignored by git), so it never touches the fuzzing binaries.
set -euo pipefail
target=$1
cd "$(dirname "$0")/../.."
toolchain=${FUZZ_TOOLCHAIN:-nightly}
mkdir -p "fuzz/corpus/$target" fuzz/coverage
cargo "+$toolchain" fuzz coverage -s "${FUZZ_SANITIZER:-none}" --target-dir fuzz/coverage/target "$target" \
  "fuzz/corpus/$target" "fuzz/seeds/$target" > "fuzz/coverage/$target.log" 2>&1
bin="$(rustc "+$toolchain" --print sysroot)/lib/rustlib/$(rustc "+$toolchain" -vV | sed -n 's/^host: //p')/bin"
binary=$(find fuzz/coverage/target -type f -perm -u+x -path "*/release/$target" | head -1)
"$bin/llvm-cov" report "$binary" -instr-profile="fuzz/coverage/$target/coverage.profdata" \
  common/src/template/wire.rs common/src/template/verify.rs common/src/template/account.rs \
  common/src/instruction.rs
