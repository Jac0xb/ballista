// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with the lending program's address, its
// liquidate instruction data, and the offset of the health value in its position account.
const LENDING_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const LIQUIDATE_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const LIQUIDATE_ARGUMENT = 10_000n;
const HEALTH_OFFSET = 8;

/** Liquidate only when the position's health value is below `threshold`. */
export const liquidateOnlyWhenUnhealthy = defineTemplate({
  inputs: { threshold: { type: 'u64' } },
  accounts: {
    lendingProgram: { executable: true, address: LENDING_PROGRAM },
    position: { owner: LENDING_PROGRAM, minDataLength: 128 },
    liquidator: { signer: true, writable: true },
    vault: { writable: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('lendingProgram'),
      accounts: [
        { account: account.fixed('liquidator'), signer: true, writable: true },
        { account: account.fixed('vault'), signer: false, writable: true },
      ],
      data: [data.literal(LIQUIDATE_DISCRIMINATOR), data.encode('u64', expression.u64(LIQUIDATE_ARGUMENT))],
      when: expression.lessThan(
        expression.accountData(account.fixed('position'), HEALTH_OFFSET, 'u64'),
        expression.input('threshold'),
      ),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const LENDING_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

export function runLiquidateOnlyWhenUnhealthy(run: {
  templateAddress: Address;
  position: Address;
  liquidator: Address;
  vault: Address;
  threshold: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(liquidateOnlyWhenUnhealthy),
    templateAddress: run.templateAddress,
    inputs: { threshold: run.threshold },
    accounts: {
      lendingProgram: { address: LENDING_PROGRAM_ADDRESS },
      position: { address: run.position },
      liquidator: { address: run.liquidator },
      vault: { address: run.vault },
    },
  });
}
// #endregion run
