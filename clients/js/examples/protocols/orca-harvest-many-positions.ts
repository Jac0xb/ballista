/** Harvest positions that earned: docs/examples/protocols/orca-harvest.md. */
// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';
import {
  ORCA_COLLECT_FEES,
  ORCA_POSITION,
  ORCA_UPDATE_FEES_AND_REWARDS,
  ORCA_WHIRLPOOL,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
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
    /** Must belong to each row's position holder (`positionBelongsToTheFeeOwner`). */
    tokenOwnerAccountA: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    tokenOwnerAccountB: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
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
      /**
       * Read for its owner field, the row's real holder (`positionBelongsToTheFeeOwner`). A row's
       * NFT can be held by either Token or Token-2022, so its owning program is not pinned;
       * Whirlpools' own mint and amount checks on this account make that read trustworthy without
       * one.
       */
      positionTokenAccount: { unsafeUnpinned: true, minDataLength: TOKEN_ACCOUNT_LENGTH },
      /** The tick array holding the position's lower tick; `update_fees_and_rewards` reads it. */
      tickArrayLower: {},
      /** The tick array holding the position's upper tick. */
      tickArrayUpper: {},
    },
  },
  steps: [
    // Fixed accounts, shared by every row: read once for the whole batch, not once per row.
    step.let(
      'feeOwnerA',
      expression.accountData(account.fixed('tokenOwnerAccountA'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
      'readFeeOwnerA',
    ),
    step.let(
      'feeOwnerB',
      expression.accountData(account.fixed('tokenOwnerAccountB'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
      'readFeeOwnerB',
    ),
    step.forEach(
      [
        step.let(
          'positionHolder',
          expression.accountData(
            account.iteration('positionTokenAccount'),
            TOKEN_ACCOUNT_OWNER_OFFSET,
            'pubkey',
          ),
          'readPositionHolder',
        ),
        // Whirlpools checks only the fee accounts' mints. The holder is the NFT account's owner, not
        // `positionAuthority`, which may be a delegate.
        step.require(
          expression.and(
            expression.equal(expression.variable('positionHolder'), expression.variable('feeOwnerA')),
            expression.equal(expression.variable('positionHolder'), expression.variable('feeOwnerB')),
          ),
          'positionBelongsToTheFeeOwner',
        ),
        // Folds the pool's fee growth into the position, so the owed fees are current. Whirlpools
        // refuses it for a position without liquidity, which earns nothing.
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
// #endregion template
