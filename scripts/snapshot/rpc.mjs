/**
 * Solana JSON-RPC and account layouts for the protocol-test snapshot. Plain `fetch` and Node
 * built-ins, no dependencies; Node 22 or later.
 *
 * What the snapshot relies on, measured against mainnet:
 * - One `getMultipleAccounts` call is served from one bank, so every account in it is at
 *   `context.slot`. It takes at most 100 keys.
 * - `minContextSlot` is only a lower bound: a node answers at its current slot, and no method
 *   returns older state. Several calls agree on a slot only by retrying until they do.
 * - The public endpoint refuses JSON-RPC batches of `getMultipleAccounts` (HTTP 429 for every
 *   element), so each call here is its own HTTP request.
 * - `rentEpoch` is u64::MAX for most accounts. Plain `JSON.parse` rounds it to
 *   18446744073709552000, so responses are parsed BigInt-safely.
 */
import { createHash } from 'node:crypto';
import zlib from 'node:zlib';

export const CLOCK_SYSVAR = 'SysvarC1ock11111111111111111111111111111111';
export const SYSVAR_OWNER = 'Sysvar1111111111111111111111111111111111111';
export const NATIVE_LOADER = 'NativeLoader1111111111111111111111111111111';
export const LOADER_V3 = 'BPFLoaderUpgradeab1e11111111111111111111111';
export const LOADER_V2 = 'BPFLoader2111111111111111111111111111111111';
export const LOADER_V1 = 'BPFLoader1111111111111111111111111111111111';
export const LOOKUP_TABLE_PROGRAM = 'AddressLookupTab1e1111111111111111111111111';
export const U64_MAX = (1n << 64n) - 1n;

export const RPC_URL = process.env.SOLANA_RPC_URL || 'https://api.mainnet.solana.com';
/** The endpoint's host alone: a private URL can carry an API key in its path or query. */
export const RPC_HOST = new URL(RPC_URL).host;

const MAX_KEYS_PER_CALL = 100;
const MAX_ATTEMPTS = 8;
const REQUEST_TIMEOUT_MS = 120_000;

// ------------------------------------------------------------------------------------ JSON

// Without source text in revivers, `parseJson` would round u64s silently instead of failing.
if (
  typeof JSON.rawJSON !== 'function' ||
  JSON.parse('1', (key, value, context) => context?.source) !== '1'
) {
  throw new Error(`Node ${process.versions.node} is too old; the snapshot tool needs Node 22 or later`);
}

/**
 * `JSON.parse`, keeping integers beyond 2^53 exact as BigInt. Node 22 hands revivers the source
 * text, which is what makes this possible.
 */
export function parseJson(text) {
  return JSON.parse(text, (key, value, context) =>
    typeof value === 'number' &&
    !Number.isSafeInteger(value) &&
    /^-?\d+$/.test(context?.source ?? '')
      ? BigInt(context.source)
      : value,
  );
}

/** `JSON.stringify`, writing BigInt as an exact JSON number. */
export function stringifyJson(value, indent) {
  return JSON.stringify(
    value,
    (key, item) => (typeof item === 'bigint' ? JSON.rawJSON(item.toString()) : item),
    indent,
  );
}

/** A u64 read as BigInt, as a Number when that is exact. */
export const u64 = (value) =>
  value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value;

// --------------------------------------------------------------------------------- retries

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Exponential backoff with jitter (1 s, 2 s, 4 s … 30 s), never shorter than `retry-after`. */
export function backoffMs(attempt, retryAfter) {
  const exponential = Math.min(30_000, 500 * 2 ** attempt) * (0.75 + Math.random() * 0.5);
  const hinted = Number(retryAfter) * 1000;
  return Math.max(exponential, Number.isFinite(hinted) ? hinted : 0);
}

/**
 * `fetch` with retries on network failures, HTTP 429 and 5xx. Any other response goes to
 * `accept(text, status)`, which returns `{ value }`, asks for another attempt with
 * `{ retry: reason }`, or throws to stop.
 */
