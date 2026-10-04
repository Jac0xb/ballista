#!/usr/bin/env python3
"""Critic tooling: Ballista's SBF coverage from Mollusk register traces.

cargo-fuzz and proptest see only host code. Run through Mollusk, Ballista executes as SBF bytecode
inside the interpreter, so neither sees which of its branches ran. Mollusk's `register-tracing`
feature records every executed instruction's registers, the program counter among them; this
script maps those program counters onto the unstripped ELF's function symbols.

  cargo build-sbf --manifest-path programs/ballista/Cargo.toml
  SBF_TRACE_DIR=/tmp/trace cargo test --manifest-path tests/ballista/Cargo.toml --features sbf-trace -- <filter>
  scripts/critic/sbf_coverage.py /tmp/trace [--functions]

Prints the distinct program counters reached out of the text section's instruction slots, and the
functions entered; `--functions` lists each Ballista function's reached and total slots.
"""
import glob
import os
import pathlib
import struct
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
ELF = ROOT / 'target/sbpf-solana-solana/release/ballista.so'
BALLISTA = 'BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD'
READELF = sorted(glob.glob(os.path.expanduser('~/.cache/solana/*/platform-tools/llvm/bin/llvm-readelf')))[-1]


def readelf(*args):
    return subprocess.run([READELF, *args, str(ELF)], capture_output=True, text=True, check=True).stdout


def text_section():
    for line in readelf('-S', '--wide').splitlines():
        parts = line.replace('[ ', '[').split()
        if len(parts) > 5 and parts[1] == '.text':
            return int(parts[3], 16), int(parts[5], 16)
    raise SystemExit('no .text section')


def functions():
    found = []
    for line in readelf('-s', '--wide').splitlines():
        parts = line.split(None, 7)
        if len(parts) == 8 and parts[3] == 'FUNC' and int(parts[2]):
            found.append((int(parts[1], 16), int(parts[2]), parts[7]))
    return sorted(found)


def program_counters(directories):
    seen, runs = set(), 0
    for directory in directories:
        for name in os.listdir(directory):
            if not name.endswith('.program_id'):
                continue
            base = os.path.join(directory, name[: -len('.program_id')])
            if open(base + '.program_id').read().strip() != BALLISTA:
                continue
            runs += 1
            data = open(base + '.regs', 'rb').read()
            # Twelve little-endian u64 per step: r0 to r10, then the program counter.
            for offset in range(0, len(data), 96):
                seen.add(struct.unpack_from('<Q', data, offset + 88)[0])
    return seen, runs


directories = [arg for arg in sys.argv[1:] if not arg.startswith('--')]
seen, runs = program_counters(directories)
text_address, text_size = text_section()
rows = []
for address, size, name in functions():
    first, end = (address - text_address) // 8, (address - text_address + size) // 8
    rows.append((name, sum(1 for pc in range(first, end) if pc in seen), end - first))
slots = text_size // 8
print(f'{runs} Ballista invocations; {len(seen)} of {slots} instruction slots reached ({100 * len(seen) / slots:.1f}%)')
print(f'functions entered: {sum(1 for _, hit, _ in rows if hit)} of {len(rows)}')
if '--functions' in sys.argv:
    for name, hit, total in sorted(rows):
        if 'ballista' in name:
            print(f'  {hit:5d}/{total:5d}  {name.rsplit("::h", 1)[0]}')
