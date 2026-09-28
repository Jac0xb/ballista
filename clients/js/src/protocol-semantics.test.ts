/**
 * What the live-protocol examples mean, not just that they compile.
 *
 * The protocols these templates call cannot run in CI, so these tests read the template documents
 * directly: which accounts reach Jupiter in which position, and which on-chain reads a guarantee
 * actually depends on. Each one pins a mistake an example once made.
 */
import { getAddressDecoder, isWritableRole, type Address } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  expression,
  type AccountReference,
  type Expression,
  type Step,
  type Template,
} from './index.js';
import * as protocols from '../examples/protocols/index.js';
import {
  jitoProfitGuardedTip,
  jupiterDepositExactOutput,
  jupiterOracleCheckedSwap,
  kaminoLiquidateWithProof,
  kaminoRepaySwapOutput,
  marginfiToKaminoRebalance,
  marginfiWithdrawAllWithFloor,
  pythFreshPriceGate,
  tokenSweepIntoSwap,
} from '../examples/protocols/index.js';
import { buildJupiterDepositRun } from '../examples/protocols/run-jupiter-deposit.js';
import {
  JITO_TIP_PAYMENT,
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_DEPOSIT,
  KAMINO_FARMS,
  KAMINO_LEND,
  KAMINO_LIQUIDATE,
  KAMINO_REPAY,
  MARGINFI_V2,
  MARGINFI_WITHDRAW,
  PYTH,
  PYTH_RECEIVER,
  SPL_MINT,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  WRAPPED_SOL_MINT,
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

const accountKey = (name: string) => (candidate: Expression) =>
  candidate.kind === 'accountField' &&
  candidate.account.kind === 'account' &&
  candidate.account.name === name &&
  candidate.field === 'key';

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
    // A round trip: it starts and ends in the one account the template measures.
    'jitoProfitGuardedTip',
    jitoProfitGuardedTip,
    { program: 'strategyProgram', accounts: ['tokenProgram', 'searcher', 'wsolAccount', 'wsolAccount'] },
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
          expression.accountField(account.fixed('trader'), 'key'),
        ),
      );
    }
    const labels = jupiterOracleCheckedSwap.steps.map((step) => step.label);
    expect(labels.indexOf('sellsTheTradersOwnTokens')).toBe(labels.indexOf('destinationHoldsTheDestinationMint') + 1);
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

  test.each(pythTemplates)('%s requires the price account to carry the feed id it is given', (_, template) => {
    expect(requireLabeled(template, 'priceIsTheExpectedFeed').condition).toEqual(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.feedId, 'pubkey'),
        expression.input('feedId'),
      ),
    );
    expect(template.inputs?.feedId).toEqual({ type: 'pubkey' });
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

