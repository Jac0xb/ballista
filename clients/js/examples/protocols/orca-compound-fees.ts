/**
 * Collect Orca Whirlpool fees and reinvest exactly what was collected.
 *
 * `increase_liquidity(liquidity_amount: u128, token_max_a: u64, token_max_b: u64)` takes caps,
 * not amounts. Pass caps that are too low and the call fails; too high and you top up from the
 * wallet without meaning to. The right caps are the fees the position had actually earned when
 * the block ran, which is a number that does not exist at signing.
 *
 * Offsets come from `Position`, declared as `whirlpool, position_mint, liquidity,
 * tick_lower_index, tick_upper_index, fee_growth_checkpoint_a, fee_owed_a,
 * fee_growth_checkpoint_b, fee_owed_b, reward_infos` with `LEN = 8 + 136 + 72`.
 *
 * The `when` guard is the other half: a scheduled compounder that finds nothing to compound
 * should land a no-op, not revert and burn the fee.
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
  ORCA_INCREASE_LIQUIDITY,
  ORCA_POSITION,
  ORCA_WHIRLPOOL,
  addressBytes,
} from './shared.js';

export const orcaCompoundFees = defineTemplate({
  inputs: {
    /** How much liquidity the caller wants minted; the token caps below bound what it costs. */
    liquidityAmount: { type: 'u128' },
    /** Below this the fees are not worth a transaction. */
    dustFloor: { type: 'u64' },
  },
  accounts: {
    whirlpoolProgram: { executable: true, address: addressBytes(ORCA_WHIRLPOOL) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    positionAuthority: { signer: true },
    whirlpool: { writable: true },
    /** Owner-pinned so `fee_owed_a` and `fee_owed_b` are read from a real Whirlpool position. */
    position: {
      writable: true,
      owner: addressBytes(ORCA_WHIRLPOOL),
      minDataLength: ORCA_POSITION.length,
    },
    positionTokenAccount: {},
    tokenOwnerAccountA: { writable: true },
    tokenOwnerAccountB: { writable: true },
    tokenVaultA: { writable: true },
    tokenVaultB: { writable: true },
    tickArrayLower: { writable: true },
    tickArrayUpper: { writable: true },
  },
  steps: [
    // Read the owed fees before collecting, because collecting zeroes them.
    step.let(
      'owedA',
      expression.accountData(account.fixed('position'), ORCA_POSITION.feeOwedA, 'u64'),
      'readFeesOwedA',
    ),
    step.let(
      'owedB',
      expression.accountData(account.fixed('position'), ORCA_POSITION.feeOwedB, 'u64'),
      'readFeesOwedB',
    ),

    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: account.fixed('positionAuthority'), signer: true, writable: false },
        { account: account.fixed('position'), signer: false, writable: true },
        { account: account.fixed('positionTokenAccount'), signer: false, writable: false },
        { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
        { account: account.fixed('tokenVaultA'), signer: false, writable: true },
        { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
        { account: account.fixed('tokenVaultB'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [data.literal(ORCA_COLLECT_FEES)],
      // Nothing earned, nothing to do, and no reason to fail the crank.
      when: expression.greaterThan(expression.variable('owedA'), expression.input('dustFloor')),
      label: 'collectFees',
    }),

    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('positionAuthority'), signer: true, writable: false },
        { account: account.fixed('position'), signer: false, writable: true },
        { account: account.fixed('positionTokenAccount'), signer: false, writable: false },
        { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
        { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
        { account: account.fixed('tokenVaultA'), signer: false, writable: true },
        { account: account.fixed('tokenVaultB'), signer: false, writable: true },
        { account: account.fixed('tickArrayLower'), signer: false, writable: true },
        { account: account.fixed('tickArrayUpper'), signer: false, writable: true },
      ],
      data: [
        data.literal(ORCA_INCREASE_LIQUIDITY),
        data.encode('u128', expression.input('liquidityAmount')),
        // The caps are the fees that were actually owed, so the top-up is exact.
        data.encode('u64', expression.variable('owedA')),
        data.encode('u64', expression.variable('owedB')),
      ],
      when: expression.greaterThan(expression.variable('owedA'), expression.input('dustFloor')),
      label: 'compoundFees',
    }),
  ],
});

export const compiled = compileTemplate(orcaCompoundFees);
