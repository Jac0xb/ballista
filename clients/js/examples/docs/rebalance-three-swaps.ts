// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
  type Step,
} from '@jac0xb/ballista';
import { JUPITER_V6, addressBytes } from '../protocols/shared.js';

// SPL Token account layout: the owner is the pubkey at byte 32, the amount the u64 at byte 64.
const OWNER_OFFSET = 32;
const AMOUNT_OFFSET = 64;
// No fixed mint: the caller decides which tokens are involved, and the checks decide whether the
// result is acceptable. The owner and size pins let the template read the accounts' data.
const tokenAccount = { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } as const;

/** The five steps of one optional swap: leg `A` uses sourceA, destinationA, routeA and group ammA. */
function swapLeg(leg: 'A' | 'B' | 'C'): Step[] {
  const destination = account.fixed(`destination${leg}`);
  const needed = expression.variable(`need${leg}`);
  return [
    step.snapshot(`balance${leg}`, expression.accountData(destination, AMOUNT_OFFSET, 'u64')),
    step.let(`need${leg}`, expression.lessThan(expression.snapshot(`balance${leg}`), expression.input(`target${leg}`))),
    step.require(
      expression.equal(
        expression.accountData(destination, OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('user'), 'key'),
      ),
    ),
    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [
        // Jupiter's leading accounts, in its order; the quote's pools follow as the group.
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('user'), signer: true, writable: false },
        { account: account.fixed(`source${leg}`), signer: false, writable: true },
        { account: destination, signer: false, writable: true },
      ],
      accountGroup: `amm${leg}`,
      data: [data.encode('bytes', expression.input(`route${leg}`))],
      when: needed,
    }),
    // Skipped, or the balance rose by at least the minimum.
    step.require(
      expression.or(
        expression.not(needed),
        expression.greaterThanOrEqual(
          expression.subtract(
            expression.accountData(destination, AMOUNT_OFFSET, 'u64'),
            expression.snapshot(`balance${leg}`),
          ),
          expression.input(`minOut${leg}`),
        ),
      ),
    ),
  ];
}

/** Three optional swaps, each forwarding its own account group, each checked afterwards. */
export const rebalanceThreeSwaps = defineTemplate({
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    user: { signer: true, writable: true },
    sourceA: tokenAccount,
    destinationA: tokenAccount,
    sourceB: tokenAccount,
    destinationB: tokenAccount,
    sourceC: tokenAccount,
    destinationC: tokenAccount,
  },
  inputs: {
    routeA: { type: 'bytes', maxLength: 256 },
    routeB: { type: 'bytes', maxLength: 256 },
    routeC: { type: 'bytes', maxLength: 256 },
    targetA: { type: 'u64' },
    targetB: { type: 'u64' },
    targetC: { type: 'u64' },
    minOutA: { type: 'u64' },
    minOutB: { type: 'u64' },
    minOutC: { type: 'u64' },
  },
  accountGroups: ['ammA', 'ammB', 'ammC'],
  steps: [...swapLeg('A'), ...swapLeg('B'), ...swapLeg('C')],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction, type KitAccountBinding } from '@jac0xb/ballista/kit';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

/**
 * One swap: its token accounts, its quote's route data and pool accounts, and its limits. A swap
 * that is not needed passes empty route data and no pools.
 */
export interface SwapLeg {
  source: Address;
  destination: Address;
  route: Uint8Array;
  pools: readonly KitAccountBinding[];
  target: bigint;
  minOut: bigint;
}

export function runRebalanceThreeSwaps(run: {
  templateAddress: Address;
  user: Address;
  legs: readonly [SwapLeg, SwapLeg, SwapLeg];
}) {
  const [a, b, c] = run.legs;
  return buildKitRunInstruction({
    compiled: compileTemplate(rebalanceThreeSwaps),
    templateAddress: run.templateAddress,
    accounts: {
      jupiter: { address: address(JUPITER_V6) },
      tokenProgram: { address: TOKEN_PROGRAM },
      user: { address: run.user },
      sourceA: { address: a.source },
      destinationA: { address: a.destination },
      sourceB: { address: b.source },
      destinationB: { address: b.destination },
      sourceC: { address: c.source },
      destinationC: { address: c.destination },
    },
    inputs: {
      routeA: a.route,
      routeB: b.route,
      routeC: c.route,
      targetA: a.target,
      targetB: b.target,
      targetC: c.target,
      minOutA: a.minOut,
      minOutB: b.minOut,
      minOutC: c.minOut,
    },
    // Each member is `{ address, writable }`, as the quote lists it. Members never sign.
    accountGroups: { ammA: a.pools, ammB: b.pools, ammC: c.pools },
  });
}
// #endregion run
