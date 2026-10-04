/** Compound collected fees: docs/examples/protocols/orca-compound.md. */
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
  MEMO_PROGRAM,
  OPTION_NONE,
  ORCA_BY_TOKEN_AMOUNTS,
  ORCA_COLLECT_FEES,
  ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2,
  ORCA_POSITION,
  ORCA_UPDATE_FEES_AND_REWARDS,
  ORCA_WHIRLPOOL,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

const position = account.fixed('position');

export const orcaCompoundFees = defineTemplate({
  inputs: {
    /**
     * Fees at or below this, in either token's base units, are not worth collecting. Not safe at
     * 0: a fee too small to buy any liquidity fails the deposit, and the whole run with it.
     */
    dustFloor: { type: 'u64' },
    /** The lowest pool sqrt price (Q64.64) the deposit accepts. */
    minSqrtPrice: { type: 'u128' },
    /** The highest pool sqrt price (Q64.64) the deposit accepts. */
    maxSqrtPrice: { type: 'u128' },
  },
  accounts: {
    whirlpoolProgram: { executable: true, address: addressBytes(ORCA_WHIRLPOOL) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    memoProgram: { executable: true, address: addressBytes(MEMO_PROGRAM) },
    positionAuthority: { signer: true },
    whirlpool: { writable: true },
    /** Owner-pinned so `liquidity` and the owed fees are read from a real Whirlpool position. */
    position: {
      writable: true,
      owner: addressBytes(ORCA_WHIRLPOOL),
      minDataLength: ORCA_POSITION.length,
    },
    /**
     * Read for its owner field, the position's real holder (`feesGoToThePositionHolder`). Could be
     * Token- or Token-2022-owned, so its owning program is not pinned; Whirlpools' own mint and
     * amount checks on this account make that read trustworthy without one.
     */
    positionTokenAccount: { unsafeUnpinned: true, minDataLength: TOKEN_ACCOUNT_LENGTH },
    tokenMintA: {},
    tokenMintB: {},
    /** Must belong to the position's holder (`feesGoToThePositionHolder`). */
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
    tickArrayLower: { writable: true },
    tickArrayUpper: { writable: true },
  },
  steps: [
    // Whirlpools' collect_fees checks only the mint of these accounts; nothing stops a run built
    // by someone other than the position's holder from pointing them elsewhere. The holder is
    // positionTokenAccount's owner, not positionAuthority, which may only be its delegate.
    step.let(
      'positionHolder',
      expression.accountData(account.fixed('positionTokenAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
      'readPositionHolder',
    ),
    step.require(
      expression.and(
        expression.equal(
          expression.accountData(account.fixed('tokenOwnerAccountA'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
          expression.variable('positionHolder'),
        ),
        expression.equal(
          expression.accountData(account.fixed('tokenOwnerAccountB'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
          expression.variable('positionHolder'),
        ),
      ),
      'feesGoToThePositionHolder',
    ),
    step.let(
      'hasLiquidity',
      expression.greaterThan(
        expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
        expression.u128(0),
      ),
      'readLiquidity',
    ),
    // Folds the pool's fee growth into the position, so the owed fees are current. Whirlpools
    // refuses it for a position without liquidity, which earns nothing.
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: position, signer: false, writable: true },
        { account: account.fixed('tickArrayLower'), signer: false, writable: false },
        { account: account.fixed('tickArrayUpper'), signer: false, writable: false },
      ],
      data: [data.literal(ORCA_UPDATE_FEES_AND_REWARDS)],
      when: expression.variable('hasLiquidity'),
      label: 'updateFees',
    }),
    // Read after the update, which makes them current, and before the collect, which zeroes them.
    step.let('owedA', expression.accountData(position, ORCA_POSITION.feeOwedA, 'u64'), 'readFeesOwedA'),
    step.let('owedB', expression.accountData(position, ORCA_POSITION.feeOwedB, 'u64'), 'readFeesOwedB'),
    step.let(
      'earnedA',
      expression.greaterThan(expression.variable('owedA'), expression.input('dustFloor')),
    ),
    step.let(
      'earnedB',
      expression.greaterThan(expression.variable('owedB'), expression.input('dustFloor')),
    ),
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: false },
        { account: account.fixed('positionAuthority'), signer: true, writable: false },
        { account: position, signer: false, writable: true },
        { account: account.fixed('positionTokenAccount'), signer: false, writable: false },
        { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
        { account: account.fixed('tokenVaultA'), signer: false, writable: true },
        { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
        { account: account.fixed('tokenVaultB'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [data.literal(ORCA_COLLECT_FEES)],
      // Either fee is worth collecting.
      when: expression.or(expression.variable('earnedA'), expression.variable('earnedB')),
      label: 'collectFees',
    }),
    // By token amounts: with the fees as caps, Whirlpools works out the most liquidity they buy at
    // the price when it runs. `increase_liquidity` would take a liquidity fixed at signing.
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('memoProgram'), signer: false, writable: false },
        { account: account.fixed('positionAuthority'), signer: true, writable: false },
        { account: position, signer: false, writable: true },
        { account: account.fixed('positionTokenAccount'), signer: false, writable: false },
        { account: account.fixed('tokenMintA'), signer: false, writable: false },
        { account: account.fixed('tokenMintB'), signer: false, writable: false },
        { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
        { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
        { account: account.fixed('tokenVaultA'), signer: false, writable: true },
        { account: account.fixed('tokenVaultB'), signer: false, writable: true },
        { account: account.fixed('tickArrayLower'), signer: false, writable: true },
        { account: account.fixed('tickArrayUpper'), signer: false, writable: true },
      ],
      data: [
        data.literal(ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2),
        data.literal(ORCA_BY_TOKEN_AMOUNTS),
        data.encode('u64', expression.variable('owedA')),
        data.encode('u64', expression.variable('owedB')),
        data.encode('u128', expression.input('minSqrtPrice')),
        data.encode('u128', expression.input('maxSqrtPrice')),
        data.literal(OPTION_NONE),
      ],
      // In range, liquidity needs both tokens; an emptied position stays empty.
      when: expression.and(
        expression.variable('hasLiquidity'),
        expression.and(expression.variable('earnedA'), expression.variable('earnedB')),
      ),
      label: 'compoundFees',
    }),
  ],
});

export const compiled = compileTemplate(orcaCompoundFees);
// #endregion template
