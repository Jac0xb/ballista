/**
 * Harvest fees from a page of Orca positions, skipping the ones that earned nothing.
 *
 * A liquidity manager holds dozens of positions. Most have earned something since the last
 * harvest and some have not, and which is which depends on trades that land after the transaction
 * is signed.
 *
 * So each row first calls `update_fees_and_rewards`, which folds the pool's fee growth into the
 * position. Without it, `fee_owed_a` and `fee_owed_b` hold only what the last update recorded, and
 * a row that had earned would look empty. Then the row collects if either fee is above
 * `dustFloor`. The update is skipped for a position without liquidity, where it fails with
 * `LiquidityZero` (6012) and has nothing to record.
 *
 * Skipping a row saves compute, about 11,000 units per collect, and leaves dust alone. It does not
 * prevent reverts: `collect_fees` with nothing owed succeeds and moves nothing. What reverts the
 * whole harvest is a row Whirlpools refuses, such as a position the signer does not hold
 * (`MissingOrInvalidDelegate`, 6019) or one from another pool (`ConstraintHasOne`, 2001). Those do
 * not depend on trades, so filter them out before building the run.
 *
 * A row is the position, the token account holding its NFT, and the tick arrays holding its lower
 * and upper ticks. A row that collects costs about 24,000 compute units, so eight fit the default
 * limit of 200,000 and more need a compute budget. Rows share keys when they share tick arrays: then
 * about ten fit a legacy transaction, and twelve, the template's limit, need a lookup table.
 *
 * Offsets come from `Position`, `LEN = 8 + 136 + 72`: `liquidity` at 72, `fee_owed_a` at 112 and
 * `fee_owed_b` at 136.
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
  ORCA_COLLECT_FEES,
  ORCA_POSITION,
  ORCA_UPDATE_FEES_AND_REWARDS,
  ORCA_WHIRLPOOL,
  addressBytes,
} from './shared.js';

const position = account.iteration('position');
const aboveFloor = (offset: number) =>
  expression.greaterThan(
    expression.accountData(position, offset, 'u64'),
    expression.input('dustFloor'),
  );

export const orcaHarvestManyPositions = defineTemplate({
  inputs: {
    /** Fees at or below this, in either token's base units, are left for a later harvest. */
    dustFloor: { type: 'u64' },
  },
  accounts: {
    whirlpoolProgram: { executable: true, address: addressBytes(ORCA_WHIRLPOOL) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    positionAuthority: { signer: true },
    /** Written by each row's `update_fees_and_rewards`. */
    whirlpool: { writable: true },
    tokenOwnerAccountA: { writable: true },
    tokenOwnerAccountB: { writable: true },
    tokenVaultA: { writable: true },
    tokenVaultB: { writable: true },
  },
  batch: {
    maxIterations: 12,
    minIterations: 1,
    row: {
      position: {
        writable: true,
        owner: addressBytes(ORCA_WHIRLPOOL),
        minDataLength: ORCA_POSITION.length,
      },
      positionTokenAccount: {},
      /** The tick array holding the position's lower tick; `update_fees_and_rewards` reads it. */
      tickArrayLower: {},
      /** The tick array holding the position's upper tick. */
      tickArrayUpper: {},
    },
  },
  steps: [
    step.forEach(
      [
        step.invoke({
          program: account.fixed('whirlpoolProgram'),
          accounts: [
            { account: account.fixed('whirlpool'), signer: false, writable: true },
            { account: position, signer: false, writable: true },
            { account: account.iteration('tickArrayLower'), signer: false, writable: false },
            { account: account.iteration('tickArrayUpper'), signer: false, writable: false },
          ],
          data: [data.literal(ORCA_UPDATE_FEES_AND_REWARDS)],
          when: expression.greaterThan(
            expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
            expression.u128(0),
          ),
          label: 'updateIfLiquid',
        }),
        step.invoke({
          program: account.fixed('whirlpoolProgram'),
          accounts: [
            { account: account.fixed('whirlpool'), signer: false, writable: false },
            { account: account.fixed('positionAuthority'), signer: true, writable: false },
            { account: position, signer: false, writable: true },
            { account: account.iteration('positionTokenAccount'), signer: false, writable: false },
            { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
            { account: account.fixed('tokenVaultA'), signer: false, writable: true },
            { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
            { account: account.fixed('tokenVaultB'), signer: false, writable: true },
            { account: account.fixed('tokenProgram'), signer: false, writable: false },
          ],
          data: [data.literal(ORCA_COLLECT_FEES)],
          // This row's own fees, just updated, decide whether it collects.
          when: expression.or(aboveFloor(ORCA_POSITION.feeOwedA), aboveFloor(ORCA_POSITION.feeOwedB)),
          label: 'collectIfWorthIt',
        }),
      ],
      { label: 'everyPosition' },
    ),
  ],
});

export const compiled = compileTemplate(orcaHarvestManyPositions);
