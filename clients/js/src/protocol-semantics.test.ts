/**
 * What the live-protocol examples mean, not just that they compile.
 *
 * The protocols these templates call cannot run in CI, so these tests read the template documents
 * directly: which accounts reach Jupiter in which position, and which on-chain reads a guarantee
 * actually depends on. Each one pins a mistake an example once made.
 */
import { getAddressDecoder, type Address } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  type AccountReference,
  type Expression,
  type Step,
  type Template,
} from './index.js';
import * as protocols from '../examples/protocols/index.js';
import {
  driftSettleWhenProfitable,
  jitoProfitGuardedTip,
  jupiterDepositExactOutput,
  jupiterOracleCheckedSwap,
  kaminoRepaySwapOutput,
  pythFreshPriceGate,
  tokenSweepIntoSwap,
} from '../examples/protocols/index.js';
import { buildJupiterDepositRun } from '../examples/protocols/run-jupiter-deposit.js';
import {
  BORSH_TRUE,
  DRIFT_WITHDRAW,
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_REPAY,
  PYTH,
  SPL_MINT,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  addressBytes,
  anchorDiscriminator,
} from '../examples/protocols/shared.js';

type Invoke = Extract<Step, { kind: 'invoke' }>;
type Require = Extract<Step, { kind: 'require' }>;

function steps(template: Template): Step[] {
  const all: Step[] = [];
  const visit = (list: Step[]) => {
    for (const step of list) {
      all.push(step);
      if (step.kind === 'forEach') visit(step.steps);
    }
  };
  visit(template.steps);
  return all;
}

function invokesOf(template: Template, program: string): Invoke[] {
  return steps(template).filter(
    (step): step is Invoke =>
      step.kind === 'invoke' && step.program.kind === 'account' && step.program.name === program,
  );
}

function requireLabeled(template: Template, label: string): Require {
  const found = steps(template).find(
    (step): step is Require => step.kind === 'require' && step.label === label,
  );
  if (!found) throw new Error(`no requirement labeled ${label}`);
  return found;
}

/** Every binding in the template, so a dependency can be followed through variables. */
function bindingsOf(template: Template): Map<string, Expression[]> {
  const bindings = new Map<string, Expression[]>();
  for (const step of steps(template)) {
    if (step.kind === 'let' || step.kind === 'assign') {
      bindings.set(step.name, [...(bindings.get(step.name) ?? []), step.value]);
    }
  }
  return bindings;
}

/** Whether `expression` reads, directly or through bindings, something `matches` accepts. */
function dependsOn(
  expression: Expression,
  bindings: Map<string, Expression[]>,
  matches: (candidate: Expression) => boolean,
  seen = new Set<string>(),
): boolean {
  if (matches(expression)) return true;
  const recurse = (inner: Expression) => dependsOn(inner, bindings, matches, seen);
  switch (expression.kind) {
    case 'variable': {
      if (seen.has(expression.name)) return false;
      seen.add(expression.name);
      return (bindings.get(expression.name) ?? []).some(recurse);
    }
    case 'binary':
      return recurse(expression.left) || recurse(expression.right);
    case 'not':
    case 'cast':
      return recurse(expression.value);
    case 'select':
      return (
        recurse(expression.condition) || recurse(expression.ifTrue) || recurse(expression.ifFalse)
      );
    case 'accountData':
      return typeof expression.offset !== 'number' && recurse(expression.offset);
    case 'pda':
      return (
        expression.seeds.some(recurse) || (expression.bump !== undefined && recurse(expression.bump))
      );
    case 'multiplyDivide':
      return recurse(expression.left) || recurse(expression.right) || recurse(expression.divisor);
    case 'powerOfTen':
      return recurse(expression.exponent);
    default:
      return false;
  }
}

const reads = (name: string, offset: number) => (candidate: Expression) =>
  candidate.kind === 'accountData' &&
  candidate.account.kind === 'account' &&
  candidate.account.name === name &&
  candidate.offset === offset;

const nameOf = (reference: AccountReference) => reference.name;

/**
 * Jupiter v6 `route` starts its account list with `tokenProgram`, `userTransferAuthority` (the
 * one signer), `userSourceTokenAccount` and `userDestinationTokenAccount`. A template that
 * measures the swap's token accounts passes all four itself; one that does not passes the first
 * two and lets the token accounts travel in the group.
 */
