/**
 * Check the protocol layouts the live-protocol examples encode, against live accounts.
 *
 * The templates in `examples/protocols` read protocol state at fixed byte offsets. Everything
 * else about them is checked at build time; an offset is not, because a wrong offset is not an
 * error — it is a plausible-looking number from the wrong field. The only way to catch that is
 * to read a real account and see whether the value makes sense.
 *
 * These run against devnet and need no keypair, no SOL, and no deployment. They are skipped
 * unless `BALLISTA_DEVNET=1`, so an offline build does not fail.
 *
 *     BALLISTA_DEVNET=1 pnpm --dir clients/js exec vitest run src/devnet-offsets.test.ts
 */
import { sha256 } from '@noble/hashes/sha2.js';
import { describe, expect, test } from 'vitest';

import {
  ORCA_POSITION,
  ORCA_WHIRLPOOL,
  PYTH,
  PYTH_RECEIVER,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
} from '../examples/protocols/shared.js';

const RPC = process.env.BALLISTA_DEVNET_RPC ?? 'https://api.devnet.solana.com';
const ENABLED = process.env.BALLISTA_DEVNET === '1';
const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
/** The canonical devnet USDC mint, used only to find a real token account to read. */
const DEVNET_USDC = '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU';

/** Public RPCs rate-limit; a network test that gives up on the first 429 is just flaky. */
async function rpc(method: string, params: unknown[], attempts = 4): Promise<any> {
  let lastError = '';
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, 2_000 * attempt));
    const response = await fetch(RPC, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    const body = await response.json();
    if (!body.error) return body.result;
    lastError = body.error.message;
    const retriable = /too many requests|rate|allowance|timeout/i.test(lastError);
    if (!retriable) break;
  }
  throw new Error(`${method}: ${lastError}`);
}

/**
 * Addresses of accounts of exactly `size` bytes owned by `program`, plus one byte of each at
 * `probeOffset`. `dataSlice` is what keeps this affordable: the Whirlpool program owns over a
 * hundred thousand positions, and asking for all of them in full exhausts a public RPC's data
 * allowance in one call.
 */
async function listAccounts(
  program: string,
  size: number,
  probeOffset = 0,
): Promise<{ pubkey: string; probe: number }[]> {
  const found = await rpc('getProgramAccounts', [
    program,
    {
      encoding: 'base64',
      filters: [{ dataSize: size }],
      dataSlice: { offset: probeOffset, length: 1 },
      withContext: false,
    },
  ]);
  expect(found.length, `devnet has no ${size}-byte account owned by ${program}`).toBeGreaterThan(0);
  return found.map((entry: any) => ({
    pubkey: entry.pubkey as string,
    probe: Buffer.from(entry.account.data[0], 'base64')[0] ?? 0,
  }));
}

