/**
 * The guide and example pages include their TypeScript from this directory. This test holds each
 * file to what the pages promise:
 *
 * - every template compiles to the bytes `fixtures/benchmarks.json` records for the example of the
 *   same name, which is the template the benchmarks measure;
 * - every run builds the run data and account privileges that fixture records.
 *
 * The few examples the benchmarks do not measure are recorded in
 * `clients/rust/tests/fixtures/docs-examples.json` in the same shape, which the Rust test
 * `clients/rust/tests/docs_examples.rs` reads to hold the Rust versions of every example to the
 * same bytes.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { getAddressDecoder, type Address, type Instruction } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { compileTemplate, type Template } from '../../src/index.js';
import { assertCreateThenTransfer, runAssertCreateThenTransfer } from './assert-create-then-transfer.js';
import { basisPointRevenueSplit, runBasisPointRevenueSplit } from './basis-point-revenue-split.js';
import { boundedKeeperCrank, runBoundedKeeperCrank } from './bounded-keeper-crank.js';
import { boundedSolPayroll, runBoundedSolPayroll } from './bounded-sol-payroll.js';
import { budgetedPayroll, explainBudgetFailure, runBudgetedPayroll } from './budgeted-payroll.js';
import { canonicalPositionAccount, runCanonicalPositionAccount } from './canonical-position-account.js';
import { claimOnlyWhenThereIsSomething, runClaimOnlyWhenThereIsSomething } from './claim-only-when-there-is-something.js';
import { claimThenDistribute, runClaimThenDistribute } from './claim-then-distribute.js';
import { closeEmptyTokenAccounts, runCloseEmptyTokenAccounts } from './close-empty-token-accounts.js';
import { conditionalAtaSetup, runConditionalAtaSetup } from './conditional-ata-setup.js';
import {
  consolidateOnlyTheFundedAccounts,
  runConsolidateOnlyTheFundedAccounts,
} from './consolidate-only-the-funded-accounts.js';
import { crankOncePerWaitingEntry, runCrankOncePerWaitingEntry } from './crank-once-per-waiting-entry.js';
import { crankOnlyTheRipeEntries, runCrankOnlyTheRipeEntries } from './crank-only-the-ripe-entries.js';
import { deadlineAndMinimumOutput, runDeadlineAndMinimumOutput } from './deadline-and-minimum-output.js';
import { deadlineRefund, runDeadlineRefund } from './deadline-refund.js';
import { distributeARuntimePotProRata, runDistributeARuntimePotProRata } from './distribute-a-runtime-pot-pro-rata.js';
import { exactLamportDelta, runExactLamportDelta } from './exact-lamport-delta.js';
import { exactTokenDebit, runExactTokenDebit } from './exact-token-debit.js';
import { existingAccountTokenPayroll, runExistingAccountTokenPayroll } from './existing-account-token-payroll.js';
import { forwardTheWholeTokenBalance, runForwardTheWholeTokenBalance } from './forward-the-whole-token-balance.js';
import { genericCpi, runGenericCpi } from './generic-cpi.js';
import { indexWeightedRewards, runIndexWeightedRewards } from './index-weighted-rewards.js';
import { initializeOnlyIfMissing, runInitializeOnlyIfMissing } from './initialize-only-if-missing.js';
import { liquidateOnlyWhenUnhealthy, runLiquidateOnlyWhenUnhealthy } from './liquidate-only-when-unhealthy.js';
import { maximumLamportSpend, runMaximumLamportSpend } from './maximum-lamport-spend.js';
import { oraclePriceBand, runOraclePriceBand } from './oracle-price-band.js';
import { pinnedProgramAndOwner, runPinnedProgramAndOwner } from './pinned-program-and-owner.js';
import { primaryOrFallbackRoute, runPrimaryOrFallbackRoute } from './primary-or-fallback-route.js';
import { rebalanceThreeSwaps, runRebalanceThreeSwaps } from './rebalance-three-swaps.js';
import { repayExactlyWhatIsOwed, runRepayExactlyWhatIsOwed } from './repay-exactly-what-is-owed.js';
import { reservePreservingSweep, runReservePreservingSweep } from './reserve-preserving-sweep.js';
import { rowAmounts, runRowAmounts } from './row-amounts.js';
import { runSplitWhatArrived, splitWhatArrived } from './split-what-arrived.js';
import { runSweepAboveAReserve, sweepAboveAReserve } from './sweep-above-a-reserve.js';
import { runSwapThenDeposit, swapThenDeposit } from './swap-then-deposit.js';
import { runTimeGatedGovernanceExecution, timeGatedGovernanceExecution } from './time-gated-governance-execution.js';
import { runTokenTransfer, tokenTransferTemplate } from './token-transfer.js';
import { runTopUpOnlyWhenLow, topUpOnlyWhenLow } from './top-up-only-when-low.js';
import { runWaterfallUntilTheMoneyRunsOut, waterfallUntilTheMoneyRunsOut } from './waterfall-until-the-money-runs-out.js';

const BENCHMARKS_PATH = fileURLToPath(new URL('../../../../fixtures/benchmarks.json', import.meta.url));
const DOCS_ONLY_PATH = fileURLToPath(
  new URL('../../../rust/tests/fixtures/docs-examples.json', import.meta.url),
);

interface Recorded {
  templateHex: string;
  runData: string;
  runtimeAccountFlags: { signer: boolean; writable: boolean }[];
  rowCount: number;
}

const benchmarks = JSON.parse(readFileSync(BENCHMARKS_PATH, 'utf8')) as Record<string, Recorded>;

const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString('hex');
/** A distinct placeholder address per index, the same bytes the Rust runs use. */
const key = (index: number): Address => getAddressDecoder().decode(new Uint8Array(32).fill(index + 1));
const keys = (start: number, count: number) => Array.from({ length: count }, (_, index) => key(start + index));
const TEMPLATE = key(200);