const jupiterCalls: [string, Template, { program: string; accounts: string[] }][] = [
  [
    'jupiterDepositExactOutput',
    jupiterDepositExactOutput,
    { program: 'jupiter', accounts: ['tokenProgram', 'owner', 'sourceAta', 'destinationAta'] },
  ],
  [
    'jupiterOracleCheckedSwap',
    jupiterOracleCheckedSwap,
    { program: 'jupiter', accounts: ['tokenProgram', 'trader', 'sourceAta', 'destinationAta'] },
  ],
  [
    'kaminoRepaySwapOutput',
    kaminoRepaySwapOutput,
    { program: 'jupiter', accounts: ['tokenProgram', 'borrower', 'collateralAta', 'borrowedAssetAta'] },
  ],
  [
    'tokenSweepIntoSwap',
    tokenSweepIntoSwap,
    { program: 'jupiter', accounts: ['tokenProgram', 'seller', 'sourceAta', 'destinationAta'] },
  ],
  ['pythFreshPriceGate', pythFreshPriceGate, { program: 'actionProgram', accounts: ['tokenProgram', 'actor'] }],
  [
    'jitoProfitGuardedTip',
    jitoProfitGuardedTip,
    { program: 'strategyProgram', accounts: ['tokenProgram', 'searcher'] },
  ],
];

describe('Jupiter calls are `route`, with its accounts in its order', () => {
  test('every example that pins Jupiter is listed here', () => {
    const jupiter = [...addressBytes(JUPITER_V6)];
    const pinning = Object.entries(protocols)
      .filter(([, template]) =>
        Object.values(template.accounts).some(
          (constraint) => constraint.address !== undefined && [...constraint.address].join() === jupiter.join(),
        ),
      )
      .map(([name]) => name)
      .sort();
    expect(pinning).toEqual(jupiterCalls.map(([name]) => name).sort());
  });

  test.each(jupiterCalls)('%s', (_, template, expected) => {
    const calls = invokesOf(template, expected.program);
    expect(calls).toHaveLength(1);
    const [call] = calls as [Invoke];

    // The discriminator is pinned, so the fixed positions below are the ones `route` defines.
    const [discriminator] = call.data;
    expect(discriminator?.kind).toBe('literal');
    expect(discriminator?.kind === 'literal' ? [...discriminator.bytes] : []).toEqual([...JUPITER_ROUTE]);

    expect(call.accounts.map((entry) => nameOf(entry.account))).toEqual(expected.accounts);
    expect(template.accounts.tokenProgram?.address).toEqual(TOKEN_PROGRAM_ADDRESS_BYTES);
    expect(call.accounts.map((entry) => entry.signer)).toEqual(
      expected.accounts.map((_, index) => index === 1),
    );
    // Everything after the accounts the template passes is the route's own and arrives as a group.
    expect(call.accountGroup).toBeDefined();
  });
});

describe('the oracle-checked swap', () => {
  const bindings = bindingsOf(jupiterOracleCheckedSwap);
  const check = requireLabeled(jupiterOracleCheckedSwap, 'fillBeatTheOracle');

  test('bounds the fill by the Pyth price', () => {
    expect(dependsOn(check.condition, bindings, reads('priceUpdate', PYTH.price))).toBe(true);
  });

  test('prices what was actually sold, not what the caller said it would sell', () => {
    expect(
      dependsOn(check.condition, bindings, reads('sourceAta', TOKEN_ACCOUNT_AMOUNT_OFFSET)),
    ).toBe(true);
  });

  test("pins the feed's exponent, which decides what the price means", () => {
    const pinned = steps(jupiterOracleCheckedSwap).some(
      (step) =>
        step.kind === 'require' && dependsOn(step.condition, bindings, reads('priceUpdate', PYTH.exponent)),
    );
    expect(pinned).toBe(true);
  });

  test('scales by both mints’ decimals, read on chain rather than supplied', () => {
    expect(dependsOn(check.condition, bindings, reads('sourceMint', SPL_MINT.decimals))).toBe(true);
    expect(dependsOn(check.condition, bindings, reads('destinationMint', SPL_MINT.decimals))).toBe(true);
    expect(Object.keys(jupiterOracleCheckedSwap.inputs ?? {})).not.toContain('scaleDivisor');
  });
});

describe('the token sweep', () => {
  const bindings = bindingsOf(tokenSweepIntoSwap);
  const [swap] = invokesOf(tokenSweepIntoSwap, 'jupiter') as [Invoke];
  const sourceBalance = reads('sourceAta', TOKEN_ACCOUNT_AMOUNT_OFFSET);

  // `route(route_plan, in_amount, quoted_out_amount, slippage_bps, platform_fee_bps)`, after the
  // discriminator.
  test('sells the balance it read, not the amount the route was quoted for', () => {
    const inAmount = swap.data[2];
    expect(inAmount?.kind === 'encoded' ? inAmount.encoding : undefined).toBe('u64');
    expect(inAmount?.kind === 'encoded' && dependsOn(inAmount.value, bindings, sourceBalance)).toBe(
      true,
    );
  });

  test('rescales the quote to the amount it sells', () => {
    const quotedOut = swap.data[3];
    expect(quotedOut?.kind === 'encoded' ? quotedOut.encoding : undefined).toBe('u64');
    expect(
      quotedOut?.kind === 'encoded' && dependsOn(quotedOut.value, bindings, sourceBalance),
    ).toBe(true);
  });
});

