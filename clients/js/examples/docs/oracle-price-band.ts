// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with the oracle program (the owner of the
// price account), the price's offset in its layout, and the protocol call. A real template also
// checks which feed the account holds and when its price was published.
const ORACLE_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const PRICE_OFFSET = 8;
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const INSTRUCTION_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const INSTRUCTION_ARGUMENT = 10_000n;

const price = expression.accountData(account.fixed('oracle'), PRICE_OFFSET, 'i64');

/** Call the protocol only while the oracle's price lies within `[minimumPrice, maximumPrice]`. */
export const oraclePriceBand = defineTemplate({
  inputs: { minimumPrice: { type: 'i64' }, maximumPrice: { type: 'i64' } },
  accounts: {
    oracle: { owner: ORACLE_PROGRAM, minDataLength: 128 },
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    payer: { signer: true, writable: true },
    pool: { writable: true },
  },
  steps: [
    step.require(
      expression.and(
        expression.greaterThanOrEqual(price, expression.input('minimumPrice')),
        expression.lessThanOrEqual(price, expression.input('maximumPrice')),
      ),
    ),
    step.invoke({
      program: account.fixed('protocolProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.literal(INSTRUCTION_DISCRIMINATOR), data.encode('u64', expression.u64(INSTRUCTION_ARGUMENT))],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const PROTOCOL_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** The band is in the oracle's own units. */
export function runOraclePriceBand(run: {
  templateAddress: Address;
  oracle: Address;
  payer: Address;
  pool: Address;
  minimumPrice: bigint;
  maximumPrice: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(oraclePriceBand),
    templateAddress: run.templateAddress,
    inputs: { minimumPrice: run.minimumPrice, maximumPrice: run.maximumPrice },
    accounts: {
      oracle: { address: run.oracle },
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
    },
  });
}
// #endregion run