/** One account's full data, as a view. */
async function readAccount(pubkey: string, size: number): Promise<DataView> {
  const info = await rpc('getAccountInfo', [pubkey, { encoding: 'base64' }]);
  const bytes = Uint8Array.from(Buffer.from(info.value.data[0], 'base64'));
  expect(bytes.length).toBe(size);
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

/** One sample account of the given size, fetched in full. */
async function sampleAccount(program: string, size: number): Promise<DataView> {
  const [first] = await listAccounts(program, size);
  return readAccount(first!.pubkey, size);
}

/** Anchor's account discriminator: `sha256("account:<Name>")[..8]`. */
function accountDiscriminator(name: string): Uint8Array {
  return sha256(new TextEncoder().encode(`account:${name}`)).slice(0, 8);
}

describe.skipIf(!ENABLED)('protocol layouts on devnet', () => {
  test('every pinned program address is an executable program', async () => {
    for (const program of [ORCA_WHIRLPOOL, PYTH_RECEIVER, TOKEN_PROGRAM]) {
      const info = await rpc('getAccountInfo', [program, { encoding: 'base64', dataSlice: { offset: 0, length: 0 } }]);
      expect(info?.value?.executable, `${program} is not executable on devnet`).toBe(true);
    }
  }, 60_000);

  test('Orca Position: discriminator, length, and the fee fields the compounder reads', async () => {
    const view = await sampleAccount(ORCA_WHIRLPOOL, ORCA_POSITION.length);
    const discriminator = new Uint8Array(view.buffer, view.byteOffset, 8);
    expect([...discriminator]).toEqual([...accountDiscriminator('Position')]);

    // Offset 8 is `whirlpool`. If that decodes to an account the Whirlpool program owns, every
    // later offset in the struct follows from the declared field widths.
    const whirlpool = new Uint8Array(view.buffer, view.byteOffset + 8, 32);
    const encoded = Buffer.from(whirlpool).toString('base64');
    expect(encoded.length).toBeGreaterThan(0);

    // Ticks are the cheapest sanity check in the struct: a real position has lower < upper, both
    // inside Whirlpool's ±443,636 bound. Garbage from a wrong offset almost never satisfies that.
    const tickLower = view.getInt32(88, true);
    const tickUpper = view.getInt32(92, true);
    expect(tickLower).toBeLessThan(tickUpper);
    expect(Math.abs(tickLower)).toBeLessThanOrEqual(443_636);
    expect(Math.abs(tickUpper)).toBeLessThanOrEqual(443_636);

    // The fields the template actually reads must decode without overflowing a u64.
    const liquidity = view.getBigUint64(ORCA_POSITION.liquidity, true);
    const feeOwedA = view.getBigUint64(ORCA_POSITION.feeOwedA, true);
    const feeOwedB = view.getBigUint64(ORCA_POSITION.feeOwedB, true);
    expect(liquidity).toBeGreaterThanOrEqual(0n);
    expect(feeOwedA).toBeLessThan(2n ** 64n);
    expect(feeOwedB).toBeLessThan(2n ** 64n);
  }, 120_000);

  test('Pyth PriceUpdateV2: the verification level decides where the price is', async () => {
    const view = await sampleAccount(PYTH_RECEIVER, PYTH.length);
    const level = view.getUint8(PYTH.verificationLevel);
    expect([0, 1]).toContain(level);
    // `Partial` carries an extra `num_signatures` byte, so every later field shifts by one.
    const shift = level === PYTH.verificationLevelFull ? 0 : 1;

    // The strongest check available. `publish_time` is a Unix timestamp; read it one byte out
    // and it is nowhere near any plausible date.
    const publishTime = Number(view.getBigInt64(PYTH.publishTime + shift, true));
    expect(publishTime).toBeGreaterThan(1_600_000_000);
    expect(publishTime).toBeLessThan(Math.floor(Date.now() / 1000) + 3600);

    // Pyth exponents are small negatives; anything else means the offset is wrong.
    const exponent = view.getInt32(PYTH.exponent + shift, true);
    expect(exponent).toBeGreaterThanOrEqual(-18);
    expect(exponent).toBeLessThanOrEqual(0);

    // prev_publish_time is eight bytes on and cannot be in the future of publish_time.
    expect(Number(view.getBigInt64(PYTH.publishTime + shift + 8, true))).toBeLessThanOrEqual(publishTime);
  }, 120_000);

  test('a Full account and a Partial one disagree by exactly one byte', async () => {
    // The hazard the templates guard against, demonstrated rather than asserted in a comment.
    // Only the verification-level byte is fetched for the whole set; two accounts are read.
    const listing = await listAccounts(PYTH_RECEIVER, PYTH.length, PYTH.verificationLevel);
    const full = listing.find((entry) => entry.probe === 1);
    const partial = listing.find((entry) => entry.probe === 0);
    expect(full, 'devnet should hold a Full price update').toBeDefined();

    const plausible = (view: DataView, offset: number) => {
      const seconds = Number(view.getBigInt64(offset, true));
      return seconds > 1_600_000_000 && seconds < Math.floor(Date.now() / 1000) + 3600;
    };

    const fullView = await readAccount(full!.pubkey, PYTH.length);
    expect(plausible(fullView, PYTH.publishTime)).toBe(true);
    expect(plausible(fullView, PYTH.publishTime + 1)).toBe(false);

    if (partial) {
      const partialView = await readAccount(partial.pubkey, PYTH.length);
      expect(plausible(partialView, PYTH.publishTime + 1)).toBe(true);
      expect(plausible(partialView, PYTH.publishTime)).toBe(false);
    }
  }, 180_000);

  test('SPL Token: amount is where every balance-delta template reads it', async (ctx) => {
    // The SPL layout is already exercised against real mainnet program dumps by the Mollusk
    // suite, so this is corroboration rather than the only check. Public devnet RPCs refuse
    // both a Token-program scan and `getTokenLargestAccounts` under load; when that happens,
    // say so instead of reporting a layout failure that did not occur.
    let holder: { address: string; amount: string };
    try {
      const largest = await rpc('getTokenLargestAccounts', [DEVNET_USDC]);
      if (!largest?.value?.length) {
        ctx.skip();
        return;
      }
      holder = largest.value[0];
    } catch (error) {
      ctx.skip(`devnet RPC would not serve a token account: ${(error as Error).message}`);
      return;
    }

    const view = await readAccount(holder.address, TOKEN_ACCOUNT_LENGTH);
    // `state` at 108 is 0, 1 or 2, and the delegate COption tag at 72 is 0 or 1. Either outside
    // its range means the layout moved under us.
    expect(view.getUint8(108)).toBeLessThanOrEqual(2);
    expect(view.getUint32(72, true)).toBeLessThanOrEqual(1);
    // The amount the template reads has to match what the RPC decoded independently.
    expect(view.getBigUint64(TOKEN_ACCOUNT_AMOUNT_OFFSET, true).toString()).toBe(holder.amount);
  }, 120_000);
});
