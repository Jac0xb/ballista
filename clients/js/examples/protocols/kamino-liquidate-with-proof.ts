/**
 * Liquidate a Kamino obligation and prove the liquidator came out ahead.
 *
 * `liquidate_obligation_and_redeem_reserve_collateral_v2(liquidity_amount,
 * min_acceptable_received_liquidity_amount, max_allowed_ltv_override_percent)` repays part of an
 * unhealthy obligation's debt and seizes collateral in return. In the same instruction it redeems
 * the seized cTokens and pays the underlying to `userDestinationLiquidity`, less Kamino's
 * protocol fee. If the reserve cannot redeem all of it, the rest stays in
 * `userDestinationCollateral` as cTokens.
 *
 * So the template measures `userDestinationLiquidity`, and requires it to have grown by at least
 * `minimumBounty`, in the collateral's own units. Anything less, including a payout left in
 * cTokens, and the run reverts. The reverted transaction still pays its fee.
 * - `minimumBounty` is the runner's bar, for example the repaid amount valued at the oracle price
 *   plus the margin worth liquidating for.
 * - Kamino's own `min_acceptable_received_liquidity_amount` is a floor on its computed liquidity
 *   leg. It is not a measurement of what arrived.
 *
 * Kamino checks the mints of the accounts it pays, not whose they are, and the run names them. So
 * the liquidator must own both: `userDestinationLiquidity` (`bountyGoesToTheLiquidator`) and
 * `userDestinationCollateral` (`seizedCollateralGoesToTheLiquidator`). Otherwise a run could pay
 * the seized collateral to someone else, and the bounty would be measured on their account.
 *
 * The liquidation is Kamino's `_v2` handler; the v1 handler refuses every caller but Kamino itself
 * and a short whitelist. v2 takes the 20 accounts declared below, then `farmAccounts`:
 * - the borrower's user state in the withdrawn reserve's collateral farm, and that farm;
 * - the borrower's user state in the repaid reserve's debt farm, and that farm;
 * - the Farms program.
 * The Kamino program stands in for each farm account the reserve does not have. A group carries
 * them so each keeps its own writable flag.
 *
 * Kamino liquidates only against both reserves and the obligation refreshed in the same slot, and
 * refuses a healthy obligation (`ObligationHealthy`). Put `refresh_reserve` for each reserve the
 * obligation holds, then `refresh_obligation` with them, before this run.
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
  KAMINO_LEND,
  KAMINO_LIQUIDATE,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

export const kaminoLiquidateWithProof = defineTemplate({
  inputs: {
    /** How much debt to repay on behalf of the borrower. */
    liquidityAmount: { type: 'u64' },
    /** Kamino's own floor on the liquidity leg. */
    minAcceptableReceived: { type: 'u64' },
    /** What the liquidator must receive, in the seized collateral's underlying token. */
    minimumBounty: { type: 'u64' },
  },
  accounts: {
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    /** Kamino declares it a bare signer, so it is read-only. */
    liquidator: { signer: true },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    repayReserve: { writable: true },
    repayReserveLiquidityMint: {},
    repayReserveLiquiditySupply: { writable: true },
    withdrawReserve: { writable: true },
    withdrawReserveLiquidityMint: {},
    withdrawReserveCollateralMint: { writable: true },
    withdrawReserveCollateralSupply: { writable: true },
    withdrawReserveLiquiditySupply: { writable: true },
    /** Where Kamino's protocol fee on the seized collateral goes: the withdrawn reserve's fee vault. */
    withdrawReserveFeeReceiver: { writable: true },
    /** Pays the repayment. */
    userSourceLiquidity: { writable: true },
    /** Receives the seized cTokens, which Kamino redeems in the same instruction. */
    userDestinationCollateral: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    /** Receives the redeemed collateral: the account the bounty is measured on. */
    userDestinationLiquidity: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  /** Kamino's v2 tail, described above. */
  accountGroups: ['farmAccounts'],
  steps: [
    step.require(
      expression.equal(
        expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('liquidator'), 'key'),
      ),
      'bountyGoesToTheLiquidator',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('userDestinationCollateral'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('liquidator'), 'key'),
      ),
      'seizedCollateralGoesToTheLiquidator',
    ),

    step.snapshot(
      'payoutBefore',
      expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readPayoutBefore',
    ),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('liquidator'), signer: true, writable: false },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('repayReserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('repayReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserve'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('withdrawReserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveCollateralSupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveFeeReceiver'), signer: false, writable: true },
        { account: account.fixed('userSourceLiquidity'), signer: false, writable: true },
        { account: account.fixed('userDestinationCollateral'), signer: false, writable: true },
        { account: account.fixed('userDestinationLiquidity'), signer: false, writable: true },
        // `collateral_token_program`, `repay_liquidity_token_program`, `withdraw_liquidity_token_program`.
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_LIQUIDATE),
        data.encode('u64', expression.input('liquidityAmount')),
        data.encode('u64', expression.input('minAcceptableReceived')),
        // No LTV override: liquidate on the protocol's own terms.
        data.encode('u64', expression.u64(0)),
      ],
      label: 'liquidate',
    }),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
          expression.snapshot('payoutBefore'),
        ),
        expression.input('minimumBounty'),
      ),
      'liquidationPaidTheBounty',
    ),
  ],
});

export const compiled = compileTemplate(kaminoLiquidateWithProof);
