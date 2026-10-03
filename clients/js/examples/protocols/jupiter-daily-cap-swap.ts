/**
 * A per-caller daily cap on a Jupiter swap.
 *
 * Each caller may sell at most `DAILY_CAP` lamports of its wrapped SOL through the template,
 * refilling at `REFILL_PER_SECOND`, which is the cap over a day. What a caller has sold and when lives in a registry entry keyed by
 * the caller's address: the first run creates it, at the caller's expense, and only this
 * template's runs can change it.
 *
 * The caller hands over the route in parts, as `splitJupiterRoute` splits the Swap API's data,
 * and the template reassembles Jupiter's `route` data from them, so the `inAmount` it charges is
 * the `inAmount` Jupiter sells. It passes the route its token program, the actor and the source
 * token account itself, and forwards the rest of the route's accounts as its group. The cap and
 * the rate are constants: a cap the caller could set would limit nothing.
 *
 * The cap counts lamports, so the source must be the actor's own wrapped SOL: `inAmount` is in
 * base units of whatever the route sells, and 1.728 USDC counted as 1.728 SOL is no cap. Jupiter
 * asks of the source only that it hold `inAmount`, and moves the accounts its steps name, so the
 * template also requires exactly `inAmount` to have left the source once the route has run.
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
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  WRAPPED_SOL_MINT,
  addressBytes,
} from './shared.js';

const sourceBalance = expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

/** 1.728 SOL, in lamports. */
export const DAILY_CAP = 1_728_000_000n;
/** The cap over 86,400 seconds. */
export const REFILL_PER_SECOND = 20_000n;

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

export const jupiterDailyCapSwap = defineTemplate({
  inputs: {
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`, in lamports: what it sells, what the cap is charged, and what must leave `sourceAta`. */
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
    /** The actor's wrapped-SOL token account, which the route sells from. */
    sourceAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    spend: account.registry('dailySpend', { key: expression.accountKey('actor'), payer: 'actor' }),
    systemProgram: account.systemProgram(),
  },
  accountGroups: ['actionAccounts'],
  steps: [
    // The cap counts lamports: a route that sells another mint would be charged in its units.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.pubkey(addressBytes(WRAPPED_SOL_MINT)),
      ),
      'spendsWrappedSol',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountKey('actor'),
      ),
      'sourceBelongsToTheCaller',
    ),
    ...rateLimit({
      registry: 'spend',
      cap: expression.u64(DAILY_CAP),
      refillPerSecond: expression.u64(REFILL_PER_SECOND),
      amount: expression.input('inAmount'),
    }),
    step.snapshot('sourceBefore', sourceBalance, 'readSourceBeforeSwap'),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),
    step.invoke({
      program: account.fixed('actionProgram'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('actor'), signer: true, writable: false },
        { account: account.fixed('sourceAta'), signer: false, writable: true },
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
    // Jupiter moves the accounts its steps name, not the source it was handed: a step that sold
    // another account's tokens would leave the charge in their units. Jupiter required the source
    // to hold `inAmount`, so the subtraction cannot underflow.
    step.require(
      expression.equal(
        expression.subtract(expression.snapshot('sourceBefore'), expression.input('inAmount')),
        sourceBalance,
      ),
      'soldWhatTheCapCharged',
    ),
  ],
});

export const compiled = compileTemplate(jupiterDailyCapSwap);
