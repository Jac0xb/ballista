/**
 * Liquidate a Kamino obligation and prove the liquidator came out ahead.
 *
 * `liquidate_obligation_and_redeem_reserve_collateral_v2(liquidity_amount,
 * min_acceptable_received_liquidity_amount, max_allowed_ltv_override_percent)` has a minimum on
 * the liquidity leg. It does not bound what the liquidator nets across the whole operation, and
 * a liquidation is only priced correctly against a freshly refreshed reserve and obligation —
 * refreshes that happen in this same transaction, after signing.
 *
 * So the template refreshes, liquidates, and then requires the liquidator's collateral account to
 * have grown by at least the bounty it was chasing. Anything less and the run reverts: no
 * half-executed liquidation, no paying gas to improve someone else's position.
 *
 * Gating the liquidation on health itself is also possible — read the obligation's borrowed and
 * unhealthy-borrow values and attach a `when` — but those offsets are not derived here. Take
 * them from the current klend IDL if you want the skip-instead-of-revert behaviour.
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
  KAMINO_REFRESH_OBLIGATION,
  KAMINO_REFRESH_RESERVE,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const kaminoLiquidateWithProof = defineTemplate({
  inputs: {
    /** How much debt to repay on behalf of the borrower. */
    liquidityAmount: { type: 'u64' },
    /** The protocol's own floor on the liquidity leg. */
    minAcceptableReceived: { type: 'u64' },
    /** What the liquidator must net in collateral, across the whole operation. */
    minimumBounty: { type: 'u64' },
  },
  accounts: {
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    liquidator: { signer: true, writable: true },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    repayReserve: { writable: true },
    repayReserveLiquiditySupply: { writable: true },
    withdrawReserve: { writable: true },
    withdrawReserveCollateralMint: { writable: true },
    withdrawReserveLiquiditySupply: { writable: true },
    reservePriceFeed: {},
    userSourceLiquidity: { writable: true },
    /** Where the seized collateral lands. */
    userDestinationCollateral: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  steps: [
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('withdrawReserve'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('reservePriceFeed'), signer: false, writable: false },
      ],
      data: [data.literal(KAMINO_REFRESH_RESERVE)],
      label: 'refreshReserve',
    }),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('obligation'), signer: false, writable: true },
      ],
      data: [data.literal(KAMINO_REFRESH_OBLIGATION)],
      label: 'refreshObligation',
    }),

    step.snapshot(
      'collateralBefore',
      expression.accountData(
        account.fixed('userDestinationCollateral'),
        TOKEN_ACCOUNT_AMOUNT_OFFSET,
        'u64',
      ),
      'readCollateralBefore',
    ),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('liquidator'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('repayReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserve'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('userSourceLiquidity'), signer: false, writable: true },
        { account: account.fixed('userDestinationCollateral'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
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
          expression.accountData(
            account.fixed('userDestinationCollateral'),
            TOKEN_ACCOUNT_AMOUNT_OFFSET,
            'u64',
          ),
          expression.snapshot('collateralBefore'),
        ),
        expression.input('minimumBounty'),
      ),
      'liquidationPaidTheBounty',
    ),
  ],
});

export const compiled = compileTemplate(kaminoLiquidateWithProof);
