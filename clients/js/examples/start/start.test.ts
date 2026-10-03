/**
 * Holds the Getting started and Template lifecycle pages to what they promise in TypeScript.
 *
 * Both pages send each upload instruction in its own version 0 transaction, built by `send` in
 * `connect.ts`, and pass that transaction to the upload plan. Here every one of those transactions
 * is built for each protocol template, several of them over 1 KB, encoded as it goes on the wire,
 * and checked against Solana's 1,232-byte limit. So is every transaction of a resumed upload.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import {
  appendTransactionMessageInstructions,
  blockhash,
  compileTransaction,
  createTransactionMessage,
  generateKeyPairSigner,
  getTransactionEncoder,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  type Instruction,
  type TransactionSigner,
} from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { compileTemplate, TEMPLATE_STATE_UPLOADING, type CompiledTemplate } from '../../src/index.js';
import { buildKitResumeTemplateUploadPlan, buildKitTemplateUploadPlan } from '../../src/kit.js';
import * as protocols from '../protocols/index.js';
import { compiled as sweep } from './sweep.js';

/** The most a legacy or version 0 transaction can hold. */
const TRANSACTION_SIZE_LIMIT = 1_232;

/** The message `emptyMessage` in `connect.ts` builds. The blockhash's value doesn't change the size. */
function emptyMessage(feePayer: TransactionSigner) {
  return pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(feePayer, m),
    (m) =>
      setTransactionMessageLifetimeUsingBlockhash(
        { blockhash: blockhash('11111111111111111111111111111111'), lastValidBlockHeight: 0n },
        m,
      ),
  );
}

/** The wire size of the transaction `send` builds for `instruction`. */
function sentSize(feePayer: TransactionSigner, instruction: Instruction): number {
  const message = appendTransactionMessageInstructions([instruction], emptyMessage(feePayer));
  return getTransactionEncoder().encode(compileTransaction(message)).length;
}

const templates: [string, CompiledTemplate][] = Object.entries(protocols).map(([name, template]) => [
  name,
  compileTemplate(template),
]);

describe('Getting started and Template lifecycle', () => {
  test('several protocol templates are over 1 KB', () => {
    expect(templates.filter(([, compiled]) => compiled.bytes.length > 1_024).length).toBeGreaterThan(1);
  });

  test('every upload transaction fits, for every protocol template', async () => {
    const creator = await generateKeyPairSigner();
    for (const [name, compiled] of templates) {
      const plan = await buildKitTemplateUploadPlan({
        compiled,
        creator: creator.address,
        templateId: 42,
        transactionMessage: emptyMessage(creator),
      });
      if (compiled.bytes.length > 1_024) expect(plan.mode, name).toBe('chunked');
      for (const { kind, offset, instruction } of plan.instructions) {
        expect(sentSize(creator, instruction), `${name}: ${kind} ${offset ?? ''}`).toBeLessThanOrEqual(
          TRANSACTION_SIZE_LIMIT,
        );
      }
    }
  });

  test('every transaction of a resumed upload fits', async () => {
    const creator = await generateKeyPairSigner();
    for (const [name, compiled] of templates) {
      // An upload stopped after its first 100 bytes.
      const writtenLength = 100;
      const payload = new Uint8Array(compiled.bytes.length);
      payload.set(compiled.bytes.subarray(0, writtenLength));
      const plan = await buildKitResumeTemplateUploadPlan({
        compiled,
        account: {
          version: 2,
          state: TEMPLATE_STATE_UPLOADING,
          bump: 255,
          creator: new Uint8Array(32),
          templateId: 43,
          payloadLength: compiled.bytes.length,
          writtenLength,
          payloadHash: compiled.hash,
          payload,
        },
        creator: creator.address,
        templateId: 43,
        transactionMessage: emptyMessage(creator),
      });
      expect(plan.instructions[0]?.offset, name).toBe(writtenLength);
      for (const { kind, offset, instruction } of plan.instructions) {
        expect(sentSize(creator, instruction), `${name}: ${kind} ${offset ?? ''}`).toBeLessThanOrEqual(
          TRANSACTION_SIZE_LIMIT,
        );
      }
    }
  });

  test("Getting started's sweep compiles to the guide's sweep-above-a-reserve bytes", () => {
    const benchmarks = JSON.parse(
      readFileSync(fileURLToPath(new URL('../../../../fixtures/benchmarks.json', import.meta.url)), 'utf8'),
    ) as Record<string, { templateHex: string }>;
    expect(Buffer.from(sweep.bytes).toString('hex')).toBe(benchmarks['sweep-above-a-reserve']!.templateHex);
  });
});
