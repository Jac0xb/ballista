import {
  address,
  appendTransactionMessageInstruction,
  blockhash,
  createTransactionMessage,
  getAddressDecoder,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
} from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { compileTemplate } from './compiler.js';
import {
  buildKitRunInstruction,
  buildKitTemplateUploadPlan,
  getTemplateAddress,
  measureTransactionMessage,
} from './kit.js';
import { account, defineTemplate, expression, step } from './schema.js';
import { systemTransfer } from './helpers.js';

const decoder = getAddressDecoder();
const byteAddress = (byte: number) => decoder.decode(new Uint8Array(32).fill(byte));
const systemAddress = address('11111111111111111111111111111111');

describe('Solana Kit adapter', () => {
  test('derives the template-v2 PDA identically to Rust', async () => {
    await expect(getTemplateAddress(systemAddress, 7)).resolves.toEqual([
      address('HJhruxADGAkstBRu5Kj77XG6tGmkVHUQZ5JmgNd6xm2f'),
      255,
    ]);
  });

  test('keeps a 30-recipient Ballista-only v1 message below 4096 bytes', () => {
    const compiled = batchTransferTemplate();
    const source = byteAddress(2);
    const instruction = buildKitRunInstruction({
      compiled,
      templateAddress: byteAddress(8),
      inputs: { amount: 10_000n },
      accounts: {
        systemProgram: { address: systemAddress },
        source: { address: source },
      },
      batchRows: Array.from({ length: 30 }, (_, index) => ({
        recipient: { address: byteAddress(index + 20) },
      })),
    });
    const message = pipe(
      createTransactionMessage({ version: 1 }),
      (value) => setTransactionMessageFeePayer(source, value),
      (value) =>
        setTransactionMessageLifetimeUsingBlockhash(
          { blockhash: blockhash('11111111111111111111111111111111'), lastValidBlockHeight: 1n },
          value,
        ),
      (value) => appendTransactionMessageInstruction(instruction, value),
    );
    const measurement = measureTransactionMessage(message);
    expect(instruction.accounts).toHaveLength(33);
    expect(measurement.limit).toBe(4_096);
    expect(measurement.fits).toBe(true);
    expect(measurement.size).toBe(1_240);
  });

  test('uses exact v1 measurement when selecting an upload plan', async () => {
    const compiled = batchTransferTemplate();
    const creator = byteAddress(3);
    const baseMessage = pipe(
      createTransactionMessage({ version: 1 }),
      (value) => setTransactionMessageFeePayer(creator, value),
      (value) =>
        setTransactionMessageLifetimeUsingBlockhash(
          { blockhash: blockhash('11111111111111111111111111111111'), lastValidBlockHeight: 1n },
          value,
        ),
    );
    const plan = await buildKitTemplateUploadPlan({
      compiled,
      creator,
      templateId: 4,
      transactionMessage: baseMessage,
    });
    expect(plan.mode).toBe('oneShot');
    const measured = measureTransactionMessage(
      appendTransactionMessageInstruction(plan.instructions[0]!.instruction, baseMessage),
    );
    expect(measured.fits).toBe(true);
  });
});

function batchTransferTemplate() {
  return compileTemplate(
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: new Uint8Array(32) },
        source: { signer: true, writable: true },
      },
      batch: { maxIterations: 30, row: { recipient: { writable: true } } },
      steps: [
        step.forEach([
          systemTransfer({
            systemProgram: account.fixed('systemProgram'),
            from: account.fixed('source'),
            to: account.iteration('recipient'),
            lamports: expression.input('amount'),
          }),
        ]),
      ],
    }),
  );
}
