#!/usr/bin/env python3
"""Lists the stack copies in the spec binary that the Certora prover may not follow.

The prover keeps stack values by offset. An eight-byte stack load that spans narrower stores carries
at most the store at its own offset: a field at another offset inside the word, such as an error
code at offset 4 after a two-byte tag, is lost. Two four-byte halves are no exception. The control
`rule_stack_word_copy_keeps_both_halves` failed at cb2fb2d (prover jobs
88f0a104b1624caf821dc961242d6bec and a83bc7fc09084b30ac2d1ab974763c62): the front end turned the
copy into a `memcpy`, and the load after it read the first half alone. LLVM moves `RunResult` errors
and `RuntimeValue`s this way.

For every function whose name contains PATTERN (default: rule_), this prints each eight-byte load
from the stack that spans narrower stores. It scans each function's code in address order and
ignores control flow, so a hit marks code to read, not a proven imprecision. Some rules with hits
did prove in prover jobs: a copy loses nothing a rule reads if the rule reads only the first store,
such as a one-byte `Ok`/`Err` tag at offset 0.

Usage, after certora/build-sbf.sh:
    certora/scan-stack-copies.py [PATTERN]
Environment: SO (binary), OBJDUMP (llvm-objdump; defaults to Certora's platform tools v1.53).
"""
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SO = os.environ.get('SO', os.path.join(HERE, 'target/sbpf-solana-solana/release/ballista_specs.so'))
OBJDUMP = os.environ.get(
    'OBJDUMP',
    os.path.expanduser('~/.cache/solana/v1.53/platform-tools-certora/llvm/bin/llvm-objdump'),
)
WIDTH = {'b': 1, 'h': 2, 'w': 4, 'dw': 8}
STORE = re.compile(r'\s([0-9a-f]+):\s+stx?(b|h|w|dw) \[r10 - 0x([0-9a-f]+)\]')
LOAD = re.compile(r'\s([0-9a-f]+):\s+ldxdw r\d+, \[r10 - 0x([0-9a-f]+)\]')


def functions():
    table = subprocess.run([OBJDUMP, '-t', SO], capture_output=True, text=True, check=True).stdout
    found = set()
    for line in table.splitlines():
        parts = line.split()
        if len(parts) >= 5 and ' F ' in line and '.text' in line:
            found.add((int(parts[0], 16), parts[-1]))
    return sorted(found)


def scan(start, stop):
    text = subprocess.run(
        [OBJDUMP, '-d', '--no-show-raw-insn', f'--start-address={start:#x}', f'--stop-address={stop:#x}', SO],
        capture_output=True, text=True, check=True,
    ).stdout
    cells = {}  # stack offset -> width of the last store that starts there
    hits = []
    for line in text.splitlines():
        match = STORE.search(line)
        if match:
            offset, width = -int(match.group(3), 16), WIDTH[match.group(2)]
            for other in list(cells):
                if other < offset + width and offset < other + cells[other]:
                    del cells[other]
            cells[offset] = width
            continue
        match = LOAD.search(line)
        if match:
            offset = -int(match.group(2), 16)
            inside = sorted((o, w) for o, w in cells.items() if offset <= o < offset + 8)
            if inside and inside != [(offset, 8)]:
                hits.append((match.group(1), offset, inside))
    return hits


def main():
    pattern = sys.argv[1] if len(sys.argv) > 1 else 'rule_'
    listed = functions()
    for index, (start, name) in enumerate(listed):
        if pattern not in name:
            continue
        stop = listed[index + 1][0] if index + 1 < len(listed) else start + 0x40000
        hits = scan(start, stop)
        if hits:
            print(f'{len(hits):3d} {name}')
            for address, offset, inside in hits[:3]:
                stores = ', '.join(f'{width} bytes at r10{o:+#x}' for o, width in inside)
                print(f'      {address}: load of 8 bytes at r10{offset:+#x} over {stores}')


if __name__ == '__main__':
    main()
