/**
 * The snapshot tool's `route` decoding and v0 size estimate:
 *
 *   node --test 'scripts/snapshot/*.test.mjs'
 */
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { ROUTE_DISCRIMINATOR, splitRouteData, templateRunSize, v0TransactionSize } from './jupiter.mjs';
import { decodeLookupTable, parseJson } from './rpc.mjs';

const SNAPSHOT = fileURLToPath(new URL('../../tests/protocols/snapshot/', import.meta.url));
const routes = parseJson(readFileSync(`${SNAPSHOT}routes.json`, 'utf8'));
const accounts = new Map(
  parseJson(readFileSync(`${SNAPSHOT}accounts.json`, 'utf8')).map((entry) => [entry.address, entry]),
);
const tablesOf = (legs) =>
  [...new Set(legs.flatMap((leg) => leg.addressLookupTableAddresses))].map((address) => ({
    address,
    addresses: decodeLookupTable(Buffer.from(accounts.get(address).data, 'base64')).addresses,
  }));
const legs = Object.entries(routes.routes).flatMap(([name, route]) =>
  route.legs.map((leg, index) => ({ label: `${name}[${index}]`, leg })),
);

test('every committed route is `route`, and splitRouteData recovers its arguments', () => {
  assert.equal(ROUTE_DISCRIMINATOR, 'e517cb977ae3ad2a'); // sha256("global:route")[..8]
  for (const { label, leg } of legs) {
    const data = Buffer.from(leg.instructions.swap.data, 'base64');
    const route = splitRouteData(data);
    assert.equal(route.discriminator, ROUTE_DISCRIMINATOR, label);
    assert.equal(route.argsHex, data.subarray(8).toString('hex'), label);
    assert.ok(route.argsHex.startsWith(route.routePlanHex), label);
    assert.equal(route.inAmount, BigInt(leg.inAmount), label);
    assert.equal(route.quotedOutAmount, BigInt(leg.outAmount), label);
    assert.equal(route.slippageBps, leg.slippageBps, label);
    assert.equal(route.platformFeeBps, 0, label);
  }
  assert.throws(() => splitRouteData(Buffer.alloc(30)), /too short/);
});

test('v0TransactionSize matches hand-computed sizes', () => {
  const payer = 'payer';
  const instructions = [
    { programId: 'program', accounts: [{ pubkey: 'a', isSigner: false, isWritable: true }], data: 'AQID' },
  ];
  // Signature 1 + 64; prefix and header 4; 3 static keys 1 + 96; blockhash 32;
  // one instruction 1 + (1 + 1 + 1 + 1 + 3); no lookups 1.
  assert.deepEqual(v0TransactionSize({ payer, instructions, tables: [] }), {
    bytes: 207,
    staticKeys: 3,
    lookedUpKeys: 0,
    tablesUsed: 0,
  });
  // `a` moves to a table: 32 bytes fewer static, 32 + 1 + 1 + 1 for the lookup.
  assert.deepEqual(v0TransactionSize({ payer, instructions, tables: [{ address: 't', addresses: ['a'] }] }), {
    bytes: 210,
    staticKeys: 2,
    lookedUpKeys: 1,
    tablesUsed: 1,
  });
  // Signers and invoked programs stay static even when a table holds them.
  const unusable = { address: 't', addresses: [payer, 'program'] };
  assert.equal(v0TransactionSize({ payer, instructions, tables: [unusable] }).bytes, 207);
});

test('the size estimates in routes.json are what the estimator computes', () => {
  for (const { label, leg } of legs) {
    const { computeBudget, setup, tokenLedger, swap, cleanup, other } = leg.instructions;
    const instructions = [...computeBudget, ...setup, tokenLedger, swap, cleanup, ...other].filter(Boolean);
    const size = v0TransactionSize({ payer: routes.wallet.address, instructions, tables: tablesOf([leg]) });
    assert.deepEqual(size, leg.transaction, label);
  }
  for (const [name, route] of Object.entries(routes.routes)) {
    const { spare, ...estimate } = route.templateRun;
    const size = templateRunSize({ payer: routes.wallet.address, legs: route.legs, tables: tablesOf(route.legs) });
    assert.deepEqual(size, estimate, name);
    assert.equal(spare, 1232 - size.bytes, name);
  }
});
