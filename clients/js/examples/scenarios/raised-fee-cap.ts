/**
 * `jupiterOracleCheckedSwap` with its `MAX_PLATFORM_FEE_BPS` raised from 0 to 100, as an author
 * running their own frontend might raise it. `tests/protocols/tests/oracle_checked_swap.rs` runs
 * it to show that a fee within the cap lands and one above it fails at `platformFeeWithinCap`.
 */
import { expression, step, type Template } from '@jac0xb/ballista';
import { jupiterOracleCheckedSwap } from '../protocols/jupiter-oracle-checked-swap.js';

/** 1%: the raised cap. */
export const RAISED_PLATFORM_FEE_BPS = 100n;

export const jupiterOracleCheckedSwapFeeCap100: Template = {
  ...jupiterOracleCheckedSwap,
  steps: jupiterOracleCheckedSwap.steps.map((entry) =>
    entry.kind === 'require' && entry.label === 'platformFeeWithinCap'
      ? step.require(
          expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(RAISED_PLATFORM_FEE_BPS)),
          'platformFeeWithinCap',
        )
      : entry,
  ),
};
