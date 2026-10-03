#!/bin/bash
# Runs one fuzz target for a while, from the repository root or anywhere:
#
#   fuzz/scripts/run.sh TARGET [SECONDS] [libFuzzer flags...]
#
# New inputs go to fuzz/corpus/TARGET (ignored by git); fuzz/seeds/TARGET is read as well and
# never written. Crashes land in fuzz/artifacts/TARGET. SECONDS defaults to 900.
#
# FUZZ_SANITIZER picks the sanitizer, `none` by default: on macOS 26 the AddressSanitizer runtime
# of both installed nightlies deadlocks while the process starts. Use `address` on Linux.
# FUZZ_TOOLCHAIN picks the nightly, `nightly` by default.
set -euo pipefail
target=$1
seconds=${2:-900}
shift $(( $# < 2 ? $# : 2 ))
cd "$(dirname "$0")/../.."
mkdir -p "fuzz/corpus/$target" "fuzz/artifacts/$target"
exec cargo "+${FUZZ_TOOLCHAIN:-nightly}" fuzz run -s "${FUZZ_SANITIZER:-none}" "$target" \
  "fuzz/corpus/$target" "fuzz/seeds/$target" -- \
  -max_total_time="$seconds" -max_len=11000 -timeout=5 -use_value_profile=1 -print_final_stats=1 "$@"