export async function fetchWithRetry(label, url, init, accept) {
  for (let attempt = 1; ; attempt++) {
    let response;
    let text;
    let reason;
    try {
      response = await fetch(url, { ...init, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
      text = await response.text();
    } catch (error) {
      response = undefined;
      reason = error.cause?.message ?? error.message;
    }
    if (response && (response.status === 429 || response.status >= 500)) {
      reason = `HTTP ${response.status} ${text.slice(0, 160).trim()}`;
    } else if (response) {
      const outcome = accept(text, response.status);
      if (outcome.retry === undefined) return outcome.value;
      reason = outcome.retry;
    }
    if (attempt >= MAX_ATTEMPTS) {
      throw new Error(`${label}: gave up after ${attempt} attempts: ${reason}`);
    }
    const wait = backoffMs(attempt, response?.headers.get('retry-after'));
    console.error(`  ${label}: ${reason}; retrying in ${(wait / 1000).toFixed(1)} s`);
    await sleep(wait);
  }
}

// ------------------------------------------------------------------------------------- RPC

/** JSON-RPC errors worth another attempt: rate limits, and a node behind `minContextSlot`. */
const RETRYABLE_RPC_ERRORS = new Set([
  429,
  -32005, // the node is behind
  -32016, // minimum context slot has not been reached
]);

let requestId = 0;

/** One JSON-RPC call, sent as its own HTTP request (never a batch). */
export function rpc(method, params) {
  const label = `${method} via ${RPC_HOST}`;
  const body = stringifyJson({ jsonrpc: '2.0', id: ++requestId, method, params });
  const init = { method: 'POST', headers: { 'content-type': 'application/json' }, body };
  return fetchWithRetry(label, RPC_URL, init, (text, status) => {
    let reply;
    try {
      reply = parseJson(text);
    } catch {
      throw new Error(`${label}: HTTP ${status}, not JSON: ${text.slice(0, 200)}`);
    }
    if (!reply.error) return { value: reply.result };
    if (RETRYABLE_RPC_ERRORS.has(reply.error.code)) return { retry: stringifyJson(reply.error) };
    throw new Error(`${label}: ${stringifyJson(reply.error)}`);
  });
}

const ZSTD = typeof zlib.zstdDecompressSync === 'function'; // Node 22.15 and later
const ENCODING = ZSTD ? 'base64+zstd' : 'base64';

/** An RPC account value as `{ owner, lamports, executable, rentEpoch, data: Buffer }`, or null. */
export function decodeAccount(address, value) {
  if (!value) return null;
  const [encoded, encoding] = value.data;
  const raw = Buffer.from(encoded, 'base64');
  const data = encoding === 'base64+zstd' && raw.length ? zlib.zstdDecompressSync(raw) : raw;
  if (value.space !== undefined && Number(value.space) !== data.length) {
    throw new Error(`${address}: decoded ${data.length} bytes, but the RPC says ${value.space}`);
  }
  return {
    owner: value.owner,
    lamports: value.lamports,
    executable: value.executable,
    rentEpoch: value.rentEpoch,
    data,
  };
}

/**
 * Every account in `addresses` at one slot, plus the Clock sysvar of that slot.
 *
 * Keys go out in chunks of at most 100, one request per chunk, each chunk carrying the Clock so
 * that every response states its own slot and time. If the chunks come back from different slots,
 * all of them are fetched again with `minContextSlot` raised to the newest one seen, until they
 * agree. Missing accounts map to null.
 */
export async function getAccountsAtOneSlot(addresses, { minContextSlot } = {}) {
  const keys = [...new Set(addresses)].filter((key) => key !== CLOCK_SYSVAR);
  const perCall = MAX_KEYS_PER_CALL - 1;
  for (let round = 1; round <= 10; round++) {
    const accounts = new Map();
    const slots = new Set();
    let clock;
    for (let start = 0; start === 0 || start < keys.length; start += perCall) {
      const chunk = [...keys.slice(start, start + perCall), CLOCK_SYSVAR];
      const config = {
        encoding: ENCODING,
        commitment: 'confirmed',
        ...(minContextSlot !== undefined && { minContextSlot }),
      };
      const { context, value } = await rpc('getMultipleAccounts', [chunk, config]);
      const slot = Number(context.slot);
      value.forEach((entry, index) => accounts.set(chunk[index], decodeAccount(chunk[index], entry)));
      clock = decodeClock(accounts.get(CLOCK_SYSVAR).data);
      if (clock.slot !== slot) {
        throw new Error(`the Clock read at context slot ${slot} says slot ${clock.slot}`);
      }
      slots.add(slot);
    }
    accounts.delete(CLOCK_SYSVAR);
    if (slots.size === 1) return { slot: clock.slot, clock, accounts };
    minContextSlot = Math.max(...slots);
    console.error(`  chunks came from slots ${[...slots].join(', ')}; fetching again`);
  }
  throw new Error(`could not read ${keys.length} accounts at one slot`);
}

// --------------------------------------------------------------------------------- layouts

/** The Clock sysvar: slot, epoch_start_timestamp, epoch, leader_schedule_epoch, unix_timestamp. */
export function decodeClock(data) {
  if (data.length !== 40) throw new Error(`the Clock sysvar is ${data.length} bytes, not 40`);
  return {
    slot: Number(data.readBigUInt64LE(0)),
    epochStartTimestamp: Number(data.readBigInt64LE(8)),
    epoch: Number(data.readBigUInt64LE(16)),
    leaderScheduleEpoch: Number(data.readBigUInt64LE(24)),
    unixTimestamp: Number(data.readBigInt64LE(32)),
  };
}

/**
 * An address lookup table: a 56-byte header (`u32` type 1, `u64 deactivation_slot`, `u64
 * last_extended_slot`, `u8 last_extended_slot_start_index`, `Option<Pubkey>` authority, 2 bytes
 * of padding), then 32-byte addresses.
 */
export function decodeLookupTable(data) {
  if (data.length < 56 || data.readUInt32LE(0) !== 1 || (data.length - 56) % 32 !== 0) {
    throw new Error('not an address lookup table');
  }
  const addresses = [];
  for (let offset = 56; offset < data.length; offset += 32) {
    addresses.push(base58Encode(data.subarray(offset, offset + 32)));
  }
  return {
    deactivationSlot: data.readBigUInt64LE(4),
    lastExtendedSlot: data.readBigUInt64LE(12),
    lastExtendedSlotStartIndex: data[20],
    authority: data[21] === 1 ? base58Encode(data.subarray(22, 54)) : null,
    addresses,
  };
}

/** A loader-v3 Program account is `u32` tag 2 followed by its ProgramData address (36 bytes). */
export function programDataAddress(programAccountData) {
  if (programAccountData.length < 36 || programAccountData.readUInt32LE(0) !== 2) {
    throw new Error('not an upgradeable Program account');
  }
  return base58Encode(programAccountData.subarray(4, 36));
}

export const PROGRAM_DATA_HEADER_LENGTH = 45;

/**
 * A ProgramData account: `u32` tag 3, `u64` deploy slot, `Option<Pubkey>` upgrade authority
 * (33 bytes reserved either way), which makes 45 bytes; then the ELF, zero-padded to the size the
 * account was allocated with.
 */
export function splitProgramData(data) {
  if (data.length < PROGRAM_DATA_HEADER_LENGTH || data.readUInt32LE(0) !== 3) {
    throw new Error('not a ProgramData account');
  }
  return {
    deploySlot: Number(data.readBigUInt64LE(4)),
    upgradeAuthority: data[12] === 1 ? base58Encode(data.subarray(13, 45)) : null,
    header: data.subarray(0, PROGRAM_DATA_HEADER_LENGTH),
    elf: data.subarray(PROGRAM_DATA_HEADER_LENGTH),
  };
}

/**
 * Where an ELF's own bytes end: the furthest extent of its header, program headers, section
 * headers and section contents. In an SBF program the section-header table comes last, so this is
 * `e_shoff + e_shnum × e_shentsize`.
 */
export function elfEnd(elf) {
  if (elf.length < 64 || elf.readUInt32BE(0) !== 0x7f454c46) throw new Error('no ELF magic');
  if (elf[4] !== 2 || elf[5] !== 1) throw new Error('not a 64-bit little-endian ELF');
  const phoff = Number(elf.readBigUInt64LE(0x20));
  const shoff = Number(elf.readBigUInt64LE(0x28));
  const [phentsize, phnum] = [elf.readUInt16LE(0x36), elf.readUInt16LE(0x38)];
  const [shentsize, shnum] = [elf.readUInt16LE(0x3a), elf.readUInt16LE(0x3c)];
  let end = Math.max(64, phoff + phnum * phentsize, shoff + shnum * shentsize);
  if (end > elf.length) throw new Error(`ELF headers reach byte ${end} of ${elf.length}`);
  for (let index = 0; index < phnum; index++) {
    const at = phoff + index * phentsize;
    end = Math.max(end, Number(elf.readBigUInt64LE(at + 8) + elf.readBigUInt64LE(at + 32)));
  }
  const SHT_NOBITS = 8;
  for (let index = 0; index < shnum; index++) {
    const at = shoff + index * shentsize;
    if (elf.readUInt32LE(at + 4) === SHT_NOBITS) continue;
    end = Math.max(end, Number(elf.readBigUInt64LE(at + 24) + elf.readBigUInt64LE(at + 32)));
  }
  if (end > elf.length) throw new Error(`ELF contents reach byte ${end} of ${elf.length}`);
  return end;
}

/**
 * An ELF without the zero padding after it. The cut is at `elfEnd`, never at the last non-zero
 * byte: an ELF may end in zeros (its last section header does), and Jupiter's loses 15 real bytes
 * that way and then fails to load. If anything but zeros follows the ELF, nothing is cut.
 */
export function trimElf(bytes) {
  const end = elfEnd(bytes);
  const tail = bytes.subarray(end);
  if (!tail.equals(Buffer.alloc(tail.length))) {
    return { elf: bytes, padding: 0, warning: 'non-zero bytes follow the ELF; kept whole' };
  }
  return { elf: bytes.subarray(0, end), padding: tail.length };
}

// -------------------------------------------------------------------------------- encoding

const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

export function base58Encode(bytes) {
  let value = BigInt('0x' + (Buffer.from(bytes).toString('hex') || '0'));
  let text = '';
  while (value > 0n) {
    text = BASE58[Number(value % 58n)] + text;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    text = '1' + text;
  }
  return text;
}

export function base58Decode(text) {
  let value = 0n;
  for (const character of text) {
    const digit = BASE58.indexOf(character);
    if (digit < 0) throw new Error(`not base58: ${text}`);
    value = value * 58n + BigInt(digit);
  }
  const hex = value === 0n ? '' : value.toString(16);
  const leadingZeros = text.length - text.replace(/^1+/, '').length;
  const body = Buffer.from(hex.length % 2 ? `0${hex}` : hex, 'hex');
  return Buffer.concat([Buffer.alloc(leadingZeros), body]);
}

export function isAddress(text) {
  try {
    return typeof text === 'string' && base58Decode(text).length === 32;
  } catch {
    return false;
  }
}

export const sha256Hex = (bytes) => createHash('sha256').update(bytes).digest('hex');
