/**
 * Getting started, steps 3 to 5 (`docs/guide/getting-started.md`): upload the sweep template, run
 * it, and read a failure. Start the local validator the page describes, then run:
 *
 *   pnpm --dir clients/js exec tsx examples/start/getting-started.ts
 *
 * The page shows `connect.ts`, `sweep.ts` and the regions below, in that order, as one file.
 */
import { emptyMessage, fundedSigner, rpc, send } from './connect.js';
import { compiled } from './sweep.js';

// #region upload
import { buildKitTemplateUploadPlan } from '@jac0xb/ballista/kit';

// The creator uploads the template and pays the rent for its account.
const creator = await fundedSigner();
const upload = await buildKitTemplateUploadPlan({
  compiled,
  creator: creator.address,
  templateId: 7,
  // Size each instruction to fit the transactions `send` builds.
  transactionMessage: await emptyMessage(creator),
});
for (const { instruction } of upload.instructions) {
  await send(creator, [instruction]);
}
console.log('uploaded to', upload.templateAddress);
// #endregion upload

// #region run
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

// The vault is the account the template sweeps. It signs the run and pays the fee.
const vault = await fundedSigner();
// Any account can receive the lamports; here, another new wallet.
const destination = (await fundedSigner()).address;

// The caller picks the reserve; the template works out the amount.
function sweepInstruction(reserve: bigint) {
  return buildKitRunInstruction({
    compiled,
    templateAddress: upload.templateAddress,
    inputs: { reserve },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      vault: { address: vault.address },
      destination: { address: destination },
    },
  });
}

await send(vault, [sweepInstruction(2_000_000n)]);
const { value: left } = await rpc.getBalance(vault.address).send();
console.log('vault keeps', left); // 2000000n
// #endregion run

// #region failure
import { isSolanaError, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM } from '@solana/kit';
import { explainRunError } from '@jac0xb/ballista';

// The vault now holds less than 5,000,000 lamports, so the check fails.
try {
  await send(vault, [sweepInstruction(5_000_000n)]);
} catch (error) {
  const cause = error instanceof Error ? error.cause : undefined;
  if (!isSolanaError(cause, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM)) throw error;
  const { code } = cause.context;
  console.log(code, explainRunError(code, compiled)?.message);
  // 202623 RequirementFailed at steps[1] (aboveReserve)
}
// #endregion failure
