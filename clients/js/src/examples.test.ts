import { describe, expect, test } from 'vitest';

import { compiledEnsureUsdcAta } from '../examples/ensure-usdc-ata.js';
import { buildTransferRun, describeFailure } from '../examples/run-transfer.js';
import { compiledTransfer, uploadPlan } from '../examples/transfer.js';

describe('shipped examples', () => {
  test('the transfer example compiles to the shared fixture', () => {
    expect(compiledTransfer.stats).toMatchObject({ instructions: 2, cpis: 1 });
    expect(uploadPlan.mode).toBe('oneShot');
  });

  test('the run example binds accounts and explains failures', async () => {
    const instruction = await buildTransferRun(55_000n);
    expect(instruction.accounts).toHaveLength(4);
    expect(Array.from(instruction.data ?? [])).toEqual([5, 0xd8, 0xd6, 0, 0, 0, 0, 0, 0]);
    expect(describeFailure((1 << 16) | 6015)).toBe('RequirementFailed at steps[0] (paySender)');
    expect(describeFailure(1)).toBe('code 1 came from an invoked program');
  });

  test('the devnet ATA example compiles under the pin lints', () => {
    expect(compiledEnsureUsdcAta.stats).toMatchObject({ instructions: 2, cpis: 1, maxExpandedCpis: 1 });
  });
});
