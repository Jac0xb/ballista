/**
 * Deposit into Kamino exactly what a Jupiter swap produced.
 *
 * Jupiter v6's `route` carries the input amount, the *quoted* output, `slippageBps` and
 * `platformFeeBps`. What actually comes out is reported as an Anchor `SwapEvent` emitted through
 * a self-CPI (an event, not return data), so a caller cannot read it back with
 * `get_return_data`. The destination token account is the only reliable source, and it can only
 * be read after the route has run.
 *
 * Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount: u64)` needs
 * that number. A plain transaction has to write it before the swap has happened: quote it high
 * and the deposit fails, quote it low and the remainder is stranded in the ATA.
 *
 * `route` takes the token program, the signing owner, and the owner's source and destination token
 * accounts first, and the template passes those four itself. The destination is the account the
 * template measures, and the run deposits exactly its measured increase. Kamino mints whole
 * cTokens only, so it keeps back less than one cToken's worth as rounding (see below). If the
 * destination is not the signer's, the deposit fails closed: Kamino refuses to debit it. The rest
 * of the route's list varies in length with the route, so it arrives as the `routeAccounts` group.
 * Group members are forwarded with the transaction's own writable flag and never sign.
 *
 * The deposit is Kamino's `_v2` handler. The v1 handler refuses every caller but Kamino itself and
 * a short whitelist (`CpiDisabled`), so a template cannot call it at all. v2 takes 17 accounts:
 * - The 14 declared below. The unused `placeholder_user_destination_collateral` slot holds the
 *   Kamino program: Kamino requires every optional slot to be present and reads its own ID as
 *   "none". Both token-program slots hold the SPL Token program.
 * - Then `farmAccounts`: the obligation's farm user state and the reserve's collateral farm, then
 *   the Farms program. When the reserve has no collateral farm, both farm slots hold the Kamino
 *   program. A group carries them because they are writable when present and read-only when they
 *   are the Kamino program, and a declared slot has one fixed writable flag. Before an
 *   obligation's first deposit into a reserve with a farm, `init_obligation_farms_for_reserve` must
 *   create its user state.
 *
 * Kamino mints whole cTokens only, and takes just what they are worth: of the amount it is asked
 * for, less than one cToken's worth (a base unit or so) can stay in `destinationAta`.
 *
 * Kamino takes a deposit only into an obligation refreshed in the same slot. It does not care
 * where in the transaction that happened, so the refreshes belong to the transaction, not the
 * template. Put `refresh_reserve` for each reserve the obligation holds, then `refresh_obligation`
 * with those reserves, before this run.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '../../src/index.js';
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_DEPOSIT,
  KAMINO_LEND,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

export const jupiterDepositExactOutput = defineTemplate({
  inputs: {
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`. */
    inAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount`. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`, at most `MAX_PLATFORM_FEE_BPS`. */
    platformFeeBps: { type: 'u64' },
    /** Below this the route is not worth depositing and the run fails instead. */
    minimumOut: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    owner: { signer: true, writable: true },
    /** What the route sells from. */
    sourceAta: { writable: true },
    /** The route's destination, and the account the deposit draws from. */
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    reserve: { writable: true },
    reserveLiquidityMint: {},
    reserveLiquiditySupply: { writable: true },
    reserveCollateralMint: { writable: true },
    reserveDestinationDepositCollateral: { writable: true },
  },
  /**
   * `routeAccounts`: Jupiter's own list, whose length depends on the route. `farmAccounts`:
   * Kamino's v2 tail, described above.
   */
  accountGroups: ['routeAccounts', 'farmAccounts'],
  steps: [
    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeSwap',
    ),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),
    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('owner'), signer: true, writable: false },
        { account: account.fixed('sourceAta'), signer: false, writable: true },
        { account: account.fixed('destinationAta'), signer: false, writable: true },
      ],
      accountGroup: 'routeAccounts',
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'swap',
    }),

    step.let(
      'received',
      expression.subtract(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.snapshot('balanceBefore'),
      ),
      'measureSwapOutput',
    ),

    step.require(
      expression.greaterThanOrEqual(expression.variable('received'), expression.input('minimumOut')),
      'swapMetItsFloor',
    ),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('owner'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('reserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('reserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('reserveDestinationDepositCollateral'), signer: false, writable: true },
        // The deposit draws from the account the swap paid into.
        { account: account.fixed('destinationAta'), signer: false, writable: true },
        // `placeholder_user_destination_collateral`, never used: the Kamino program means "none".
        { account: account.fixed('kamino'), signer: false, writable: false },
        // `collateral_token_program`, then `liquidity_token_program`.
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_DEPOSIT),
        // Exactly what the swap produced, measured a moment ago.
        data.encode('u64', expression.variable('received')),
      ],
      label: 'depositSwapOutput',
    }),
  ],
});

export const compiled = compileTemplate(jupiterDepositExactOutput);
