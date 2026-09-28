/**
 * Collect an Orca Whirlpool position's fees and reinvest them in the position.
 *
 * A position's `fee_owed_a` and `fee_owed_b` hold what its last update recorded, not what it has
 * earned since. Swaps raise the pool's fee growth, and `update_fees_and_rewards` is what folds that
 * growth into the position. Orca's own SDK calls it before every collect, and so does this
 * template, before it reads the fees. The update needs no signature. It fails with `LiquidityZero`
 * (6012) on a position without liquidity, which earns nothing, so it is skipped for one.
 *
 * The reinvestment is `increase_liquidity_by_token_amounts_v2`, as in Orca's SDK. Given the two
 * fees as caps, the program works out the most liquidity they buy at the price when the block
 * runs, so nothing is topped up from the wallet and a price move within the bounds below does not
 * matter. One fee is used up and part of the other stays in the wallet. `increase_liquidity`, by
 * contrast, takes a liquidity chosen at signing and fails with `TokenMaxExceeded` (6017) once the
 * price has moved or the fees are in one token.
 *
 * While the price is inside the position's range, liquidity takes both tokens: with either cap at
 * zero the program works out none and fails with `LiquidityZero`. So fees in one token are
 * collected and not reinvested. Nor are the fees of a position emptied with `decrease_liquidity`:
 * they are collected, and the position stays empty.
 *
 * `minSqrtPrice` and `maxSqrtPrice` bound the pool price the deposit accepts. Orca's
 * `get_sqrt_price_slippage_bounds` computes them from a price and a tolerance. Outside them the
 * deposit fails with `PriceSlippageOutOfBounds` (6069), and the whole run reverts with it.
 *
 * `collect_fees` and the pinned token program are SPL Token's, so both of the pool's mints must be
 * SPL Token mints, as SOL and USDC are.
 *
 * With nothing above `dustFloor` in either token, the collect and the deposit are skipped and the
 * run lands: a scheduled compounder that finds nothing to do should not revert and burn the fee.
 *
 * `dustFloor` is not safe at 0. A fee just above the floor can still be too small to buy one unit
 * of liquidity over the position's range, and the deposit then fails with `LiquidityZero` (6012),
 * taking the whole run down with it, collect included
 * (`dust_that_buys_no_liquidity_fails_the_run_unless_the_floor_skips_it`). A few base units covers
 * a SOL/USDC position; a pool whose token A is worth less per base unit needs more, so a few
 * thousand base units is a safer default.
 *
 * `tokenOwnerAccountA` and `tokenOwnerAccountB` must belong to `positionAuthority`. Whirlpools'
 * `collect_fees` checks only their mint, never who owns them, so an untrusted run builder could
 * otherwise send real fees to its own accounts while the owner just signs; the template requires
 * it itself (`feesGoToTheOwner`).
 *
 * Offsets come from `Position`, declared as `whirlpool, position_mint, liquidity,
 * tick_lower_index, tick_upper_index, fee_growth_checkpoint_a, fee_owed_a,
 * fee_growth_checkpoint_b, fee_owed_b, reward_infos` with `LEN = 8 + 136 + 72`.
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
     * 0: see the header.
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
    positionTokenAccount: {},
    tokenMintA: {},
    tokenMintB: {},
    /** Must belong to `positionAuthority`: see the header (`feesGoToTheOwner`). */
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
    // by someone other than the owner from pointing them elsewhere.
    step.require(
      expression.and(
        expression.equal(
          expression.accountData(account.fixed('tokenOwnerAccountA'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
          expression.accountField(account.fixed('positionAuthority'), 'key'),
        ),
        expression.equal(
          expression.accountData(account.fixed('tokenOwnerAccountB'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
          expression.accountField(account.fixed('positionAuthority'), 'key'),
        ),
      ),
      'feesGoToTheOwner',
    ),
    step.let(
      'hasLiquidity',
      expression.greaterThan(
        expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
        expression.u128(0),
      ),
      'readLiquidity',
    ),
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