/** A System transfer's data padded to 32 bytes: the protocol-call stand-in the benchmarks use. */
const standInData = (() => {
  const bytes = new Uint8Array(32);
  bytes.set([2, 0, 0, 0]);
  new DataView(bytes.buffer).setBigUint64(4, 10_000n, true);
  return bytes;
})();

interface Case {
  name: string;
  template: Template;
  /** Builds the example's run with the inputs the fixture records, for `rows` batch rows. */
  run: (rows: number) => Instruction | Promise<Instruction>;
  /** Rows to record for an example the benchmarks do not measure. */
  docsOnlyRows?: number;
}

const cases: Case[] = [
  {
    name: 'sweep-above-a-reserve',
    template: sweepAboveAReserve,
    run: () => runSweepAboveAReserve({ templateAddress: TEMPLATE, vault: key(1), destination: key(2), reserve: 2_000_000n }),
  },
  {
    name: 'forward-the-whole-token-balance',
    template: forwardTheWholeTokenBalance,
    run: () =>
      runForwardTheWholeTokenBalance({ templateAddress: TEMPLATE, source: key(1), destination: key(2), authority: key(3) }),
  },
  {
    name: 'repay-exactly-what-is-owed',
    template: repayExactlyWhatIsOwed,
    run: () => runRepayExactlyWhatIsOwed({ templateAddress: TEMPLATE, loan: key(1), borrower: key(2), pool: key(3) }),
  },
  {
    name: 'split-what-arrived',
    template: splitWhatArrived,
    run: () =>
      runSplitWhatArrived({
        templateAddress: TEMPLATE,
        vault: key(1),
        partner: key(2),
        treasury: key(3),
        reserve: 2_000_000n,
        shareBps: 3_000n,
      }),
  },
  {
    name: 'claim-only-when-there-is-something',
    template: claimOnlyWhenThereIsSomething,
    run: () =>
      runClaimOnlyWhenThereIsSomething({ templateAddress: TEMPLATE, rewards: key(1), claimant: key(2), destination: key(3) }),
  },
  {
    name: 'liquidate-only-when-unhealthy',
    template: liquidateOnlyWhenUnhealthy,
    run: () =>
      runLiquidateOnlyWhenUnhealthy({
        templateAddress: TEMPLATE,
        position: key(1),
        liquidator: key(2),
        vault: key(3),
        threshold: 2n,
      }),
  },
  {
    name: 'top-up-only-when-low',
    template: topUpOnlyWhenLow,
    run: () =>
      runTopUpOnlyWhenLow({ templateAddress: TEMPLATE, funder: key(1), bot: key(2), floor: 2_000_000_000n, topUp: 10_000n }),
  },
  {
    name: 'initialize-only-if-missing',
    template: initializeOnlyIfMissing,
    run: () => runInitializeOnlyIfMissing({ templateAddress: TEMPLATE, payer: key(1), position: key(2) }),
  },
  {
    name: 'waterfall-until-the-money-runs-out',
    template: waterfallUntilTheMoneyRunsOut,
    run: (rows) =>
      runWaterfallUntilTheMoneyRunsOut({
        templateAddress: TEMPLATE,
        treasury: key(1),
        reserve: 2_000_000n,
        creditors: keys(10, rows).map((address) => ({ address, owed: 1_000n })),
      }),
  },
  {
    name: 'consolidate-only-the-funded-accounts',
    template: consolidateOnlyTheFundedAccounts,
    run: (rows) =>
      runConsolidateOnlyTheFundedAccounts({
        templateAddress: TEMPLATE,
        vault: key(1),
        authority: key(2),
        sources: keys(10, rows),
      }),
  },
  {
    name: 'crank-only-the-ripe-entries',
    template: crankOnlyTheRipeEntries,
    run: (rows) => runCrankOnlyTheRipeEntries({ templateAddress: TEMPLATE, keeper: key(1), entries: keys(10, rows) }),
  },
  {
    name: 'distribute-a-runtime-pot-pro-rata',
    template: distributeARuntimePotProRata,
    run: (rows) =>
      runDistributeARuntimePotProRata({
        templateAddress: TEMPLATE,
        vault: key(1),
        reserve: 2_000_000n,
        holders: keys(10, rows).map((address) => ({ address, weightBps: 100n })),
      }),
  },
  {
    name: 'bounded-sol-payroll',
    template: boundedSolPayroll,
    run: (rows) =>
      runBoundedSolPayroll({ templateAddress: TEMPLATE, treasury: key(1), recipients: keys(10, rows), amount: 10_000n }),
  },
  {
    name: 'basis-point-revenue-split',
    template: basisPointRevenueSplit,
    run: () =>
      runBasisPointRevenueSplit({
        templateAddress: TEMPLATE,
        source: key(1),
        partner: key(2),
        treasury: key(3),
        total: 1_000_000n,
        partnerBps: 250n,
      }),
  },
  {
    name: 'index-weighted-rewards',
    template: indexWeightedRewards,
    run: (rows) =>
      runIndexWeightedRewards({ templateAddress: TEMPLATE, treasury: key(1), recipients: keys(10, rows), base: 1_000n }),
  },
  {
    name: 'deadline-refund',
    template: deadlineRefund,
    run: () =>
      runDeadlineRefund({
        templateAddress: TEMPLATE,
        escrowAuthority: key(1),
        customer: key(2),
        refundAmount: 10_000n,
        deadline: 9_000_000_000n,
      }),
  },
  {
    name: 'reserve-preserving-sweep',
    template: reservePreservingSweep,
    run: () =>
      runReservePreservingSweep({
        templateAddress: TEMPLATE,
        payer: key(1),
        vault: key(2),
        reserve: 1_000_000_000n,
        cap: 50_000n,
      }),
  },
  {
    name: 'assert-create-then-transfer',
    template: assertCreateThenTransfer,
    run: (rows) =>
      runAssertCreateThenTransfer({
        templateAddress: TEMPLATE,
        mint: key(1),
        payer: key(2),
        authority: key(3),
        source: key(4),
        recipients: keys(10, rows).map((wallet, index) => ({ wallet, ata: key(100 + index) })),
        amount: 1_000n,
      }),
  },
  {
    name: 'existing-account-token-payroll',
    template: existingAccountTokenPayroll,
    run: (rows) =>
      runExistingAccountTokenPayroll({
        templateAddress: TEMPLATE,
        source: key(1),
        authority: key(2),
        destinations: keys(10, rows),
        amount: 1_000n,
      }),
  },
  {
    name: 'conditional-ata-setup',
    template: conditionalAtaSetup,
    run: () =>
      runConditionalAtaSetup({ templateAddress: TEMPLATE, mint: key(1), payer: key(2), wallet: key(3), ata: key(4) }),
  },
  {
    name: 'close-empty-token-accounts',
    template: closeEmptyTokenAccounts,
    run: (rows) =>
      runCloseEmptyTokenAccounts({
        templateAddress: TEMPLATE,
        rentDestination: key(1),
        authority: key(2),
        tokenAccounts: keys(10, rows),
      }),
  },
  {
    name: 'exact-token-debit',
    template: exactTokenDebit,
    run: () =>
      runExactTokenDebit({
        templateAddress: TEMPLATE,
        source: key(1),
        destination: key(2),
        authority: key(3),
        amount: 1_000n,
      }),
  },
  {
    name: 'deadline-and-minimum-output',
    template: deadlineAndMinimumOutput,
    run: () =>
      runDeadlineAndMinimumOutput({
        templateAddress: TEMPLATE,
        payer: key(1),
        pool: key(2),
        deadline: 9_000_000_000n,
        quotedOut: 1_000n,
        minimumOut: 900n,
        routeData: standInData,
      }),
  },
  {
    name: 'pinned-program-and-owner',
    template: pinnedProgramAndOwner,
    run: () => runPinnedProgramAndOwner({ templateAddress: TEMPLATE, payer: key(1), position: key(2), amount: 10_000n }),
  },
  {
    name: 'oracle-price-band',
    template: oraclePriceBand,
    run: () =>
      runOraclePriceBand({
        templateAddress: TEMPLATE,
        oracle: key(1),
        payer: key(2),
        pool: key(3),
        minimumPrice: 1n,
        maximumPrice: 1_000_000n,
      }),
  },
  {
    name: 'maximum-lamport-spend',
    template: maximumLamportSpend,
    run: () => runMaximumLamportSpend({ templateAddress: TEMPLATE, payer: key(1), pool: key(2), maximumSpend: 1_000_000n }),
  },
  {
    name: 'canonical-position-account',
    template: canonicalPositionAccount,
    run: () => runCanonicalPositionAccount({ templateAddress: TEMPLATE, owner: key(1), positionId: 7n }),
  },
  {
    name: 'swap-then-deposit',
    template: swapThenDeposit,
    run: () =>
      runSwapThenDeposit({
        templateAddress: TEMPLATE,
        payer: key(1),
        pool: key(2),
        receivedTokens: key(3),
        minimumOut: 0n,
        swapData: standInData,
        depositData: standInData,
      }),
  },
  {
    name: 'claim-then-distribute',
    template: claimThenDistribute,
    run: (rows) =>
      runClaimThenDistribute({
        templateAddress: TEMPLATE,
        claimer: key(1),
        pool: key(2),
        treasuryTokens: key(3),
        authority: key(4),
        recipientTokens: keys(10, rows),
        amountPerRecipient: 1_000n,
      }),
  },
  {
    name: 'primary-or-fallback-route',
    template: primaryOrFallbackRoute,
    run: () =>
      runPrimaryOrFallbackRoute({
        templateAddress: TEMPLATE,
        payer: key(1),
        pool: key(2),
        usePrimary: true,
        primaryData: standInData,
        fallbackData: standInData,
      }),
  },
  {
    name: 'time-gated-governance-execution',
    template: timeGatedGovernanceExecution,
    run: () =>
      runTimeGatedGovernanceExecution({
        templateAddress: TEMPLATE,
        proposal: key(1),
        payer: key(2),
        target: key(3),
        executeData: standInData,
      }),
  },
  {
    name: 'bounded-keeper-crank',
    template: boundedKeeperCrank,
    run: (rows) =>
      runBoundedKeeperCrank({
        templateAddress: TEMPLATE,
        keeper: key(1),
        rows: keys(10, rows).map((market, index) => ({ market, queue: key(100 + index) })),
      }),
  },
  // Examples only the guide pages use.
  {
    name: 'row-amounts',
    template: rowAmounts,
    docsOnlyRows: 3,
    run: (rows) =>
      runRowAmounts({
        templateAddress: TEMPLATE,
        treasury: key(1),
        payees: keys(10, rows).map((address, index) => ({ address, lamports: 10_000n * BigInt(index + 1) })),
      }),
  },
  {
    name: 'budgeted-payroll',
    template: budgetedPayroll,
    docsOnlyRows: 3,
    run: (rows) =>
      runBudgetedPayroll({
        templateAddress: TEMPLATE,
        treasury: key(1),
        recipients: keys(10, rows),
        amount: 10_000n,
        budget: 250_000n,
      }),
  },
  {
    name: 'exact-lamport-delta',
    template: exactLamportDelta,
    docsOnlyRows: 0,
    run: () => runExactLamportDelta({ templateAddress: TEMPLATE, sender: key(1), recipient: key(2), amount: 50_000_000n }),
  },
  {
    name: 'token-transfer',
    template: tokenTransferTemplate,
    docsOnlyRows: 0,
    run: () =>
      runTokenTransfer({
        templateAddress: TEMPLATE,
        authority: key(1),
        source: key(2),
        destination: key(3),
        amount: 25_000n,
      }),
  },
  {
    name: 'generic-cpi',
    template: genericCpi,
    docsOnlyRows: 0,
    run: () =>
      runGenericCpi({
        templateAddress: TEMPLATE,
        vault: key(1),
        authority: key(2),
        amount: 25_000n,
        clientPayload: Uint8Array.of(9, 9, 9),
        enabled: true,
      }),
  },
  {
    name: 'rebalance-three-swaps',
    template: rebalanceThreeSwaps,
    docsOnlyRows: 0,
    run: () =>
      runRebalanceThreeSwaps({
        templateAddress: TEMPLATE,
        user: key(10),
        legs: [
          {
            source: key(1),
            destination: key(2),
            route: new Uint8Array(40).fill(1),
            pools: keys(20, 3).map((address) => ({ address, writable: true })),
            target: 5_000n,
            minOut: 4_900n,
          },
          { source: key(3), destination: key(4), route: new Uint8Array(), pools: [], target: 0n, minOut: 0n },
          {
            source: key(5),
            destination: key(6),
            route: new Uint8Array(24).fill(3),
            pools: keys(30, 2).map((address) => ({ address, writable: true })),
            target: 7_000n,
            minOut: 6_800n,
          },
        ],
      }),
  },
  {
    name: 'crank-once-per-waiting-entry',
    template: crankOncePerWaitingEntry,
    docsOnlyRows: 0,
    run: () => runCrankOncePerWaitingEntry({ templateAddress: TEMPLATE, keeper: key(1), queue: key(2) }),
  },
];

