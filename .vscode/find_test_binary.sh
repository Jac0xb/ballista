#!/bin/bash

# This is for step-through debugging the ballista tests in VSCode which requires locating the binary file of the test lib build.

# Find the latest ballista_tests test binary in the target/debug/deps directory
latest_test_binary=$(ls -t $1/target/debug/deps/tests_lib-* | grep -v '\.' | head -n 1)

filename=$(basename "$latest_test_binary" | awk '{ gsub(/[ \t\n\r]+$/, "", $0); print $0 }')
mkdir -p $2

echo "KEY=${filename}" > $2/ballista_test_binary.txt