describe('the Jito tip', () => {
  const bindings = bindingsOf(jitoProfitGuardedTip);
  const check = requireLabeled(jitoProfitGuardedTip, 'profitCoversTheTip');
  const lamportsOf = (candidate: Expression) => candidate.kind === 'accountField' && candidate.field === 'lamports';

  // Measured against the real programs in tests/protocols/tests/jito_tip.rs: Jupiter's `route`
  // moves token accounts only, and the Swap API wraps and unwraps SOL in instructions of their own
  // before and after it, so the searcher's lamports do not move while the route runs.
  test('measures profit on the wrapped-SOL account the round trip ends in, not on lamports', () => {
    expect(dependsOn(check.condition, bindings, reads('wsolAccount', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(true);
    expect(dependsOn(check.condition, bindings, lamportsOf)).toBe(false);
  });

  test('counts profit in lamports, the unit of the tip, by requiring wrapped SOL', () => {
    const holdsWrappedSol = requireLabeled(jitoProfitGuardedTip, 'wsolAccountHoldsWrappedSol');
    expect(
      dependsOn(holdsWrappedSol.condition, bindings, reads('wsolAccount', TOKEN_ACCOUNT_MINT_OFFSET)),
    ).toBe(true);
    const wrappedSolMint = [...addressBytes(WRAPPED_SOL_MINT)].join();
    expect(
      dependsOn(
        holdsWrappedSol.condition,
        bindings,
        (candidate) =>
          candidate.kind === 'literal' &&
          candidate.value.type === 'pubkey' &&
          [...candidate.value.value].join() === wrappedSolMint,
      ),
    ).toBe(true);
  });

  test('counts only profit that reaches the searcher, who pays the tip', () => {
    const ownsIt = requireLabeled(jitoProfitGuardedTip, 'searcherOwnsTheWsolAccount');
    expect(dependsOn(ownsIt.condition, bindings, reads('wsolAccount', TOKEN_ACCOUNT_OWNER_OFFSET))).toBe(true);
    expect(dependsOn(ownsIt.condition, bindings, accountKey('searcher'))).toBe(true);
  });

  // A subtraction of the balance before from the balance after underflows on a loss, and the run
  // then fails with ArithmeticOverflow before the requirement is ever reached.
  test('fails a loss at the requirement: nothing on the way to it subtracts', () => {
    const subtracts = (candidate: Expression) => candidate.kind === 'binary' && candidate.op === 'subtract';
    expect(dependsOn(check.condition, bindings, subtracts)).toBe(false);
    for (const input of ['tipLamports', 'minimumEdge']) {
      expect(
        dependsOn(check.condition, bindings, (candidate) => candidate.kind === 'input' && candidate.name === input),
      ).toBe(true);
    }
  });

  test('reads the balance before the strategy, and checks it after the strategy and before the tip', () => {
    const at = (matches: (step: Step) => boolean) => jitoProfitGuardedTip.steps.findIndex(matches);
    const readBefore = at((step) => step.kind === 'let' && step.label === 'readBalanceBeforeStrategy');
    const strategy = at((step) => step.kind === 'invoke' && step.label === 'runStrategy');
    const requirement = at((step) => step.kind === 'require' && step.label === 'profitCoversTheTip');
    const tip = at((step) => step.kind === 'invoke' && step.label === 'payJitoTip');
    expect([readBefore, strategy, requirement, tip].every((index) => index >= 0)).toBe(true);
    expect(readBefore < strategy && strategy < requirement && requirement < tip).toBe(true);
  });

  test("pays only an account of Jito's Tip Payment program", () => {
    expect(jitoProfitGuardedTip.accounts.jitoTip?.owner).toEqual(addressBytes(JITO_TIP_PAYMENT));
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
  ['marginfiToKaminoRebalance', marginfiToKaminoRebalance, { discriminator: KAMINO_DEPOSIT, declared: 14, amount: { kind: 'variable', name: 'moved' } }],
];
const kaminoDeposits: Template[] = [jupiterDepositExactOutput, marginfiToKaminoRebalance];

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
  ['marginfiWithdrawAllWithFloor', 'withdrawalGoesToTheAuthority', marginfiWithdrawAllWithFloor, { account: 'destinationAta', signer: 'authority' }],
  ['marginfiWithdrawAllWithFloor', 'sweepGoesToTheAuthority', marginfiWithdrawAllWithFloor, { account: 'treasuryAta', signer: 'authority' }],
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

const marginfiWithdrawals: [string, Template][] = [
  ['marginfiToKaminoRebalance', marginfiToKaminoRebalance],
  ['marginfiWithdrawAllWithFloor', marginfiWithdrawAllWithFloor],
];

describe('marginfi withdrawals', () => {
  test('every example that pins marginfi is listed here', () => {
    expect(pinning(MARGINFI_V2)).toEqual(marginfiWithdrawals.map(([name]) => name).sort());
  });

  test.each(marginfiWithdrawals)("%s forwards the health check's banks and oracles after withdraw's eight accounts", (_, template) => {
    const [withdraw] = invokesOf(template, 'marginfi') as [Invoke];
    const [discriminator] = withdraw.data;
    expect(discriminator?.kind === 'literal' ? [...discriminator.bytes] : []).toEqual([...MARGINFI_WITHDRAW]);
    expect(withdraw.accounts).toHaveLength(8);
    expect(withdraw.accountGroup).toBe('healthAccounts');
    // The vault authority is a PDA marginfi signs for; nothing writes it.
    expect(withdraw.accounts[5]!.writable).toBe(false);
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