async function record(item: Case, rows: number): Promise<Recorded> {
  const compiled = compileTemplate(item.template);
  const instruction = await item.run(rows);
  const data = instruction.data ?? new Uint8Array();
  expect(data[0], `${item.name}: Run discriminator`).toBe(5);
  return {
    templateHex: hex(compiled.bytes),
    runData: hex(data.slice(1)),
    // The template account is first; the rest follow the template's declared order.
    runtimeAccountFlags: (instruction.accounts ?? []).slice(1).map((entry) => ({
      signer: (Number(entry.role) & 2) !== 0,
      writable: (Number(entry.role) & 1) !== 0,
    })),
    rowCount: rows,
  };
}

describe('docs examples', () => {
  test('every benchmarked example compiles and runs exactly as the benchmark fixture records', async () => {
    for (const item of cases.filter((entry) => entry.docsOnlyRows === undefined)) {
      const expected = benchmarks[item.name];
      expect(expected, `${item.name} is in fixtures/benchmarks.json`).toBeDefined();
      const actual = await record(item, expected!.rowCount);
      expect(actual.templateHex, `${item.name}: template bytes`).toBe(expected!.templateHex);
      expect(actual.runData, `${item.name}: run data`).toBe(expected!.runData);
      expect(actual.runtimeAccountFlags, `${item.name}: account privileges`).toEqual(expected!.runtimeAccountFlags);
    }
  });

  test('records the guide-only examples for the Rust test', async () => {
    const output: Record<string, Recorded> = {};
    for (const item of cases.filter((entry) => entry.docsOnlyRows !== undefined)) {
      expect(benchmarks[item.name], `${item.name} is not a benchmark`).toBeUndefined();
      output[item.name] = await record(item, item.docsOnlyRows!);
    }
    writeFileSync(DOCS_ONLY_PATH, `${JSON.stringify(output, null, 2)}\n`);
    expect(Object.keys(output)).toHaveLength(7);
  });

  test('the budget example explains its labelled failure', () => {
    // RequirementFailed (6015) at program counter 8, the budget check after the loop.
    expect(explainBudgetFailure((8 << 16) | 6015)).toBe('RequirementFailed at steps[2] (withinBudget)');
  });
});
