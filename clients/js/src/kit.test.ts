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
  findFreeTemplateId,
  getComputeUnitLimitWithMargin,
  getComputeUnitsConsumed,
  getLoadedAccountsDataSizeLimitWithHeadroom,
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
  test('derives the template PDA identically to Rust', async () => {
    await expect(getTemplateAddress(systemAddress, 7)).resolves.toEqual([
      address('CFbBQL1sPP69VFwLutH11V4cpaCUSyvaCDfAeetHEtJW'),
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
      simulatedLoadedAccountsDataSize: 4_096,
      loadedAccountsDataSizeLimit: 32_768,
    });
    expect(getTransactionMessageComputeUnitLimit(result.transactionMessage)).toBe(110_000);
    expect(getTransactionMessageLoadedAccountsDataSizeLimit(result.transactionMessage)).toBe(32_768);
  });

  test('probes template addresses in batches until one is free', async () => {
    const creator = byteAddress(5);
    const [taken0] = await getTemplateAddress(creator, 0);
    const [taken1] = await getTemplateAddress(creator, 1);
    const [taken2] = await getTemplateAddress(creator, 2);
    const occupied = new Set<string>([taken0, taken1, taken2]);
    const calls: number[] = [];
    const rpc = {
      getMultipleAccounts: (addresses: readonly string[]) => ({
        send: async () => {
          calls.push(addresses.length);
          return { value: addresses.map((candidate) => (occupied.has(candidate) ? { lamports: 1n } : null)) };
        },
      }),
    };

    const free = await findFreeTemplateId({ rpc, creator, batchSize: 2 });
    expect(free.templateId).toBe(3);
    expect(free.templateAddress).toBe((await getTemplateAddress(creator, 3))[0]);
    expect(calls).toEqual([2, 2]);

    const later = await findFreeTemplateId({ rpc, creator, start: 65_534 });
    expect(later.templateId).toBe(65_534);

    const everythingTaken = {
      getMultipleAccounts: (addresses: readonly string[]) => ({
        send: async () => ({ value: addresses.map(() => ({ lamports: 1n })) }),
      }),
    };
    await expect(findFreeTemplateId({ rpc: everythingTaken, creator, start: 65_530 })).rejects.toThrow('taken');
    await expect(findFreeTemplateId({ rpc, creator, batchSize: 0 })).rejects.toThrow('batch size');
  });

  test('caps margins and reads authoritative confirmed CU metadata', () => {
    expect(getComputeUnitLimitWithMargin(1_390_000)).toMatchObject({
      computeUnitLimit: 1_400_000,
      marginComputeUnits: 10_000,
      capped: true,
    });
    expect(getComputeUnitsConsumed({ meta: { computeUnitsConsumed: 16_400n } })).toBe(16_400);
    expect(getComputeUnitsConsumed({ meta: null })).toBeUndefined();
    expect(getLoadedAccountsDataSizeLimitWithHeadroom(32_769)).toBe(65_536);
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
