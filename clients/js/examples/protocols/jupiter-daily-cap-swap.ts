/**
 * A per-caller daily cap on a Jupiter swap.
 *
 * Each caller may sell at most `DAILY_CAP` of a route's input, refilling at `REFILL_PER_SECOND`,
 * which is the cap over a day. What a caller has sold and when lives in a registry entry keyed by
 * the caller's address: the first run creates it, at the caller's expense, and only this
 * template's runs can change it.
 *
 * The caller hands over the route in parts, as `splitJupiterRoute` splits the Swap API's data,
 * and the template reassembles Jupiter's `route` data from them, so the `inAmount` it charges is
 * the `inAmount` Jupiter sells. It passes the route its token program and the actor itself, and
 * forwards the rest of the route's accounts as its group. The cap and the rate are constants: a
 * cap the caller could set would limit nothing.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  rateLimit,
  step,
} from '../../src/index.js';
import { JUPITER_ROUTE, JUPITER_V6, addressBytes } from './shared.js';

/** 1.728 SOL, in lamports. */
export const DAILY_CAP = 1_728_000_000n;
/** The cap over 86,400 seconds. */
export const REFILL_PER_SECOND = 20_000n;

export const jupiterDailyCapSwap = defineTemplate({
  inputs: {
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`: what the route sells, and what the cap is charged. */
    inAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount`. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`. */
    platformFeeBps: { type: 'u64' },
  },
  registries: { dailySpend: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    actionProgram: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    actor: { signer: true, writable: true },
    spend: account.registry('dailySpend', { key: expression.accountKey('actor'), payer: 'actor' }),
    systemProgram: account.systemProgram(),
  },
  accountGroups: ['actionAccounts'],
  steps: [
    ...rateLimit({
      registry: 'spend',
      cap: expression.u64(DAILY_CAP),
      refillPerSecond: expression.u64(REFILL_PER_SECOND),
      amount: expression.input('inAmount'),
    }),
    step.invoke({
      program: account.fixed('actionProgram'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('actor'), signer: true, writable: false },
      ],
      accountGroup: 'actionAccounts',
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'swapWithinTheCap',
    }),
  ],
});

export const compiled = compileTemplate(jupiterDailyCapSwap);
