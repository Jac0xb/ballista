/**
 * The snapshot tool's decoders, checked against the committed snapshot in tests/protocols/snapshot:
 *
 *   node --test 'scripts/snapshot/*.test.mjs'
 *
 * Checks on the committed program binaries are skipped when the checkout holds their Git LFS
 * pointers instead; a synthetic ELF covers the same rules either way.
 */
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  U64_MAX,
  base58Decode,
  base58Encode,
  decodeClock,
  decodeLookupTable,
  elfEnd,
  isAddress,
  parseJson,
  programDataAddress,
  splitProgramData,
  stringifyJson,
  trimElf,
  u64,
} from './rpc.mjs';

const SNAPSHOT = fileURLToPath(new URL('../../tests/protocols/snapshot/', import.meta.url));
const manifestText = readFileSync(`${SNAPSHOT}manifest.json`, 'utf8');
const accountsText = readFileSync(`${SNAPSHOT}accounts.json`, 'utf8');
const manifest = parseJson(manifestText);
const accounts = new Map(parseJson(accountsText).map((entry) => [entry.address, entry]));
const dataOf = (address) => Buffer.from(accounts.get(address).data, 'base64');

/** A committed program binary, or null when the checkout holds its Git LFS pointer. */
function committedElf(program) {
  const bytes = readFileSync(`${SNAPSHOT}${program.file}`);
  return bytes.subarray(0, 4).toString('latin1') === '\x7fELF' ? bytes : null;
}

/**
 * A minimal 64-bit ELF: header, one program header, 16 bytes of code, then a section-header table
 * (null, the code as PROGBITS, and a 1 MB NOBITS section that occupies no file bytes) ending in
 * zero fields, as real SBF programs do.
 */
function syntheticElf({ segmentEnd = 136 } = {}) {
  const elf = Buffer.alloc(136 + 3 * 64);
  elf.writeUInt32BE(0x7f454c46, 0);
  elf[4] = 2; // 64-bit
  elf[5] = 1; // little-endian
  elf.writeBigUInt64LE(64n, 0x20); // e_phoff
  elf.writeBigUInt64LE(136n, 0x28); // e_shoff
  elf.writeUInt16LE(56, 0x36); // e_phentsize
  elf.writeUInt16LE(1, 0x38); // e_phnum
  elf.writeUInt16LE(64, 0x3a); // e_shentsize
  elf.writeUInt16LE(3, 0x3c); // e_shnum
  elf.writeUInt32LE(1, 64); // PT_LOAD
  elf.writeBigUInt64LE(120n, 64 + 8); // p_offset
  elf.writeBigUInt64LE(BigInt(segmentEnd - 120), 64 + 32); // p_filesz
  elf.fill(0xab, 120, 136); // the code
  const section = (index, type, offset, size) => {
    const at = 136 + index * 64;
    elf.writeUInt32LE(type, at + 4);
    elf.writeBigUInt64LE(BigInt(offset), at + 24);
    elf.writeBigUInt64LE(BigInt(size), at + 32);
    elf.writeBigUInt64LE(1n, at + 48); // sh_addralign; sh_entsize stays 0
  };
  section(1, 1, 120, 16); // PROGBITS
  section(2, 8, 136, 1_000_000); // NOBITS
  return elf;
}

test('base58 round-trips every address in the snapshot and keeps leading zeros', () => {
  assert.equal(base58Encode(Buffer.alloc(32)), '11111111111111111111111111111111');
  assert.deepEqual(base58Decode('11111111111111111111111111111111'), Buffer.alloc(32));
  assert.equal(base58Encode(Buffer.from([0, 0, 1])), '112');
  assert.deepEqual(base58Decode('112'), Buffer.from([0, 0, 1]));
  for (const { address } of manifest.accounts) {
    const bytes = base58Decode(address);
    assert.equal(bytes.length, 32, address);
    assert.equal(base58Encode(bytes), address);
  }
});

test('isAddress accepts 32-byte base58 keys only', () => {
  assert.ok(isAddress('JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4'));
  assert.ok(!isAddress('JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV')); // 31 bytes
  assert.ok(!isAddress('0OIl')); // characters base58 leaves out
  assert.ok(!isAddress('not-an-address'));
  assert.ok(!isAddress(42));
  assert.throws(() => base58Decode('0'), /not base58/);
});

test('parseJson keeps u64s exact, and stringifyJson writes them back unchanged', () => {
  const text = '{"rentEpoch":18446744073709551615,"lamports":1461600,"timeTaken":0.0014,"big":1e21}';
  const value = parseJson(text);
  assert.equal(value.rentEpoch, U64_MAX); // plain JSON.parse gives 18446744073709552000
  assert.equal(value.lamports, 1461600);
  assert.equal(value.timeTaken, 0.0014);
  assert.equal(value.big, 1e21);
  assert.equal(stringifyJson(value), text.replace('1e21', '1e+21'));
  assert.equal(stringifyJson({ slot: 451100151n }), '{"slot":451100151}');
  assert.equal(u64(5n), 5);
  assert.equal(u64(U64_MAX), U64_MAX);
});

test('the committed JSON files are exactly what the writer produces from them', () => {
  const lines = [...accounts.values()].map((entry) => stringifyJson(entry));
  assert.equal(`[\n${lines.join(',\n')}\n]\n`, accountsText);
  assert.equal(`${stringifyJson(manifest, 2)}\n`, manifestText);
  for (const entry of accounts.values()) assert.equal(entry.rentEpoch, U64_MAX, entry.address);
});

