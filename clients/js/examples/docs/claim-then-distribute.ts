// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with the rewards program's address and
// its claim instruction data.
const REWARDS_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CLAIM_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const CLAIM_ARGUMENT = 10_000n;

/** Claim once, then pay `amountPerRecipient` tokens to each row's token account. */
export const claimThenDistribute = defineTemplate({
  inputs: { amountPerRecipient: { type: 'u64' } },
  accounts: {
    rewardsProgram: { executable: true, address: REWARDS_PROGRAM },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    claimer: { signer: true, writable: true },
    pool: { writable: true },
    treasuryTokens: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    authority: { signer: true },
  },
  batch: {
    maxIterations: 16,
    minIterations: 1,
    row: { recipientTokens: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
  },
  steps: [
    step.invoke({
      program: account.fixed('rewardsProgram'),
      accounts: [
        { account: account.fixed('claimer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.literal(CLAIM_DISCRIMINATOR), data.encode('u64', expression.u64(CLAIM_ARGUMENT))],
    }),
    step.forEach([
      tokenTransfer({
        tokenProgram: account.fixed('tokenProgram'),
        source: account.fixed('treasuryTokens'),
        destination: account.iteration('recipientTokens'),
        authority: account.fixed('authority'),
        amount: expression.input('amountPerRecipient'),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const REWARDS_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in
const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

export function runClaimThenDistribute(run: {
  templateAddress: Address;
  claimer: Address;
  pool: Address;
  treasuryTokens: Address;
  authority: Address;
  recipientTokens: readonly Address[];
  amountPerRecipient: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(claimThenDistribute),
    templateAddress: run.templateAddress,
    inputs: { amountPerRecipient: run.amountPerRecipient },
    accounts: {
      rewardsProgram: { address: REWARDS_PROGRAM_ADDRESS },
      tokenProgram: { address: TOKEN_PROGRAM },
      claimer: { address: run.claimer },
      pool: { address: run.pool },
      treasuryTokens: { address: run.treasuryTokens },
      authority: { address: run.authority },
    },
    batchRows: run.recipientTokens.map((recipient) => ({ recipientTokens: { address: recipient } })),
  });
}
// #endregion run
