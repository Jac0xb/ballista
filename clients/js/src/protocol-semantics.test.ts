/**
 * What the live-protocol examples mean, not just that they compile.
 *
 * The protocols these templates call cannot run in CI, so these tests read the template documents
 * directly: which accounts reach Jupiter in which position, and which on-chain reads a guarantee
 * actually depends on. Each one pins a mistake an example once made.
 */
import { isDeepStrictEqual } from 'node:util';

import { AccountRole, address, getAddressDecoder, isWritableRole, type Address } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import {
  ED25519_PROGRAM_ADDRESS_BYTES,
  TOKEN_2022_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  expression,
  type AccountReference,
  type Expression,
  type ReadType,
  type Step,
  type Template,
} from './index.js';
import * as protocols from '../examples/protocols/index.js';
import {
  jupiterDailyCapSwap,
  jupiterDepositExactOutput,
  jupiterOracleCheckedSwap,
  kaminoLiquidateWithProof,
  kaminoRepaySwapOutput,
  orcaCompoundFees,
  orcaHarvestManyPositions,
  pumpFunBuyBasket,
  pumpFunSellAll,
  pythFreshPriceGate,
  signedQuoteSettlement,
  tokenSweepIntoSwap,
} from '../examples/protocols/index.js';
import { QUOTE, QUOTE_TAG, signedQuote } from '../examples/protocols/signed-quote-settlement.js';
import * as dailyCapModule from '../examples/protocols/jupiter-daily-cap-swap.js';
import * as depositModule from '../examples/protocols/jupiter-deposit-exact-output.js';
import * as oracleSwapModule from '../examples/protocols/jupiter-oracle-checked-swap.js';
import * as repayModule from '../examples/protocols/kamino-repay-swap-output.js';
import * as pythGateModule from '../examples/protocols/pyth-fresh-price-gate.js';
import * as sweepModule from '../examples/protocols/token-sweep-into-swap.js';
import { jupiterOracleCheckedSwapFeeCap100 } from '../examples/scenarios/index.js';
import { RAISED_PLATFORM_FEE_BPS } from '../examples/scenarios/raised-fee-cap.js';
import {
  MAX_PLATFORM_FEE_BPS as SPLIT_SELL_MAX_PLATFORM_FEE_BPS,
  splitSellInner,
  splitSellPayout,
} from '../examples/scenarios/split-sell.js';
import { buildJupiterDepositRun } from '../examples/protocols/run-jupiter-deposit.js';
import { buildDailyCapRun } from '../examples/protocols/run/jupiter-daily-cap.js';
import { buildPumpBuyBasketRun } from '../examples/protocols/run/pump-buy-basket.js';
import { pumpCoinAccounts, pumpUserVolumeAccumulator, token2022Ata } from '../examples/protocols/run/pump-fun.js';
import { buildPumpSellAllRun } from '../examples/protocols/run/pump-sell-all.js';
import {
  buildOrcaHarvestRun,
  describeFailure,
  getOrcaTickArrayAddress,
} from '../examples/protocols/run-orca-harvest.js';
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_DEPOSIT,
  KAMINO_FARMS,
  KAMINO_LEND,
  KAMINO_LIQUIDATE,
  KAMINO_REPAY,
  PUMP_BONDING_CURVE,
  PUMP_FEES,
  PUMP_FUN,
  PUMP_FUN_BUY,
  PUMP_FUN_SELL,
  PYTH,
  PYTH_RECEIVER,
  SPL_MINT,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  USDC_MINT,
  WRAPPED_SOL_MINT,
  addressBytes,
  anchorDiscriminator,
} from '../examples/protocols/shared.js';
import { findRegistryEntryAddress } from './kit.js';

/** Each Jupiter template's module, by export name, for the constants beside it. */
const protocolModules: Record<string, { MAX_PLATFORM_FEE_BPS?: bigint }> = {
  jupiterDailyCapSwap: dailyCapModule,
  jupiterDepositExactOutput: depositModule,
  jupiterOracleCheckedSwap: oracleSwapModule,
  kaminoRepaySwapOutput: repayModule,
  pythFreshPriceGate: pythGateModule,
  tokenSweepIntoSwap: sweepModule,
};

type Invoke = Extract<Step, { kind: 'invoke' }>;
type Require = Extract<Step, { kind: 'require' }>;

function steps(template: Template): Step[] {
  const all: Step[] = [];
  const visit = (list: Step[]) => {
    for (const step of list) {
      all.push(step);
      if (step.kind === 'forEach' || step.kind === 'repeat') visit(step.steps);
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
    case 'instruction':
      return recurse(expression.index);
    case 'instructionAccount':
      return recurse(expression.index) || recurse(expression.position);
    case 'instructionData':
    case 'instructionDataBytes':
      return recurse(expression.index) || recurse(expression.offset);
    case 'accountDataBytes':
      return recurse(expression.offset);
    case 'bytesLength':
      return recurse(expression.value);
    default:
      return false;
  }
}

const reads = (name: string, offset: number) => (candidate: Expression) =>
  candidate.kind === 'accountData' &&
  candidate.account.kind === 'account' &&
  candidate.account.name === name &&
  candidate.offset === offset;

const accountKey = (name: string) => (candidate: Expression) =>
  candidate.kind === 'accountField' &&
  candidate.account.kind === 'account' &&
  candidate.account.name === name &&
  candidate.field === 'key';

const nameOf = (reference: AccountReference) => reference.name;

/**
 * Jupiter v6 `route` starts its account list with `tokenProgram`, `userTransferAuthority` (the
 * one signer), `userSourceTokenAccount` and `userDestinationTokenAccount`. A template passes
 * itself the ones it reads, up to all four, and lets the rest travel in the group.
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
    // It passes the source it checks, and lets the destination travel in the group.
    'jupiterDailyCapSwap',
    jupiterDailyCapSwap,
    { program: 'actionProgram', accounts: ['tokenProgram', 'actor', 'sourceAta'] },
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

/**
 * Jupiter's `route` pays `platform_fee_bps` of its output to the platform fee account, position 6
 * of its list, which every template forwards in its group. Whoever builds the run picks both, so
 * each template caps the rate at a constant of its own, 0 unless its author raises it, before the
 * call.
 */
describe('every Jupiter route caps the platform fee', () => {
  const capped: [string, Template, string, bigint][] = [
    ...jupiterCalls.map(
      ([name, template, { program }]) =>
        [name, template, program, (protocolModules[name] ?? {}).MAX_PLATFORM_FEE_BPS] as [string, Template, string, bigint],
    ),
    ['splitSellPayout', splitSellPayout, 'jupiter', SPLIT_SELL_MAX_PLATFORM_FEE_BPS],
    ['splitSellInner', splitSellInner, 'jupiter', SPLIT_SELL_MAX_PLATFORM_FEE_BPS],
  ];

  test.each(capped)('%s takes the fee as an input, writes it last, and caps it before the call', (_, template, program, cap) => {
    expect(cap).toBe(0n);
    expect(template.inputs?.platformFeeBps).toEqual({ type: 'u64' });
    const [call] = invokesOf(template, program) as [Invoke];
    expect(call.data.at(-1)).toEqual(data.encode('u8', expression.input('platformFeeBps')));

    const check = requireLabeled(template, 'platformFeeWithinCap');
    expect(check.condition).toEqual(expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(cap)));
    const all = steps(template);
    expect(all.indexOf(check)).toBeLessThan(all.indexOf(call));
  });

  test('the raised-cap scenario differs from the oracle swap only in its cap', () => {
    const raised = requireLabeled(jupiterOracleCheckedSwapFeeCap100, 'platformFeeWithinCap');
    expect(raised.condition).toEqual(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(RAISED_PLATFORM_FEE_BPS)),
    );
    const others = (template: Template) => template.steps.filter((step) => step.label !== 'platformFeeWithinCap');
    expect(others(jupiterOracleCheckedSwapFeeCap100)).toEqual(others(jupiterOracleCheckedSwap));
    expect({ ...jupiterOracleCheckedSwapFeeCap100, steps: [] }).toEqual({ ...jupiterOracleCheckedSwap, steps: [] });
  });
});