test('decodeLookupTable reads the snapshot tables as the manifest records them', () => {
  assert.ok(manifest.lookupTables.length > 0);
  for (const expected of manifest.lookupTables) {
    const table = decodeLookupTable(dataOf(expected.address));
    assert.equal(table.deactivationSlot, U64_MAX, 'active');
    assert.equal(table.lastExtendedSlot, BigInt(expected.lastExtendedSlot));
    assert.ok(table.lastExtendedSlot < BigInt(manifest.slot));
    assert.equal(table.lastExtendedSlotStartIndex, expected.lastExtendedSlotStartIndex);
    assert.equal(table.authority, expected.authority);
    assert.equal(table.addresses.length, expected.entries);
    assert.ok(table.addresses.every(isAddress));
  }
});

test('decodeLookupTable rejects anything but a table', () => {
  const table = dataOf(manifest.lookupTables[0].address);
  const wrongType = Buffer.from(table);
  wrongType.writeUInt32LE(0, 0); // uninitialized
  assert.throws(() => decodeLookupTable(wrongType), /not an address lookup table/);
  assert.throws(() => decodeLookupTable(table.subarray(0, 55)), /not an address lookup table/);
  assert.throws(() => decodeLookupTable(table.subarray(0, 56 + 31)), /not an address lookup table/);
});

test('programDataAddress and splitProgramData read each upgradeable program', () => {
  const upgradeable = manifest.programs.filter((program) => program.programData);
  assert.ok(upgradeable.length > 0);
  for (const program of upgradeable) {
    assert.equal(programDataAddress(dataOf(program.programId)), program.programData);
    const header = dataOf(program.programData); // the committed entry holds only the header
    const split = splitProgramData(header);
    assert.equal(split.deploySlot, program.deploySlot);
    assert.equal(split.upgradeAuthority, program.upgradeAuthority);
    assert.equal(split.header.length, 45);
    const elf = committedElf(program);
    if (elf) assert.deepEqual(splitProgramData(Buffer.concat([header, elf])).elf, elf);
  }
  const program = upgradeable[0];
  assert.throws(() => splitProgramData(dataOf(program.programId)), /not a ProgramData account/);
  assert.throws(() => programDataAddress(dataOf(program.programData)), /not an upgradeable Program/);
});

test('decodeClock reads the five Clock fields', () => {
  const { clock } = manifest;
  const data = Buffer.alloc(40);
  data.writeBigUInt64LE(BigInt(clock.slot), 0);
  data.writeBigInt64LE(BigInt(clock.epochStartTimestamp), 8);
  data.writeBigUInt64LE(BigInt(clock.epoch), 16);
  data.writeBigUInt64LE(BigInt(clock.leaderScheduleEpoch), 24);
  data.writeBigInt64LE(BigInt(clock.unixTimestamp), 32);
  assert.deepEqual(decodeClock(data), clock);
  assert.throws(() => decodeClock(data.subarray(0, 39)), /not 40/);
});

test('elfEnd finds the end of a synthetic ELF, ignoring NOBITS and counting segments', () => {
  const elf = syntheticElf();
  assert.equal(elfEnd(elf), elf.length); // the section-header table comes last
  assert.equal(elf.at(-1), 0);
  // A segment reaching past the section-header table extends the ELF.
  const longer = Buffer.concat([syntheticElf({ segmentEnd: elf.length + 40 }), Buffer.alloc(40)]);
  assert.equal(elfEnd(longer), elf.length + 40);
});

test('elfEnd rejects what is not a whole 64-bit little-endian ELF', () => {
  const elf = syntheticElf();
  assert.throws(() => elfEnd(Buffer.alloc(64)), /no ELF magic/);
  const bigEndian = Buffer.from(elf);
  bigEndian[5] = 2;
  assert.throws(() => elfEnd(bigEndian), /64-bit little-endian/);
  assert.throws(() => elfEnd(elf.subarray(0, elf.length - 1)), /headers reach byte/);
  const overlong = syntheticElf({ segmentEnd: elf.length + 1 });
  assert.throws(() => elfEnd(overlong), /contents reach byte/);
});

test('trimElf cuts zero padding at the ELF end, never the zeros the ELF ends with', () => {
  const elf = syntheticElf();
  const padded = trimElf(Buffer.concat([elf, Buffer.alloc(4096)]));
  assert.deepEqual(padded.elf, elf);
  assert.equal(padded.padding, 4096);
  assert.equal(padded.warning, undefined);
  const exact = trimElf(elf);
  assert.equal(exact.elf.length, elf.length);
  assert.equal(exact.padding, 0);
  const dirty = trimElf(Buffer.concat([elf, Buffer.from([0, 0, 7])]));
  assert.equal(dirty.elf.length, elf.length + 3, 'kept whole');
  assert.match(dirty.warning, /non-zero/);
});

test('the committed program binaries are whole ELFs, trimmed exactly at their end', (t) => {
  for (const program of manifest.programs) {
    const elf = committedElf(program);
    if (!elf) {
      t.skip(`${program.file} is a Git LFS pointer; run git lfs pull`);
      continue;
    }
    assert.equal(elf.length, program.elfLength, program.name);
    assert.equal(elfEnd(elf), elf.length, program.name);
    // Every one ends in the zero fields of its last section header, which must survive a trim.
    assert.equal(elf.at(-1), 0, program.name);
    assert.deepEqual(trimElf(Buffer.concat([elf, Buffer.alloc(1024)])).elf, elf, program.name);
  }
});
