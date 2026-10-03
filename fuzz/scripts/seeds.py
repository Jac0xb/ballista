#!/usr/bin/env python3
"""Regenerates the seed corpora in fuzz/seeds from the repository's real templates.

Run from anywhere: python3 fuzz/scripts/seeds.py

Templates come from every payload in:
  fixtures/*.hex and clients/rust/tests/fixtures/*.hex
  fixtures/protocol-examples.json, fixtures/protocol-scenarios.json, fixtures/benchmarks.json
  clients/rust/tests/fixtures/docs-examples.json
A JSON fixture's templates are the hex strings that start with the payload magic, "BVM1".

Each target gets what its input format needs:
  parse         each payload; each as a finalized template account; each as CreateTemplate data
  verify        each payload
  differential  each payload
  structured    each payload as a base program (mode byte 2, then a little-endian u16 length),
                and a few streams for the two generators (mode bytes 0 and 1)
"""

import hashlib
import json
import random
import re
import shutil
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SEEDS = ROOT / "fuzz" / "seeds"
MAGIC_HEX = "42564d31"  # "BVM1"
HEX = re.compile(r"^[0-9a-f]+$")


def hex_files():
    for directory in (ROOT / "fixtures", ROOT / "clients" / "rust" / "tests" / "fixtures"):
        for path in sorted(directory.glob("*.hex")):
            text = path.read_text().strip()
            if text.startswith(MAGIC_HEX):
                yield f"{path.stem}", bytes.fromhex(text)


def json_payloads(path):
    def walk(node, trail):
        if isinstance(node, dict):
            for key, value in node.items():
                yield from walk(value, trail + [key])
        elif isinstance(node, list):
            for index, value in enumerate(node):
                yield from walk(value, trail + [str(index)])
        elif isinstance(node, str) and node.startswith(MAGIC_HEX) and HEX.match(node):
            yield "-".join(part for part in trail if part not in ("payload", "templateHex")), bytes.fromhex(node)

    yield from walk(json.loads(path.read_text()), [path.stem])


def templates():
    seen = {}
    sources = [hex_files()]
    for relative in (
        "fixtures/protocol-examples.json",
        "fixtures/protocol-scenarios.json",
        "fixtures/benchmarks.json",
        "clients/rust/tests/fixtures/docs-examples.json",
    ):
        sources.append(json_payloads(ROOT / relative))
    for source in sources:
        for name, payload in source:
            digest = hashlib.sha256(payload).hexdigest()[:10]
            if digest not in seen:
                seen[digest] = (re.sub(r"[^A-Za-z0-9_.-]+", "_", name)[:60], payload)
    return [(f"{name}-{digest}", payload) for digest, (name, payload) in sorted(seen.items(), key=lambda item: item[1][0])]


def template_account(payload):
    """A finalized template account, laid out as docs/reference/wire-format.md describes."""
    header = bytes([1, 2, 1, 254]) + bytes([7] * 32) + struct.pack("<H", 1) + bytes(2)
    header += struct.pack("<II", len(payload), len(payload)) + hashlib.sha256(payload).digest()
    assert len(header) == 80
    return header + payload


def create_template(payload):
    """CreateTemplate instruction data: discriminator 0, template id, payload hash, payload."""
    return bytes([0]) + struct.pack("<H", 1) + hashlib.sha256(payload).digest() + payload


def main():
    found = templates()
    targets = ("parse", "verify", "differential", "structured")
    for target in targets:
        shutil.rmtree(SEEDS / target, ignore_errors=True)
        (SEEDS / target).mkdir(parents=True)
    for name, payload in found:
        (SEEDS / "parse" / name).write_bytes(payload)
        (SEEDS / "parse" / f"{name}.account").write_bytes(template_account(payload))
        (SEEDS / "parse" / f"{name}.create").write_bytes(create_template(payload))
        (SEEDS / "verify" / name).write_bytes(payload)
        (SEEDS / "differential" / name).write_bytes(payload)
        (SEEDS / "structured" / name).write_bytes(bytes([2]) + struct.pack("<H", len(payload)) + payload)
    # Choice streams for the two generators. Fixed seeds keep the corpus reproducible.
    rng = random.Random(0xBA11)
    for mode in (0, 1):
        for index in range(16):
            stream = bytes(rng.getrandbits(8) for _ in range(rng.choice((64, 256, 1024))))
            (SEEDS / "structured" / f"generator{mode}-{index:02}").write_bytes(bytes([mode]) + stream)
    for target in targets:
        files = list((SEEDS / target).iterdir())
        size = sum(path.stat().st_size for path in files)
        print(f"{target:13} {len(files):4} seeds {size / 1024:8.1f} KiB")
    print(f"{len(found)} distinct templates")


if __name__ == "__main__":
    main()
