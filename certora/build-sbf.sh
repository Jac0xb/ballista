#!/usr/bin/env bash
# Builds the specification binary with Certora's platform tools and fails if the compiler reported
# an SBF stack frame over 4 KiB.
#
# SBPF version 0 gives every function a fixed 4 KiB frame. Certora's LLVM reports a function that
# needs more as "Error: ... overflows the maximum allowed frame space ...", but `cargo certora-sbf`
# still exits 0, and the regular `cargo build-sbf` does not check at all. This script turns that
# report into a failure. It checks the program's functions and the rules alike, since the spec
# binary contains both.
#
# Usage, from anywhere: certora/build-sbf.sh [extra cargo certora-sbf arguments]
set -euo pipefail

cd "$(dirname "$0")/ballista-specs"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT

# The report appears only while a crate compiles, so a cached build would pass silently. Drop the
# fingerprints of this workspace's own crates so the program, its common crate, the adapter and the
# rules always recompile; dependencies stay cached.
rm -rf ../target/sbpf-solana-solana/release/.fingerprint/{ballista,ballista-common,ballista-specs,cvlr-pinocchio}-*

# Platform tools v1.53 is the newest Certora publishes; the program's own build uses v1.54. Both
# ship rustc 1.89.
cargo certora-sbf --tools-version v1.53 "$@" 2>&1 | tee "$log"

if grep -q "overflows the maximum allowed frame space" "$log"; then
  echo >&2
  echo "error: an SBF stack frame exceeds 4 KiB:" >&2
  grep "overflows the maximum allowed frame space" "$log" >&2
  exit 1
fi
