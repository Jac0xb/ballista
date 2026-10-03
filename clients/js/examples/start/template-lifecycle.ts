/**
 * Template lifecycle (`docs/guide/template-lifecycle.md`): upload a template too large for one
 * transaction, and resume an upload that stopped partway. It uses Getting started's helpers. Start
 * the local validator Getting started describes, then run:
 *
 *   pnpm --dir clients/js exec tsx examples/start/template-lifecycle.ts
 */
import { compileTemplate } from '../../src/index.js';
import { signedQuoteSettlement } from '../protocols/signed-quote-settlement.js';
import { emptyMessage, fundedSigner, rpc, send } from './connect.js';

// The signed quote is over 1 KB, too large for one transaction, so it uploads in pieces.
const compiled = compileTemplate(signedQuoteSettlement);
const creator = await fundedSigner();

// #region upload
import { buildKitTemplateUploadPlan } from '../../src/kit.js';

const plan = await buildKitTemplateUploadPlan({
  compiled,
  creator: creator.address,
  templateId: 42,
  // Size each instruction to fit the transactions `send` builds.
  transactionMessage: await emptyMessage(creator),
});
for (const { kind, offset, instruction } of plan.instructions) {
  await send(creator, [instruction]);
  console.log(kind, offset ?? ''); // begin, write 0, write 1021, finalize
}
// #endregion upload

// An upload of the same template as ID 43 that stopped after its first write.
const interrupted = await buildKitTemplateUploadPlan({
  compiled,
  creator: creator.address,
  templateId: 43,
  transactionMessage: await emptyMessage(creator),
});
for (const { instruction } of interrupted.instructions.slice(0, 2)) {
  await send(creator, [instruction]);
}

// #region resume
import { fetchEncodedAccount } from '@solana/kit';
import { buildKitResumeTemplateUploadPlan, getTemplateAddress } from '../../src/kit.js';

// Plan only the writes the account is missing, then the finalize.
const [templateAddress] = await getTemplateAddress(creator.address, 43);
const stored = await fetchEncodedAccount(rpc, templateAddress);
if (!stored.exists) throw new Error('No upload to resume');
const resumed = await buildKitResumeTemplateUploadPlan({
  compiled,
  account: stored.data,
  creator: creator.address,
  templateId: 43,
  transactionMessage: await emptyMessage(creator),
});
for (const { instruction } of resumed.instructions) {
  await send(creator, [instruction]);
}
// #endregion resume