describe('the Drift settle', () => {
  test('withdraws with reduce_only set, so it can never open a borrow', () => {
    const [withdraw] = invokesOf(driftSettleWhenProfitable, 'drift').filter(
      (call) => call.data[0]?.kind === 'literal' && [...call.data[0].bytes].join() === [...DRIFT_WITHDRAW].join(),
    );
    expect(withdraw).toBeDefined();
    // `withdraw(market_index: u16, amount: u64, reduce_only: bool)`: the flag is the last part.
    const reduceOnly = withdraw!.data.at(-1);
    expect(reduceOnly?.kind === 'literal' ? [...reduceOnly.bytes] : []).toEqual([...BORSH_TRUE]);
  });
});

describe('the Kamino repay', () => {
  test('repays exactly what the swap produced', () => {
    const [repay] = invokesOf(kaminoRepaySwapOutput, 'kamino').filter(
      (call) => call.data[0]?.kind === 'literal' && [...call.data[0].bytes].join() === [...KAMINO_REPAY].join(),
    );
    expect(repay?.data[1]).toEqual({ kind: 'encoded', encoding: 'u64', value: { kind: 'variable', name: 'swapped' } });
  });
});

describe('the Jupiter deposit runner', () => {
  const decoder = getAddressDecoder();
  const key = (byte: number): Address => decoder.decode(new Uint8Array(32).fill(byte));
  const tokenProgram = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
  const owner = key(7);
  const kamino = {
    owner,
    destinationAta: key(9),
    obligation: key(10),
    lendingMarket: key(11),
    lendingMarketAuthority: key(12),
    reserve: key(13),
    reserveLiquiditySupply: key(14),
    reserveCollateralMint: key(15),
    reserveDestinationDepositCollateral: key(16),
  };
  // A `route` with an empty plan: the discriminator, a u32 zero, then the 19-byte tail.
  const data = Buffer.from([...JUPITER_ROUTE, 0, 0, 0, 0, ...new Uint8Array(19)]).toString('base64');
  const routeAccounts = [
    { pubkey: tokenProgram, isSigner: false, isWritable: false },
    { pubkey: owner, isSigner: true, isWritable: true },
    { pubkey: key(8), isSigner: false, isWritable: true },
    { pubkey: kamino.destinationAta, isSigner: false, isWritable: true },
    { pubkey: key(17), isSigner: false, isWritable: false },
    { pubkey: key(18), isSigner: false, isWritable: true },
  ];

  test("forwards only what follows route's first four accounts as the group", async () => {
    const instruction = await buildJupiterDepositRun({
      creator: owner,
      templateId: 0,
      swap: { programId: JUPITER_V6, accounts: routeAccounts, data },
      kamino,
      minimumOut: 1n,
    });
    const addresses = (instruction.accounts ?? []).map((meta) => meta.address);
    expect(addresses.slice(-2)).toEqual([key(17), key(18)]);
    // The first four reach the template as declared accounts, not again through the group.
    for (const declared of [tokenProgram, key(8), kamino.destinationAta]) {
      expect(addresses.filter((entry) => entry === declared)).toHaveLength(1);
    }
  });

  test('refuses a list that does not start the way `route` does', async () => {
    // `shared_accounts_route` puts a program authority second and the user third.
    const shared = [routeAccounts[0]!, { pubkey: key(20), isSigner: false, isWritable: false }, ...routeAccounts.slice(1)];
    await expect(
      buildJupiterDepositRun({
        creator: owner,
        templateId: 0,
        swap: { programId: JUPITER_V6, accounts: shared, data },
        kamino,
        minimumOut: 1n,
      }),
    ).rejects.toThrow(/route/);
  });

  test('refuses data that is not `route`', async () => {
    const other = Buffer.from([...anchorDiscriminator('shared_accounts_route'), 0, 0, 0, 0, ...new Uint8Array(20)]);
    await expect(
      buildJupiterDepositRun({
        creator: owner,
        templateId: 0,
        swap: { programId: JUPITER_V6, accounts: routeAccounts, data: other.toString('base64') },
        kamino,
        minimumOut: 1n,
      }),
    ).rejects.toThrow(/useSharedAccounts/);
  });
});
