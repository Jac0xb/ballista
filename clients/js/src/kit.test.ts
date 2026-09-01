import {
  address,
  appendTransactionMessageInstruction,
  blockhash,
  createTransactionMessage,
  getAddressDecoder,
  getTransactionMessageComputeUnitLimit,
  getTransactionMessageLoadedAccountsDataSizeLimit,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
} from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { compileTemplate } from './compiler.js';
import {
  buildKitRunInstruction,
  buildKitTemplateUploadPlan,
  createComputeUnitProvider,
  getComputeUnitLimitWithMargin,
  getComputeUnitsConsumed,
  getTemplateAddress,
  measureTransactionMessage,
  type ComputeUnitRpc,
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

  test('simulates and applies a buffered compute unit limit', async () => {
    const rpc = {
      simulateTransaction: () => ({
        send: async () => ({
          value: { err: null, unitsConsumed: 100_000n, loadedAccountsDataSize: 4_096 },
        }),
      }),
    } as unknown as ComputeUnitRpc;
    const payer = byteAddress(4);
    const message = pipe(
      createTransactionMessage({ version: 1 }),
      (value) => setTransactionMessageFeePayer(payer, value),
      (value) =>
        setTransactionMessageLifetimeUsingBlockhash(
          { blockhash: blockhash('11111111111111111111111111111111'), lastValidBlockHeight: 1n },
          value,
        ),
    );

    const provider = createComputeUnitProvider({ rpc });
    const result = await provider.estimateAndSet(message);
    expect(result.estimate).toMatchObject({
      simulatedComputeUnits: 100_000,
      computeUnitLimit: 110_000,
      marginComputeUnits: 10_000,
      marginBps: 1_000,
      capped: false,
    });
    expect(getTransactionMessageComputeUnitLimit(result.transactionMessage)).toBe(110_000);
    expect(getTransactionMessageLoadedAccountsDataSizeLimit(result.transactionMessage)).toBe(4_096);
  });

  test('caps margins and reads authoritative confirmed CU metadata', () => {
    expect(getComputeUnitLimitWithMargin(1_390_000)).toMatchObject({
      computeUnitLimit: 1_400_000,
      marginComputeUnits: 10_000,
      capped: true,
    });
    expect(getComputeUnitsConsumed({ meta: { computeUnitsConsumed: 16_400n } })).toBe(16_400);
    expect(getComputeUnitsConsumed({ meta: null })).toBeUndefined();
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