describe('the oracle-checked swap', () => {
  const bindings = bindingsOf(jupiterOracleCheckedSwap);
  const check = requireLabeled(jupiterOracleCheckedSwap, 'fillBeatTheOracle');

  // A run input could name another feed, another pair or a wider tolerance: whoever builds the run
  // would choose what the fill is checked against.
  test('fixes the feed, the pair it prices and the tolerance in the template, not the run', () => {
    expect(Object.keys(jupiterOracleCheckedSwap.inputs ?? {})).toEqual([
      'routePlan',
      'inAmount',
      'quotedOutAmount',
      'slippageBps',
      'platformFeeBps',
    ]);
    expect([...oracleSwapModule.FEED_ID]).toEqual([
      ...Buffer.from('ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d', 'hex'),
    ]);
    expect(jupiterOracleCheckedSwap.accounts.sourceMint?.address).toEqual(addressBytes(WRAPPED_SOL_MINT));
    expect(jupiterOracleCheckedSwap.accounts.destinationMint?.address).toEqual(addressBytes(USDC_MINT));
    expect(oracleSwapModule.TOLERANCE_BPS).toBe(100n);
    const fairOut = bindings.get('fairOut')?.[0];
    expect(fairOut?.kind === 'cast' ? fairOut.value : undefined).toMatchObject({
      kind: 'multiplyDivide',
      right: expression.u128(10_000n - oracleSwapModule.TOLERANCE_BPS),
      divisor: expression.u128(10_000),
    });
  });

  test('bounds the fill by the Pyth price', () => {
    expect(dependsOn(check.condition, bindings, reads('priceUpdate', PYTH.price))).toBe(true);
  });

  test('prices what was actually sold, not what the caller said it would sell', () => {
    expect(
      dependsOn(check.condition, bindings, reads('sourceAta', TOKEN_ACCOUNT_AMOUNT_OFFSET)),
    ).toBe(true);
  });

  test('scales by both mints’ decimals, read on chain rather than supplied', () => {
    expect(dependsOn(check.condition, bindings, reads('sourceMint', SPL_MINT.decimals))).toBe(true);
    expect(dependsOn(check.condition, bindings, reads('destinationMint', SPL_MINT.decimals))).toBe(true);
    expect(Object.keys(jupiterOracleCheckedSwap.inputs ?? {})).not.toContain('scaleDivisor');
  });

  test('closes the mint-pairing hole: each ATA is checked against the mint it scales by', () => {
    const sourceCheck = requireLabeled(jupiterOracleCheckedSwap, 'sourceHoldsTheSourceMint');
    expect(sourceCheck).toBeDefined();
    expect(
      dependsOn(sourceCheck.condition, bindings, reads('sourceAta', TOKEN_ACCOUNT_MINT_OFFSET)),
    ).toBe(true);
    expect(dependsOn(sourceCheck.condition, bindings, accountKey('sourceMint'))).toBe(true);

    const destinationCheck = requireLabeled(jupiterOracleCheckedSwap, 'destinationHoldsTheDestinationMint');
    expect(destinationCheck).toBeDefined();
    expect(
      dependsOn(destinationCheck.condition, bindings, reads('destinationAta', TOKEN_ACCOUNT_MINT_OFFSET)),
    ).toBe(true);
    expect(dependsOn(destinationCheck.condition, bindings, accountKey('destinationMint'))).toBe(true);
  });

  // Jupiter moves the accounts its steps name. Of `route`'s source it asks only a balance of at
  // least `in_amount`, so other accounts there leave both measured balances still, and a floor
  // valued on nothing sold passes any fill.
  test('writes the route’s `in_amount` itself, from the input it holds the source to', () => {
    const [swap] = invokesOf(jupiterOracleCheckedSwap, 'jupiter') as [Invoke];
    expect(swap.data).toEqual([
      data.literal(JUPITER_ROUTE),
      data.encode('bytes', expression.input('routePlan')),
      data.encode('u64', expression.input('inAmount')),
      data.encode('u64', expression.input('quotedOutAmount')),
      data.encode('u16', expression.input('slippageBps')),
      data.encode('u8', expression.input('platformFeeBps')),
    ]);
  });

  test('requires exactly `in_amount` to have left the source, before valuing the fill', () => {
    const soldCheck = requireLabeled(jupiterOracleCheckedSwap, 'soldTheRouteInput');
    expect(soldCheck.condition).toEqual(expression.equal(expression.variable('sold'), expression.input('inAmount')));
    expect(dependsOn(soldCheck.condition, bindings, reads('sourceAta', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(true);
    const labels = jupiterOracleCheckedSwap.steps.map((step) => step.label);
    expect(labels.indexOf('soldTheRouteInput')).toBe(labels.indexOf('measureAmountSold') + 1);
    expect(labels.indexOf('soldTheRouteInput')).toBeLessThan(labels.indexOf('fillBeatTheOracle'));
  });

  // Jupiter checks `route`'s destination by its mint alone, and a step pays whichever account it
  // names. On the real programs an attacker's USDC account, there and as the step's output, took
  // the whole fill and cleared the fill check (tests/protocols/findings/oracle-swap.md).
  test('requires the trader to own both token accounts, with the mint checks and before the swap', () => {
    expect(TOKEN_ACCOUNT_OWNER_OFFSET).toBe(32);
    for (const [label, name] of [
      ['sellsTheTradersOwnTokens', 'sourceAta'],
      ['proceedsGoToTheTrader', 'destinationAta'],
    ] as const) {
      expect(requireLabeled(jupiterOracleCheckedSwap, label).condition).toEqual(
        expression.equal(
          expression.accountData(account.fixed(name), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
          expression.variable('traderKey'),
        ),
      );
    }
    // The key is read once, into `traderKey`, right before the two checks.
    expect(bindingsOf(jupiterOracleCheckedSwap).get('traderKey')).toEqual([expression.accountKey('trader')]);
    const labels = jupiterOracleCheckedSwap.steps.map((step) => step.label);
    expect(labels.indexOf('sellsTheTradersOwnTokens')).toBe(labels.indexOf('destinationHoldsTheDestinationMint') + 2);
    expect(labels.indexOf('proceedsGoToTheTrader')).toBe(labels.indexOf('sellsTheTradersOwnTokens') + 1);
    expect(labels.indexOf('proceedsGoToTheTrader')).toBeLessThan(labels.indexOf('swap'));
  });
});

/**
 * The Pyth receiver owns every feed's price account alike, so pinning the owner takes any feed's
 * price: USDC/USD's would pass for SOL/USD's. Pyth's own `get_price_no_older_than` checks the
 * feed id, and so must every template that reads a price.
 */
describe('the Pyth templates', () => {
  const pythTemplates: [string, Template][] = [
    ['jupiterOracleCheckedSwap', jupiterOracleCheckedSwap],
    ['pythFreshPriceGate', pythFreshPriceGate],
  ];

  test('are every example that reads an account the Pyth receiver owns', () => {
    const receiver = [...addressBytes(PYTH_RECEIVER)].join();
    const reading = Object.entries(protocols)
      .filter(([, template]) =>
        Object.values(template.accounts).some(
          (constraint) => constraint.owner !== undefined && [...constraint.owner].join() === receiver,
        ),
      )
      .map(([name]) => name)
      .sort();
    expect(reading).toEqual(pythTemplates.map(([name]) => name).sort());
  });

  // The gate's caller names the feed; the swap pins it, so whoever builds its run cannot.
  test.each([
    ['jupiterOracleCheckedSwap', jupiterOracleCheckedSwap, expression.pubkey(oracleSwapModule.FEED_ID)],
    ['pythFreshPriceGate', pythFreshPriceGate, expression.input('feedId')],
  ] as const)('%s requires the price account to carry its feed id', (_, template, feedId) => {
    expect(requireLabeled(template, 'priceIsTheExpectedFeed').condition).toEqual(
      expression.equal(expression.accountData(account.fixed('priceUpdate'), PYTH.feedId, 'pubkey'), feedId),
    );
  });

  // The feed id's offset holds only once the verification level has fixed the layout, and a
  // price read before the pin would be a price from whichever feed was passed.
  test.each(pythTemplates)('%s pins the feed right after the layout, before reading anything else', (_, template) => {
    const labels = template.steps.map((step) => step.label);
    expect(labels.indexOf('priceIsFullyVerified')).toBe(0);
    expect(labels.indexOf('priceIsTheExpectedFeed')).toBe(1);
  });

  // A price is `price × 10^exponent`. A requirement that never reads the exponent compares raw
  // integers whose meaning the feed decides, and another exponent would move each of them by a
  // power of ten.
  test.each(pythTemplates)("%s makes a requirement depend on the feed's exponent", (_, template) => {
    const bindings = bindingsOf(template);
    const reading = steps(template).some(
      (step) => step.kind === 'require' && dependsOn(step.condition, bindings, reads('priceUpdate', PYTH.exponent)),
    );
    expect(reading).toBe(true);
  });
});

describe('the Pyth gate', () => {
  // Its bounds are raw integers at the exponent the caller set them for, so it requires that
  // exponent, as soon as the feed is known.
  test("requires the feed's exponent to be `exponent`, right after the feed pin", () => {
    expect(requireLabeled(pythFreshPriceGate, 'priceExponentIsExpected').condition).toEqual(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.exponent, 'i32'),
        expression.input('exponent'),
      ),
    );
    expect(pythFreshPriceGate.inputs?.exponent).toEqual({ type: 'i64' });
    const labels = pythFreshPriceGate.steps.map((step) => step.label);
    expect(labels.indexOf('priceExponentIsExpected')).toBe(labels.indexOf('priceIsTheExpectedFeed') + 1);
  });
});

describe('the daily cap', () => {
  const bindings = bindingsOf(jupiterDailyCapSwap);
  const within = requireLabeled(jupiterDailyCapSwap, 'withinRateLimit');
  const input = (name: string) => (candidate: Expression) => candidate.kind === 'input' && candidate.name === name;

  test('charges the inAmount it forwards to Jupiter, before the swap', () => {
    const [swap] = invokesOf(jupiterDailyCapSwap, 'actionProgram') as [Invoke];
    expect(swap.data[2]).toEqual(data.encode('u64', expression.input('inAmount')));
    expect(dependsOn(within.condition, bindings, input('inAmount'))).toBe(true);
    // The cap and the rate are literals: no other input reaches the limit.
    for (const other of ['routePlan', 'quotedOutAmount', 'slippageBps', 'platformFeeBps']) {
      expect(dependsOn(within.condition, bindings, input(other))).toBe(false);
    }
    const all = steps(jupiterDailyCapSwap);
    expect(all.indexOf(within)).toBeLessThan(all.indexOf(swap));
  });

  test("sells only the caller's own wrapped SOL, checked before the charge and the swap", () => {
    expect(requireLabeled(jupiterDailyCapSwap, 'spendsWrappedSol').condition).toEqual(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.pubkey(addressBytes(WRAPPED_SOL_MINT)),
      ),
    );
    expect(requireLabeled(jupiterDailyCapSwap, 'sourceBelongsToTheCaller').condition).toEqual(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountKey('actor'),
      ),
    );
    const all = steps(jupiterDailyCapSwap);
    const firstWrite = all.findIndex((step) => step.kind === 'setRegistry');
    const swap = all.findIndex((step) => step.kind === 'invoke');
    for (const label of ['spendsWrappedSol', 'sourceBelongsToTheCaller']) {
      const check = all.indexOf(requireLabeled(jupiterDailyCapSwap, label));
      expect(check).toBeLessThan(all.indexOf(within));
      expect(check).toBeLessThan(firstWrite);
      expect(check).toBeLessThan(swap);
    }
  });

  test('requires exactly the charge to have left the source, after the swap', () => {
    const sold = requireLabeled(jupiterDailyCapSwap, 'soldWhatTheCapCharged');
    expect(dependsOn(sold.condition, bindings, input('inAmount'))).toBe(true);
    expect(dependsOn(sold.condition, bindings, reads('sourceAta', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(true);
    const all = steps(jupiterDailyCapSwap);
    expect(all.indexOf(sold)).toBeGreaterThan(all.findIndex((step) => step.kind === 'invoke'));
  });

  test("keys each caller's entry by the caller's own signing address, and has the caller pay for it", () => {
    expect(jupiterDailyCapSwap.accounts.spend?.registry).toEqual({
      name: 'dailySpend',
      key: expression.accountKey('actor'),
      payer: 'actor',
    });
    expect(jupiterDailyCapSwap.accounts.actor).toMatchObject({ signer: true, writable: true });
  });

  test("the runner passes the actor's own entry, writable, where the template opens it", async () => {
    const decoder = getAddressDecoder();
    const key = (byte: number): Address => decoder.decode(new Uint8Array(32).fill(byte));
    const [templateAddress, actor] = [key(1), key(2)];
    // A `route` with an empty plan: the discriminator, a u32 zero, then the 19-byte tail.
    const routeData = Uint8Array.from([...JUPITER_ROUTE, 0, 0, 0, 0, ...new Uint8Array(19)]);
    const sourceAta = key(3);
    const instruction = await buildDailyCapRun({ templateAddress, actor, sourceAta, routeData, actionAccounts: [] });
    const [entry] = await findRegistryEntryAddress(templateAddress, 0, actor);
    expect(instruction.accounts?.slice(3, 7)).toEqual([
      { address: actor, role: AccountRole.WRITABLE_SIGNER },
      { address: sourceAta, role: AccountRole.WRITABLE },
      { address: entry, role: AccountRole.WRITABLE },
      { address: address('11111111111111111111111111111111'), role: AccountRole.READONLY },
    ]);
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

  // Jupiter checks `route`'s destination by its mint alone, and a step pays whichever account it
  // names. On the real programs an attacker's wrapped SOL account, there and as the step's output,
  // took the whole sale and met the quote (tests/protocols/findings/token-sweep.md).
  test('requires the seller to own both token accounts, before reading either', () => {
    expect(TOKEN_ACCOUNT_OWNER_OFFSET).toBe(32);
    for (const [label, name] of [
      ['sweepsTheSellersOwnBalance', 'sourceAta'],
      ['proceedsGoToTheSeller', 'destinationAta'],
    ] as const) {
      expect(requireLabeled(tokenSweepIntoSwap, label).condition).toEqual(
        expression.equal(
          expression.accountData(account.fixed(name), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
          expression.accountField(account.fixed('seller'), 'key'),
        ),
      );
    }
    const labels = tokenSweepIntoSwap.steps.map((step) => step.label);
    expect(labels.slice(0, 2)).toEqual(['sweepsTheSellersOwnBalance', 'proceedsGoToTheSeller']);
  });
});

/**
 * Kamino's v1 lending handlers refuse every caller but Kamino itself and a short whitelist
 * (`CpiDisabled`), so a template calls the `_v2` handler. v2 checks only that the reserves and
 * the obligation were refreshed in the current slot, so the refreshes go ahead of the run in the
 * transaction, and no template makes them. v2's list ends in farm accounts that are writable when
 * the reserve has the farm and the Kamino program when it does not, so a template forwards that
 * tail as `farmAccounts`, a group, which keeps each account's own writable flag.
 */
const kaminoCalls: [string, Template, { discriminator: Uint8Array; declared: number; amount: Expression }][] = [
  ['jupiterDepositExactOutput', jupiterDepositExactOutput, { discriminator: KAMINO_DEPOSIT, declared: 14, amount: { kind: 'variable', name: 'received' } }],
  ['kaminoRepaySwapOutput', kaminoRepaySwapOutput, { discriminator: KAMINO_REPAY, declared: 9, amount: { kind: 'variable', name: 'swapped' } }],
  ['kaminoLiquidateWithProof', kaminoLiquidateWithProof, { discriminator: KAMINO_LIQUIDATE, declared: 20, amount: { kind: 'input', name: 'liquidityAmount' } }],
];
const kaminoDeposits: Template[] = [jupiterDepositExactOutput];

/** The examples whose `accounts` pin `program`, by name. */
function pinning(program: string): string[] {
  const pinned = [...addressBytes(program)].join();
  return Object.entries(protocols)
    .filter(([, template]) =>
      Object.values(template.accounts).some(
        (constraint) => constraint.address !== undefined && [...constraint.address].join() === pinned,
      ),
    )
    .map(([name]) => name)
    .sort();
}

describe('Kamino calls are v2, forward the farm tail as a group, and leave refreshing to the transaction', () => {
  test('every example that pins Kamino is listed here', () => {
    expect(pinning(KAMINO_LEND)).toEqual(kaminoCalls.map(([name]) => name).sort());
  });

  test.each(kaminoCalls)('%s', (_, template, expected) => {
    const calls = invokesOf(template, 'kamino');
    expect(calls).toHaveLength(1);
    const [call] = calls as [Invoke];
    const [discriminator, amount] = call.data;
    expect(discriminator?.kind === 'literal' ? [...discriminator.bytes] : []).toEqual([...expected.discriminator]);
    expect(amount).toEqual({ kind: 'encoded', encoding: 'u64', value: expected.amount });
    expect(call.accounts).toHaveLength(expected.declared);
    expect(call.accountGroup).toBe('farmAccounts');
  });

  test('a deposit passes the liquidity mint, the Kamino program as its unused placeholder, and the instructions sysvar', () => {
    for (const template of kaminoDeposits) {
      const [deposit] = invokesOf(template, 'kamino') as [Invoke];
      const names = deposit.accounts.map((entry) => nameOf(entry.account));
      expect(names[5]).toBe('reserveLiquidityMint');
      expect(names[10]).toBe('kamino');
      expect(deposit.accounts[10]!.writable).toBe(false);
      expect(names.slice(11)).toEqual(['tokenProgram', 'tokenProgram', 'instructionsSysvar']);
      expect(template.accounts.instructionsSysvar?.address).toEqual(addressBytes(SYSVAR_INSTRUCTIONS));
    }
  });
});

describe('the Kamino liquidation', () => {
  test('measures the bounty where Kamino pays the seized collateral', () => {
    const bindings = bindingsOf(kaminoLiquidateWithProof);
    const check = requireLabeled(kaminoLiquidateWithProof, 'liquidationPaidTheBounty');
    expect(dependsOn(check.condition, bindings, reads('userDestinationLiquidity', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(true);
    expect(dependsOn(check.condition, bindings, reads('userDestinationCollateral', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(false);
    const [liquidate] = invokesOf(kaminoLiquidateWithProof, 'kamino') as [Invoke];
    expect(nameOf(liquidate.accounts[15]!.account)).toBe('userDestinationLiquidity');
  });
});

/**
 * The run names every token account these templates pay, and the protocols check those accounts'
 * mints, not their owners. Each one must belong to the signer, checked before anything moves:
 * otherwise a hostile run builder names an account of its own, and the template's guarantee is
 * measured on it.
 */
const payouts: [string, string, Template, { account: string; signer: string }][] = [
  ['kaminoLiquidateWithProof', 'bountyGoesToTheLiquidator', kaminoLiquidateWithProof, { account: 'userDestinationLiquidity', signer: 'liquidator' }],
  ['kaminoLiquidateWithProof', 'seizedCollateralGoesToTheLiquidator', kaminoLiquidateWithProof, { account: 'userDestinationCollateral', signer: 'liquidator' }],
  ['kaminoRepaySwapOutput', 'swapPaysTheBorrower', kaminoRepaySwapOutput, { account: 'borrowedAssetAta', signer: 'borrower' }],
];

describe('every token account a lending template pays belongs to its signer', () => {
  test.each(payouts)('%s: %s', (_, label, template, { account, signer }) => {
    const check = requireLabeled(template, label);
    expect(check.condition).toMatchObject({ kind: 'binary', op: 'equal' });
    const bindings = bindingsOf(template);
    expect(dependsOn(check.condition, bindings, reads(account, TOKEN_ACCOUNT_OWNER_OFFSET))).toBe(true);
    expect(dependsOn(check.condition, bindings, accountKey(signer))).toBe(true);
    expect(template.accounts[account]?.owner).toEqual(TOKEN_PROGRAM_ADDRESS_BYTES);
    expect(template.accounts[signer]?.signer).toBe(true);
    const all = steps(template);
    expect(all.indexOf(check)).toBeLessThan(all.findIndex((step) => step.kind === 'invoke'));
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
    reserveLiquidityMint: key(19),
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
    // The route group, then Kamino's farm tail for a reserve without a farm.
    expect(addresses.slice(-5)).toEqual([key(17), key(18), KAMINO_LEND, KAMINO_LEND, KAMINO_FARMS]);
    // The first four reach the template as declared accounts, not again through the group.
    for (const declared of [tokenProgram, key(8), kamino.destinationAta]) {
      expect(addresses.filter((entry) => entry === declared)).toHaveLength(1);
    }
  });

  test("forwards a reserve's collateral farm, writable, before the Farms program", async () => {
    const instruction = await buildJupiterDepositRun({
      creator: owner,
      templateId: 0,
      swap: { programId: JUPITER_V6, accounts: routeAccounts, data },
      kamino: { ...kamino, farm: { reserveFarmState: key(30), obligationFarmUserState: key(31) } },
      minimumOut: 1n,
    });
    const tail = (instruction.accounts ?? []).slice(-3);
    expect(tail.map((meta) => meta.address)).toEqual([key(31), key(30), KAMINO_FARMS]);
    expect(tail.map((meta) => isWritableRole(meta.role))).toEqual([true, true, false]);
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

describe('Orca fee destinations belong to the position holder', () => {
  // Whirlpools' collect_fees checks only these accounts' mint, never who owns them, so nothing
  // else stops a run an untrusted builder assembled from paying a stranger instead of the holder.
  // The holder is positionTokenAccount's own owner, not positionAuthority, which may be only a
  // delegate Whirlpools accepts in the holder's place.
  const readsRow = (name: string, offset: number) => (candidate: Expression) =>
    candidate.kind === 'accountData' &&
    candidate.account.kind === 'iterationAccount' &&
    candidate.account.name === name &&
    candidate.offset === offset;

  test('orcaCompoundFees pins tokenOwnerAccountA/B to positionTokenAccount', () => {
    const bindings = bindingsOf(orcaCompoundFees);
    const check = requireLabeled(orcaCompoundFees, 'feesGoToThePositionHolder');
    for (const acc of ['tokenOwnerAccountA', 'tokenOwnerAccountB'] as const) {
      expect(dependsOn(check.condition, bindings, reads(acc, TOKEN_ACCOUNT_OWNER_OFFSET))).toBe(true);
    }
    expect(
      dependsOn(check.condition, bindings, reads('positionTokenAccount', TOKEN_ACCOUNT_OWNER_OFFSET)),
    ).toBe(true);
    expect(dependsOn(check.condition, bindings, accountKey('positionAuthority'))).toBe(false);
  });

  test('orcaHarvestManyPositions checks each row against the fixed fee accounts', () => {
    const bindings = bindingsOf(orcaHarvestManyPositions);
    const check = requireLabeled(orcaHarvestManyPositions, 'positionBelongsToTheFeeOwner');
    expect(
      dependsOn(check.condition, bindings, readsRow('positionTokenAccount', TOKEN_ACCOUNT_OWNER_OFFSET)),
    ).toBe(true);
    for (const acc of ['tokenOwnerAccountA', 'tokenOwnerAccountB'] as const) {
      expect(dependsOn(check.condition, bindings, reads(acc, TOKEN_ACCOUNT_OWNER_OFFSET))).toBe(true);
    }
    expect(dependsOn(check.condition, bindings, accountKey('positionAuthority'))).toBe(false);
  });
});

describe('the Orca harvest runner', () => {
  const decoder = getAddressDecoder();
  const key = (byte: number): Address => decoder.decode(new Uint8Array(32).fill(byte));
  const accounts = {
    positionAuthority: key(2),
    whirlpool: key(3),
    tokenOwnerAccountA: key(4),
    tokenOwnerAccountB: key(5),
    tokenVaultA: key(6),
    tokenVaultB: key(7),
  };

  test('passes each row as the position, its NFT account and its two tick arrays', async () => {
    const instruction = await buildOrcaHarvestRun({
      creator: key(1),
      templateId: 0,
      accounts,
      positions: [
        { position: key(10), positionTokenAccount: key(11), tickArrayLower: key(12), tickArrayUpper: key(13) },
        { position: key(20), positionTokenAccount: key(21), tickArrayLower: key(12), tickArrayUpper: key(13) },
      ],
      dustFloor: 5n,
    });
    const metas = (instruction.accounts ?? []).map((meta) => [meta.address, meta.role]);
    // The template, then eight fixed accounts. Each row's update writes the pool.
    expect(metas[4]).toEqual([key(3), AccountRole.WRITABLE]);
    expect(metas.slice(9)).toEqual([
      [key(10), AccountRole.WRITABLE],
      [key(11), AccountRole.READONLY],
      [key(12), AccountRole.READONLY],
      [key(13), AccountRole.READONLY],
      [key(20), AccountRole.WRITABLE],
      [key(21), AccountRole.READONLY],
      [key(12), AccountRole.READONLY],
      [key(13), AccountRole.READONLY],
    ]);
  });

  test('finds the tick array holding a tick, as mainnet derives it', async () => {
    const solUsdc = address('Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE');
    expect(await getOrcaTickArrayAddress(solUsdc, -20_980, 4)).toBe('FdtvWk8j5u1a64YK2Uxk9eXxKZJTwLHDGx8aJPbJyw2Q');
    // An array starts at a multiple of 88 tick spacings; the tick below it is in the previous one.
    expect(await getOrcaTickArrayAddress(solUsdc, -21_120, 4)).toBe('FdtvWk8j5u1a64YK2Uxk9eXxKZJTwLHDGx8aJPbJyw2Q');
    expect(await getOrcaTickArrayAddress(solUsdc, -21_121, 4)).toBe('6hA1LN1fzCiXqymDiQXeBFn5da1b7STP1L7JmDc6hR3M');
    const thin = address('HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ');
    expect(await getOrcaTickArrayAddress(thin, -20_989, 64)).toBe('CEstjhG1v4nUgvGDyFruYEbJ18X8XeN4sX1WFCLt4D5c');
  });
});

describe('the Orca harvest runner names the program that refused', () => {
  const BALLISTA = 'BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD';
  const WHIRLPOOLS = 'whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc';

  test('blames Whirlpools for its own code, though Ballista uses the same number', () => {
    const logs = [
      `Program ${BALLISTA} invoke [1]`,
      `Program ${WHIRLPOOLS} invoke [2]`,
      'Program log: Instruction: UpdateFeesAndRewards',
      `Program ${WHIRLPOOLS} consumed 7835 of 196554 compute units`,
      `Program ${WHIRLPOOLS} success`,
      `Program ${WHIRLPOOLS} invoke [2]`,
      'Program log: Instruction: CollectFees',
      'Program log: AnchorError occurred. Error Code: MissingOrInvalidDelegate. Error Number: 6019. Error Message: Position token account has a missing or invalid delegate.',
      `Program ${WHIRLPOOLS} consumed 7400 of 186548 compute units`,
      `Program ${WHIRLPOOLS} failed: custom program error: 0x1783`,
      `Program ${BALLISTA} consumed 20852 of 200000 compute units`,
      `Program ${BALLISTA} failed: custom program error: 0x1783`,
    ];
    expect(describeFailure(6019, logs)).toBe('code 6019 came from Whirlpools, not Ballista');
  });

  test('explains a refusal in Ballista by its account or step', () => {
    const logs = [
      `Program ${BALLISTA} invoke [1]`,
      `Program ${BALLISTA} consumed 1200 of 200000 compute units`,
      `Program ${BALLISTA} failed: custom program error: 0x81784`,
    ];
    expect(describeFailure((8 << 16) | 6020, logs)).toBe(
      'AccountConstraintFailed: account position in row 0 does not satisfy its constraint',
    );
  });

  test('does not guess without logs', () => {
    expect(describeFailure(6019, [])).toBe('code 6019; the logs name no program that failed');
  });

  test('calls out truncated logs instead of reporting that no program failed', () => {
    const logs = [
      `Program ${BALLISTA} invoke [1]`,
      `Program ${WHIRLPOOLS} invoke [2]`,
      'Program log: Instruction: CollectFees',
      'Log truncated',
    ];
    expect(describeFailure(6019, logs)).toBe(
      'code 6019; the logs were truncated, so they cannot say which program failed',
    );
  });
});

describe('the signed-quote settlement', () => {
  const bindings = bindingsOf(signedQuoteSettlement);
  const is = (target: Expression) => (candidate: Expression) => isDeepStrictEqual(candidate, target);
  const signed = (offset: number, type: ReadType) => is(signedQuote.field(offset, type));
  const requirement = (label: string) => requireLabeled(signedQuoteSettlement, label).condition;
  const [takerPays, makerDelivers] = invokesOf(signedQuoteSettlement, 'tokenProgram') as [Invoke, Invoke];
  const amountOf = (call: Invoke) => {
    const part = call.data[1];
    return part?.kind === 'encoded' && part.encoding === 'u64' ? part.value : undefined;
  };

  test('takes the quote from the Ed25519 program, signed by the maker', () => {
    expect(
      dependsOn(requirement('quoteIsEd25519'), bindings, is(expression.pubkey(ED25519_PROGRAM_ADDRESS_BYTES))),
    ).toBe(true);
    expect(dependsOn(requirement('quoteIsBySigner'), bindings, accountKey('maker'))).toBe(true);
    // The maker signs the transaction as well, so the taker cannot put a key of its own there.
    expect(signedQuoteSettlement.accounts.maker?.signer).toBe(true);
  });

  test('settles only a message that starts with the quote tag', () => {
    expect(new TextDecoder().decode(QUOTE_TAG)).toBe('BLSTQT01');
    // The tag's eight bytes, read as a little-endian u64.
    expect(requirement('quoteIsTagged')).toEqual(
      expression.equal(signedQuote.field(QUOTE.tag, 'u64'), expression.u64(0x3130_5451_5453_4c42n)),
    );
  });

  test('the taker pays the signed price for what it takes', () => {
    expect(takerPays.label).toBe('takerPays');
    expect(takerPays.accounts.map((entry) => nameOf(entry.account))).toEqual([
      'takerQuoteAccount',
      'makerQuoteAccount',
      'taker',
    ]);
    const payment = amountOf(takerPays)!;
    expect(dependsOn(payment, bindings, signed(QUOTE.price, 'u64'))).toBe(true);
    expect(dependsOn(payment, bindings, is(expression.input('amount')))).toBe(true);
  });

  test('the maker delivers what the taker takes, within the signed size, to the signed taker, before expiry', () => {
    expect(makerDelivers.label).toBe('makerDelivers');
    expect(makerDelivers.accounts.map((entry) => nameOf(entry.account))).toEqual([
      'makerBaseAccount',
      'takerBaseAccount',
      'maker',
    ]);
    expect(amountOf(makerDelivers)).toEqual(expression.input('amount'));
    expect(dependsOn(requirement('withinTheQuotedSize'), bindings, signed(QUOTE.maxAmount, 'u64'))).toBe(true);
    expect(dependsOn(requirement('quoteIsForThisTaker'), bindings, signed(QUOTE.taker, 'pubkey'))).toBe(true);
    expect(dependsOn(requirement('quoteIsForThisTaker'), bindings, accountKey('taker'))).toBe(true);
    expect(dependsOn(requirement('quoteHasNotExpired'), bindings, signed(QUOTE.expiry, 'i64'))).toBe(true);
    expect(dependsOn(requirement('quoteHasNotExpired'), bindings, is(expression.clockUnixTimestamp()))).toBe(true);
  });

  test('settles only in the signed mints, into an account the maker owns', () => {
    const paysIn = requirement('paysInTheQuotedMint');
    expect(dependsOn(paysIn, bindings, signed(QUOTE.quoteMint, 'pubkey'))).toBe(true);
    expect(dependsOn(paysIn, bindings, reads('takerQuoteAccount', TOKEN_ACCOUNT_MINT_OFFSET))).toBe(true);
    const delivers = requirement('deliversTheQuotedMint');
    expect(dependsOn(delivers, bindings, signed(QUOTE.baseMint, 'pubkey'))).toBe(true);
    expect(dependsOn(delivers, bindings, reads('makerBaseAccount', TOKEN_ACCOUNT_MINT_OFFSET))).toBe(true);
    // Exactly this: the payee's owner equals the maker's key.
    expect(requirement('paymentReachesTheMaker')).toEqual(
      expression.equal(
        expression.accountData(account.fixed('makerQuoteAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('maker'), 'key'),
      ),
    );
  });

  test('every field lies inside the signed message', () => {
    expect(() => signedQuote.field(QUOTE.quoteMint + 1, 'pubkey')).toThrow(/inside/);
    expect(() => signedQuote.field(QUOTE.length - 7, 'u64')).toThrow(/inside/);
  });
});

/**
 * pump.fun's `buy` and `sell`, as its published IDL lists their accounts, then the two accounts its
 * April 2026 upgrade appended: the coin's `bonding-curve-v2` PDA and a buyback fee recipient. The
 * writable flags are a mainnet transaction's (`5SvWdLrM…` for `buy`, `2waaFKBL…` for `sell`).
 */
const pumpBuyAccounts: [string, boolean][] = [
  ['global', false],
  ['feeRecipient', true],
  ['mint', false],
  ['bondingCurve', true],
  ['curveTokenAccount', true],
  ['buyerTokenAccount', true],
  ['buyer', true],
  ['systemProgram', false],
  ['tokenProgram', false],
  ['creatorVault', true],
  ['eventAuthority', false],
  ['pumpProgram', false],
  ['globalVolumeAccumulator', true],
  ['userVolumeAccumulator', true],
  ['feeConfig', false],
  ['feeProgram', false],
  ['bondingCurveV2', false],
  ['buybackFeeRecipient', true],
];
const pumpSellAccounts: [string, boolean][] = [
  ['global', false],
  ['feeRecipient', true],
  ['mint', false],
  ['bondingCurve', true],
  ['curveTokenAccount', true],
  ['sellerTokenAccount', true],
  ['seller', true],
  ['systemProgram', false],
  ['creatorVault', true],
  ['tokenProgram', false],
  ['eventAuthority', false],
  ['pumpProgram', false],
  ['feeConfig', false],
  ['feeProgram', false],
  ['bondingCurveV2', false],
  ['buybackFeeRecipient', true],
];

describe('pump.fun calls are `buy` and `sell`, with their accounts in pump.fun’s order', () => {
  const pumpCalls: [string, Template, Uint8Array, [string, boolean][], string][] = [
    ['pumpFunBuyBasket', pumpFunBuyBasket, PUMP_FUN_BUY, pumpBuyAccounts, 'buyer'],
    ['pumpFunSellAll', pumpFunSellAll, PUMP_FUN_SELL, pumpSellAccounts, 'seller'],
  ];

  test('every example that pins pump.fun is listed here', () => {
    expect(pinning(PUMP_FUN)).toEqual(pumpCalls.map(([name]) => name).sort());
  });

  test.each(pumpCalls)('%s', (_, template, discriminator, accounts, signer) => {
    const calls = invokesOf(template, 'pumpProgram');
    expect(calls).toHaveLength(1);
    const [call] = calls as [Invoke];
    const [head] = call.data;
    expect(head?.kind === 'literal' ? [...head.bytes] : []).toEqual([...discriminator]);
    expect(call.accounts.map((entry) => [nameOf(entry.account), entry.writable])).toEqual(accounts);
    expect(call.accounts.filter((entry) => entry.signer).map((entry) => nameOf(entry.account))).toEqual([signer]);
    expect(template.accounts.pumpProgram?.address).toEqual(addressBytes(PUMP_FUN));
    expect(template.accounts.feeProgram?.address).toEqual(addressBytes(PUMP_FEES));
    expect(template.accounts.tokenProgram?.address).toEqual(TOKEN_2022_PROGRAM_ADDRESS_BYTES);
  });
});

/** Whether `candidate` reads the lamports of fixed account `name`. */
const lamportsOf = (name: string) => (candidate: Expression) =>
  candidate.kind === 'accountField' &&
  candidate.account.kind === 'account' &&
  candidate.account.name === name &&
  candidate.field === 'lamports';

describe('the pump.fun basket', () => {
  const bindings = bindingsOf(pumpFunBuyBasket);
  const [buy] = invokesOf(pumpFunBuyBasket, 'pumpProgram') as [Invoke];
  const loop = pumpFunBuyBasket.steps.find((step) => step.kind === 'forEach') as Extract<Step, { kind: 'forEach' }>;
  const row = (name: string) => account.iteration(name);

  test("buys each row's own amount, for at most its own max cost, and records the volume", () => {
    expect(buy.data.slice(1)).toEqual([
      data.encode('u64', expression.rowInput('amount')),
      data.encode('u64', expression.rowInput('maxSolCost')),
      data.literal(Uint8Array.of(1)),
    ]);
  });

  // pump.fun's `buy` pays any token account of the mint; on the real program, a stranger's
  // account in the row took the coins the buyer paid for (tests/protocols/tests/pump_fun_buy_basket.rs).
  test("requires each row's token account to be the buyer's, before buying", () => {
    expect(requireLabeled(pumpFunBuyBasket, 'tokensGoToTheBuyer').condition).toEqual(
      expression.equal(
        expression.accountData(row('buyerTokenAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('buyer'), 'key'),
      ),
    );
    expect(pumpFunBuyBasket.batch?.row.buyerTokenAccount?.owner).toEqual(TOKEN_2022_PROGRAM_ADDRESS_BYTES);
    const labels = loop.steps.map((step) => step.label);
    expect(labels.indexOf('tokensGoToTheBuyer')).toBeLessThan(labels.indexOf('buyOnTheCurve'));
  });

  test("refuses a graduated curve by its `complete` flag, read from pump.fun's own account, before buying", () => {
    expect(PUMP_BONDING_CURVE.complete).toBe(8 + 5 * 8);
    expect(requireLabeled(pumpFunBuyBasket, 'curveNotGraduated').condition).toEqual(
      expression.not(expression.accountData(row('bondingCurve'), PUMP_BONDING_CURVE.complete, 'bool')),
    );
    expect(pumpFunBuyBasket.batch?.row.bondingCurve?.owner).toEqual(addressBytes(PUMP_FUN));
    const labels = loop.steps.map((step) => step.label);
    expect(labels.indexOf('curveNotGraduated')).toBeLessThan(labels.indexOf('buyOnTheCurve'));
  });

  test('holds the running total of what the buyer lost to the budget, after every buy', () => {
    expect(loop.carry).toEqual(['spent']);
    const check = requireLabeled(pumpFunBuyBasket, 'withinBudget').condition;
    expect(dependsOn(check, bindings, lamportsOf('buyer'))).toBe(true);
    expect(dependsOn(check, bindings, (candidate) => isDeepStrictEqual(candidate, expression.input('budget')))).toBe(
      true,
    );
    // Measured on the buyer, never taken from what the run says a buy costs.
    expect(dependsOn(check, bindings, (candidate) => candidate.kind === 'rowInput')).toBe(false);
    expect(loop.steps.at(-1)?.label).toBe('withinBudget');
  });
});

describe('the pump.fun sale', () => {
  const bindings = bindingsOf(pumpFunSellAll);
  const [sell] = invokesOf(pumpFunSellAll, 'pumpProgram') as [Invoke];
  const labels = pumpFunSellAll.steps.map((step) => step.label);

  test('sells the balance it read, and leaves pump.fun’s own floor at 0', () => {
    const [, amount, floor] = sell.data;
    expect(amount?.kind === 'encoded' ? amount.encoding : undefined).toBe('u64');
    expect(
      amount?.kind === 'encoded' &&
        dependsOn(amount.value, bindings, reads('sellerTokenAccount', TOKEN_ACCOUNT_AMOUNT_OFFSET)),
    ).toBe(true);
    expect(floor).toEqual(data.encode('u64', expression.u64(0)));
  });

  test("measures the floor on the seller's lamports", () => {
    const check = requireLabeled(pumpFunSellAll, 'receivedAtLeastMinSolOut').condition;
    expect(dependsOn(check, bindings, lamportsOf('seller'))).toBe(true);
    expect(dependsOn(check, bindings, (candidate) => isDeepStrictEqual(candidate, expression.input('minSolOut')))).toBe(
      true,
    );
    expect(labels.at(-1)).toBe('receivedAtLeastMinSolOut');
  });

  test('requires the seller to own the account and the account to hold the mint, before the sale', () => {
    expect(requireLabeled(pumpFunSellAll, 'sellsTheSellersOwnTokens').condition).toEqual(
      expression.equal(
        expression.accountData(account.fixed('sellerTokenAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('seller'), 'key'),
      ),
    );
    expect(requireLabeled(pumpFunSellAll, 'holdsTheCurvesCoin').condition).toEqual(
      expression.equal(
        expression.accountData(account.fixed('sellerTokenAccount'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('mint'), 'key'),
      ),
    );
    for (const label of ['sellsTheSellersOwnTokens', 'holdsTheCurvesCoin', 'curveNotGraduated']) {
      expect(labels.indexOf(label)).toBeLessThan(labels.indexOf('sellEverything'));
    }
    expect(pumpFunSellAll.accounts.sellerTokenAccount?.owner).toEqual(TOKEN_2022_PROGRAM_ADDRESS_BYTES);
  });
});

describe('the pump.fun runners', () => {
  const decoder = getAddressDecoder();
  const key = (byte: number): Address => decoder.decode(new Uint8Array(32).fill(byte));

  // From mainnet: a buy of 9pd6hk… by 4qTArR… (`5SvWdLrM…`), and the snapshot's HjXcr1… curve,
  // whose creator is PLhgKg….
  test('derive the accounts mainnet uses', async () => {
    const traded = await pumpCoinAccounts({
      mint: address('9pd6hkPFWMN5Bid4spPhNrwsMSMjdikCH1w11nU1gj5M'),
      creator: key(0),
      mayhem: false,
    });
    expect(traded.bondingCurve).toBe('CkwPErsQdtxBG8PAt7jB48CZsqepk9GgrS7uL2djNmV2');
    expect(traded.curveTokenAccount).toBe('J4cateTFChfoTcpo46iroDYwHquxaFwxVxXpYhEWrFMH');
    expect(traded.bondingCurveV2).toBe('BMAN53X1yvLcJsbAhxxWuvsAVFJ4Hu9mnXb99ED3wKR7');
    const user = address('4qTArR2hTkL2o2gDNmz7P1LBXVUx5659waxHe2hNnuZk');
    expect(await token2022Ata(user, traded.mint)).toBe('HBhU8xqxyhKV4MnF8gJtsorkE6qopez7YKm59zPrqgm7');
    expect(await pumpUserVolumeAccumulator(user)).toBe('G8XbqQDXDG5sThi8J9GjSbegH5vXJA97AMujegJ2Gnc');
    const snapshotted = await pumpCoinAccounts({
      mint: address('HjXcr1A2k9mG2614UrCw5JnYnK7sAbe1y3EEeSmGmPUD'),
      creator: address('PLhgKg7snhYKniSsivdmydPmS9H78JFDrDdJKvBUTtu'),
      mayhem: false,
    });
    expect(snapshotted.creatorVault).toBe('91m6UXduxMiyYCkhirCkT9RbRiqJvHasBf4FTNFLsiaT');
  });

  test('pass one row per coin, in order, with a mayhem coin paying a reserved fee recipient', async () => {
    const coins = [
      { mint: key(10), creator: key(11), mayhem: false },
      { mint: key(20), creator: key(21), mayhem: true },
    ];
    const instruction = await buildPumpBuyBasketRun({
      templateAddress: key(1),
      buyer: key(2),
      buys: coins.map((coin) => ({ coin, amount: 5n, maxSolCost: 7n })),
      budget: 9n,
    });
    const metas = instruction.accounts ?? [];
    // The template, then eleven fixed accounts, then seven per row.
    expect(metas).toHaveLength(1 + 11 + 2 * 7);
    const rows = [0, 1].map((index) => metas.slice(12 + 7 * index, 19 + 7 * index));
    for (const [index, coin] of coins.entries()) {
      const accounts = await pumpCoinAccounts(coin);
      expect(rows[index]!.map((meta) => meta.address)).toEqual([
        coin.mint,
        accounts.bondingCurve,
        accounts.curveTokenAccount,
        await token2022Ata(key(2), coin.mint),
        accounts.creatorVault,
        accounts.bondingCurveV2,
        accounts.feeRecipient,
      ]);
      expect(rows[index]!.map((meta) => isWritableRole(meta.role))).toEqual([false, true, true, true, true, false, true]);
    }
    expect(rows[1]![6]!.address).toBe('GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS');
    await expect(
      buildPumpBuyBasketRun({ templateAddress: key(1), buyer: key(2), buys: [], budget: 9n }),
    ).rejects.toThrow(/1 to 7 coins/);
  });

  test("the sale names the seller's own associated token account", async () => {
    const coin = { mint: key(10), creator: key(11), mayhem: false };
    const instruction = await buildPumpSellAllRun({ templateAddress: key(1), seller: key(2), minSolOut: 3n, coin });
    const metas = instruction.accounts ?? [];
    expect(metas).toHaveLength(1 + 16);
    expect(metas[1 + 6]!.address).toBe(await token2022Ata(key(2), coin.mint));
    expect(metas[1 + 7]).toMatchObject({ address: key(2), role: AccountRole.WRITABLE_SIGNER });
  });
});
