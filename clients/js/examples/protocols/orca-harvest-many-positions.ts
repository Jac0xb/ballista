/**
 * Harvest fees from a page of Orca positions, skipping the ones that earned nothing.
 *
 * A liquidity manager holds dozens of positions. Most of them have earned something since the
 * last harvest and some have not, and which is which depends on trades that happen after the
 * transaction is signed.
 *
 * Sending one `collect_fees` per position reverts the entire batch on the first position the
 * protocol refuses, and pre-filtering off chain races the block: a position that looked empty
 * when the list was built may have earned by the time it lands, and vice versa. The row count
 * here is fixed by the account list, but whether each row does anything is decided during
 * execution, from that row's own `fee_owed_a`.
 *
 * Offsets come from `Position`, `LEN = 8 + 136 + 72`, with `fee_owed_a` at 112.
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
import { ORCA_COLLECT_FEES, ORCA_POSITION, ORCA_WHIRLPOOL, addressBytes } from './shared.js';

export const orcaHarvestManyPositions = defineTemplate({
  inputs: {
    /** Positions under this are left alone, so the harvest does not cost more than it collects. */
    dustFloor: { type: 'u64' },
  },
  accounts: {
    whirlpoolProgram: { executable: true, address: addressBytes(ORCA_WHIRLPOOL) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    positionAuthority: { signer: true },
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
    },
  },
  steps: [
    step.forEach(
      [
        step.invoke({
          program: account.fixed('whirlpoolProgram'),
          accounts: [
            { account: account.fixed('whirlpool'), signer: false, writable: true },
            { account: account.fixed('positionAuthority'), signer: true, writable: false },
            { account: account.iteration('position'), signer: false, writable: true },
            { account: account.iteration('positionTokenAccount'), signer: false, writable: false },
            { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
            { account: account.fixed('tokenVaultA'), signer: false, writable: true },
            { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
            { account: account.fixed('tokenVaultB'), signer: false, writable: true },
            { account: account.fixed('tokenProgram'), signer: false, writable: false },
          ],
          data: [data.literal(ORCA_COLLECT_FEES)],
          // This row's own earnings decide whether this row does anything.
          when: expression.greaterThan(
            expression.accountData(account.iteration('position'), ORCA_POSITION.feeOwedA, 'u64'),
            expression.input('dustFloor'),
          ),
          label: 'collectIfWorthIt',
        }),
      ],
      { label: 'everyPosition' },
    ),
  ],
});

export const compiled = compileTemplate(orcaHarvestManyPositions);
