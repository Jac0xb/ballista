/**
 * Pay a Jito tip only out of an arbitrage that actually worked.
 *
 * Jito's own advice is to keep the tip in the same transaction as the strategy, "this way if the
 * transaction fails, you don't pay the Jito tip". That only helps when the strategy *fails*. An
 * arbitrage that succeeds but comes out thinner than the tip still pays the tip in full, and a
 * transaction cannot compare its own profit against its own tip.
 *
 * This template can. It snapshots the searcher's lamports, runs the client-built strategy, and
 * refuses to continue unless the realized profit covers the tip plus a margin. A run that misses
 * the bar reverts before the tip is transferred.
 *
 * The tip amount is a run input, so it is decided before signing exactly like an ordinary tip.
 * Ballista could instead compute it as a share of realized profit — the tip is a plain SOL
 * transfer and Jito accepts one made by CPI — but the block engine is closed source and the
 * public write-ups disagree on whether a runtime-computed amount is scored at its simulated
 * value in the auction or read from the instruction. Until that is settled, bidding a fixed
 * amount and guaranteeing you only pay it when you earned it is the version that cannot
 * misbehave.
 */
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';
import { JUPITER_V6, addressBytes } from './shared.js';

export const jitoProfitGuardedTip = defineTemplate({
  inputs: {
    /** The client-built strategy instruction, for example a Jupiter route. */
    strategyData: { type: 'bytes', maxLength: 512 },
    /** The bid, fixed before signing. Jito's floor is 1,000 lamports. */
    tipLamports: { type: 'u64' },
    /** What the searcher insists on keeping after the tip. */
    minimumEdge: { type: 'u64' },
  },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    strategyProgram: { executable: true, address: addressBytes(JUPITER_V6) },
    searcher: { signer: true, writable: true },
    /** One of the eight Jito tip accounts. */
    jitoTip: { writable: true },
  },
  accountGroups: ['strategyAccounts'],
  steps: [
    step.snapshot(
      'lamportsBefore',
      expression.accountField(account.fixed('searcher'), 'lamports'),
      'readBalanceBeforeStrategy',
    ),

    step.invoke({
      program: account.fixed('strategyProgram'),
      accounts: [{ account: account.fixed('searcher'), signer: true, writable: true }],
      accountGroup: 'strategyAccounts',
      data: [data.encode('bytes', expression.input('strategyData'))],
      label: 'runStrategy',
    }),

    // Lamports only, so this measures a SOL-denominated arbitrage. For a token-denominated one,
    // snapshot the token account's amount at offset 64 instead.
    step.let(
      'profit',
      expression.subtract(
        expression.accountField(account.fixed('searcher'), 'lamports'),
        expression.snapshot('lamportsBefore'),
      ),
      'measureProfit',
    ),

    // Reverts the whole bundle if the opportunity evaporated, before any tip is paid.
    step.require(
      expression.greaterThanOrEqual(
        expression.variable('profit'),
        expression.add(expression.input('tipLamports'), expression.input('minimumEdge')),
      ),
      'profitCoversTheTip',
    ),

    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('searcher'),
      to: account.fixed('jitoTip'),
      lamports: expression.input('tipLamports'),
      label: 'payJitoTip',
    }),
  ],
});

export const compiled = compileTemplate(jitoProfitGuardedTip);
