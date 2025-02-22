#!/bin/bash

# Import utils.
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
source $(dirname $SCRIPT_DIR)/utils.sh

# Save external programs binaries to the output directory.
source ${SCRIPT_DIR}/dump.sh

# Go to the working directory.
cd $WORKING_DIR

# Get all command-line arguments.
ARGS=$*
if [ ! -z "$ARGS" ]; then
    PROGRAMS="[\"${1}\"]"
    shift
    ARGS=$*
fi

# SOLFMT="solfmt"
export SBF_OUT_DIR="${WORKING_DIR}/${OUTPUT}"

# cd ${WORKING_DIR}/ballista-common
# RUST_LOG=error RUST_BACKTRACE=1 cargo test 2>&1

# cd ${WORKING_DIR}/programs/ballista
# echo "Running solana test-sbf for ${p}..."
# RUST_LOG=error RUST_BACKTRACE=1 BPF_OUT_DIR=${WORKING_DIR}/configs/.programs cargo test 2>&1

# for p in ${TEST_PROGRAMS[@]}; do
#     printf "\nBuilding program: ${p}...\n"

#     cd ${WORKING_DIR}/${p}
#     cargo build-sbf --sbf-out-dir ${PROGRAMS_OUTPUT_DIR} $ARGS
# done


cd ${WORKING_DIR}/tests/ballista/
# RUST_LOG=info RUST_BACKTRACE=1 
RUST_BACKTRACE=1 cargo test 2>&1
