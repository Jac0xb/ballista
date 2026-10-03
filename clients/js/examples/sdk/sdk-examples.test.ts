/**
 * Runs the SDK examples the docs include, offline.
 *
 * The RPC is Kit's own, built on a stand-in transport that answers as a validator does. Kit's
 * response handling then runs as it would against a cluster: it turns every integer it was not told
 * to keep as a number into a `bigint`, which is how a simulation's custom error code arrives.
 */
import {
  appendTransactionMessageInstruction,
  assertIsFullySignedTransaction,
  blockhash,
  createSolanaRpcFromTransport,
  createTransactionMessage,
  generateKeyPairSigner,
  getAddressDecoder,
  getAddressEncoder,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type RpcTransport,
} from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { BALLISTA_PROGRAM_ADDRESS, compileTemplate } from '@jac0xb/ballista';
import { BALLISTA_ADDRESS, SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction, measureTransactionMessage } from '@jac0xb/ballista/kit';
import { budgetedPayroll, runBudgetedPayroll } from '../docs/budgeted-payroll.js';
import { sweepAboveAReserve } from '../docs/sweep-above-a-reserve.js';
import { compiled as jupiterDeposit } from '../protocols/jupiter-deposit-exact-output.js';
import { describeFailure } from '../protocols/run-jupiter-deposit.js';
import { decoded, explained } from './decode-errors.js';
import { runWithLookupTables } from './lookup-tables.js';
import { relayedSweep } from './second-signer.js';
import { returnedU64, runOutputs, simulate, whyItFails } from './simulate-run.js';
import { reportedSweep } from './report-outputs.js';

const JUPITER = 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4';
const SYSTEM = '11111111111111111111111111111111';
const latestBlockhash = { blockhash: blockhash('11111111111111111111111111111111'), lastValidBlockHeight: 1n };
const key = (index: number): Address => getAddressDecoder().decode(new Uint8Array(32).fill(index));
const base64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64');
const payroll = compileTemplate(budgetedPayroll);

/** A Kit RPC whose transport answers each method with `answers[method]`, and records the requests. */
function standInRpc(answers: Record<string, unknown>) {
  const requests: { method: string; params: unknown[] }[] = [];
  const transport = (async ({ payload }: { payload: unknown }) => {
    const { id, method, params } = payload as { id: string; method: string; params: unknown[] };
    requests.push({ method, params });
    if (!(method in answers)) throw new Error(`No stand-in answer for ${method}`);
    return { jsonrpc: '2.0', id, result: answers[method] };
  }) as RpcTransport;
  return { rpc: createSolanaRpcFromTransport(transport), requests };
}

/** A simulation result as the validator sends it, before Kit's response handling. */
function simulationAnswer(value: { err: unknown; logs: string[]; returnData?: unknown }) {
  return {
    context: { slot: 1 },
    value: {
      accounts: null,
      fee: 5_000,
      innerInstructions: null,
      loadedAccountsDataSize: 4_096,
      loadedAddresses: null,
      postBalances: null,
      postTokenBalances: null,
      preBalances: null,
      preTokenBalances: null,
      replacementBlockhash: { blockhash: latestBlockhash.blockhash, lastValidBlockHeight: 2 },
      returnData: null,
      unitsConsumed: 9_000,
      ...value,
    },
  };
}

/** A three-row payroll run, paid by its treasury, in a version 0 message. */
function payrollMessage(treasury: Address) {
  const run = runBudgetedPayroll({
    templateAddress: key(200),
    treasury,
    recipients: [key(1), key(2), key(3)],
    amount: 50_000n,
    budget: 100_000n,
  });
  return pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayer(treasury, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, m),
    (m) => appendTransactionMessageInstruction(run, m),
  );
}

describe('a template that reports back', () => {
  test('compiles with a run event, an emit and return data', () => {
    const compiled = compileTemplate(reportedSweep);
    expect(compiled.bytes.length).toBeGreaterThan(0);
  });
});

describe('decoding an error code', () => {
  test('gives the values the page shows', () => {
    expect(decoded).toEqual({ code: 464_767, kind: 6015, name: 'RequirementFailed', context: 7, source: 'runtime' });
    expect(explained).toBe('RequirementFailed at steps[2] (withinBudget)');
  });
});

describe('simulating a run', () => {
  const treasury = key(100);
  const withinBudget = payroll.sourceMap.find((entry) => entry.label === 'withinBudget')!.pc;
  const overBudget = (withinBudget << 16) | 6015;

  test('names the step when Ballista refuses it, from the bigint Kit returns', async () => {
    const { rpc, requests } = standInRpc({
      simulateTransaction: simulationAnswer({
        err: { InstructionError: [0, { Custom: overBudget }] },
        logs: [
          `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
          ...Array.from({ length: 3 }, () => [`Program ${SYSTEM} invoke [2]`, `Program ${SYSTEM} success`]).flat(),
          `Program log: 0x${withinBudget.toString(16)}, 0x28, 0xff, 0xb, 0xff`,
          `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 8410 of 200000 compute units`,
          `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x${overBudget.toString(16)}`,
        ],
      }),
    });
    const simulation = await simulate(rpc, payrollMessage(treasury));

    // Kit's type says `number`; what arrives is a bigint.
    const [, instructionError] = (simulation.err as { InstructionError: [unknown, { Custom: unknown }] }).InstructionError;
    expect(typeof instructionError.Custom).toBe('bigint');
    expect(whyItFails(simulation, payroll)).toBe('RequirementFailed at steps[2] (withinBudget)');
    expect(requests).toEqual([
      {
        method: 'simulateTransaction',
        params: [expect.any(String), expect.objectContaining({ encoding: 'base64', replaceRecentBlockhash: true, sigVerify: false })],
      },
    ]);
  });

  test('names the program when a program the run called refuses it', async () => {
    // Jupiter's slippage error is 6001, which is also Ballista's InvalidTemplateAccount.
    const { rpc } = standInRpc({
      simulateTransaction: simulationAnswer({
        err: { InstructionError: [0, { Custom: 6001 }] },
        logs: [
          `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
          `Program ${JUPITER} invoke [2]`,
          'Program log: Instruction: Route',
          'Program log: AnchorError occurred. Error Code: SlippageToleranceExceeded. Error Number: 6001. Error Message: Slippage tolerance exceeded.',
          `Program ${JUPITER} consumed 31337 of 180000 compute units`,
          `Program ${JUPITER} failed: custom program error: 0x1771`,
          `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 52000 of 200000 compute units`,
          `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x1771`,
        ],
      }),
    });
    const simulation = await simulate(rpc, payrollMessage(treasury));
    expect(whyItFails(simulation, jupiterDeposit)).toBe(`${JUPITER} refused the transaction`);
  });

  test('reads the run event, the emit outputs and the return data of a run that succeeds', async () => {
    const event = new Uint8Array(47);
    event.set(new TextEncoder().encode('BEV1'));
    event.set([1, 3, 3], 4);
    event[7] = 0b111;
    event.set(getAddressEncoder().encode(key(200)), 15);
    const paid = new Uint8Array(12);
    paid.set(new TextEncoder().encode('PAID'));
    new DataView(paid.buffer).setBigUint64(4, 150_000n, true);
    const total = new Uint8Array(8);
    new DataView(total.buffer).setBigUint64(0, 150_000n, true);
    const { rpc } = standInRpc({
      simulateTransaction: simulationAnswer({
        err: null,
        logs: [
          `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
          ...Array.from({ length: 3 }, () => [`Program ${SYSTEM} invoke [2]`, `Program ${SYSTEM} success`]).flat(),
          `Program data: ${base64(paid)}`,
          `Program data: ${base64(event)}`,
          `Program return: ${BALLISTA_PROGRAM_ADDRESS} ${base64(total)}`,
          `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 9000 of 200000 compute units`,
          `Program ${BALLISTA_PROGRAM_ADDRESS} success`,
        ],
        returnData: { programId: BALLISTA_PROGRAM_ADDRESS, data: [base64(total), 'base64'] },
      }),
    });
    const simulation = await simulate(rpc, payrollMessage(treasury));

    expect(whyItFails(simulation, payroll)).toBeUndefined();
    expect(runOutputs(simulation)).toEqual({
      events: [{ version: 1, iterations: 3, expanded: 3, executed: 0b111n, templateAddress: key(200) }],
      emits: [paid],
    });
    expect(returnedU64(simulation)).toBe(150_000n);
  });

  test('trusts no output of a run that fails, and no return data another program set', async () => {
    const failed = await simulate(
      standInRpc({
        simulateTransaction: simulationAnswer({
          err: { InstructionError: [0, { Custom: overBudget }] },
          logs: [
            `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
            'Program data: UEFJRA==',
            `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x${overBudget.toString(16)}`,
          ],
        }),
      }).rpc,
      payrollMessage(treasury),
    );
    expect(runOutputs(failed)).toEqual({ events: [], emits: [] });
    expect(() => returnedU64(failed)).toThrow(/Ballista did not set/);

    const otherProgram = await simulate(
      standInRpc({
        simulateTransaction: simulationAnswer({
          err: null,
          logs: [`Program ${JUPITER} invoke [1]`, `Program ${JUPITER} success`],
          returnData: { programId: JUPITER, data: ['AAAAAAAAAAA=', 'base64'] },
        }),
      }).rpc,
      payrollMessage(treasury),
    );
    expect(() => returnedU64(otherProgram)).toThrow(/Ballista did not set/);
  });
});

describe('a second signer', () => {
  test('signs a run whose fee payer is someone else', async () => {
    const [relayer, vault] = await Promise.all([generateKeyPairSigner(), generateKeyPairSigner()]);
    const transaction = await relayedSweep({
      compiled: compileTemplate(sweepAboveAReserve),
      templateAddress: key(200),
      relayer,
      vault,
      destination: key(3),
      reserve: 2_000_000n,
      latestBlockhash,
    });
    assertIsFullySignedTransaction(transaction);
    expect(Object.keys(transaction.signatures).sort()).toEqual([relayer.address, vault.address].sort());
  });

  test('is missing without the signer on its binding', async () => {
    const [relayer, vault] = await Promise.all([generateKeyPairSigner(), generateKeyPairSigner()]);
    const run = buildKitRunInstruction({
      compiled: compileTemplate(sweepAboveAReserve),
      templateAddress: key(200),
      inputs: { reserve: 2_000_000n },
      accounts: {
        systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
        vault: { address: vault.address },
        destination: { address: key(3) },
      },
    });
    const message = pipe(
      createTransactionMessage({ version: 0 }),
      (m) => setTransactionMessageFeePayerSigner(relayer, m),
      (m) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, m),
      (m) => appendTransactionMessageInstruction(run, m),
    );
    await expect(signTransactionMessageWithSigners(message)).rejects.toThrow(/missing signatures/i);
  });
});

describe('lookup tables', () => {
  test('fit a 30-row payroll that a version 0 transaction cannot hold', async () => {
    const treasury = await generateKeyPairSigner();
    const recipients = Array.from({ length: 30 }, (_, index) => key(index + 1));
    const run = runBudgetedPayroll({
      templateAddress: key(200),
      treasury: treasury.address,
      recipients,
      amount: 50_000n,
      budget: 1_500_000n,
    });
    const plain = pipe(
      createTransactionMessage({ version: 0 }),
      (m) => setTransactionMessageFeePayerSigner(treasury, m),
      (m) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, m),
      (m) => appendTransactionMessageInstruction(run, m),
    );
    expect(measureTransactionMessage(plain).fits).toBe(false);

    const table = key(201);
    const tableAddresses = [key(200), SYSTEM_PROGRAM_ADDRESS, ...recipients];
    const { rpc, requests } = standInRpc({
      getMultipleAccounts: {
        context: { slot: 1 },
        value: [
          {
            data: {
              parsed: {
                info: {
                  addresses: tableAddresses,
                  authority: treasury.address,
                  deactivationSlot: '18446744073709551615',
                  lastExtendedSlot: '1',
                  lastExtendedSlotStartIndex: 0,
                },
                type: 'lookupTable',
              },
              program: 'address-lookup-table',
              space: 56 + 32 * tableAddresses.length,
            },
            executable: false,
            lamports: 10_000_000,
            owner: 'AddressLookupTab1e1111111111111111111111111',
            rentEpoch: 0,
            space: 56 + 32 * tableAddresses.length,
          },
        ],
      },
    });
    const compressed = await runWithLookupTables({ rpc, feePayer: treasury, latestBlockhash, run, tables: [table] });

    expect(requests.map((request) => request.method)).toEqual(['getMultipleAccounts']);
    const measured = measureTransactionMessage(compressed);
    expect(measured.fits).toBe(true);
    // The template account, the System Program and the 30 recipients come from the table; the
    // treasury signs, so it stays in the message.
    const looked = (compressed.instructions[0]!.accounts ?? []).filter((meta) => 'lookupTableAddress' in meta);
    expect(looked).toHaveLength(32);
    expect(compressed.instructions[0]!.programAddress).toBe(BALLISTA_ADDRESS);
    assertIsFullySignedTransaction(await signTransactionMessageWithSigners(compressed));
  });
});

describe('the Jupiter deposit runner', () => {
  const floor = jupiterDeposit.sourceMap.find((entry) => entry.label === 'swapMetItsFloor')!.pc;
  const jupiterRefused = [
    `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
    `Program ${JUPITER} invoke [2]`,
    `Program ${JUPITER} failed: custom program error: 0x1771`,
    `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x1771`,
  ];

  test("blames Jupiter for Jupiter's 6001, which is also a Ballista code", () => {
    expect(describeFailure(6001, jupiterRefused)).toBe('code 6001 came from Jupiter');
    expect(describeFailure(6001n, jupiterRefused)).toBe('code 6001 came from Jupiter');
  });

  test('names the step when Ballista refused the route', () => {
    const code = (floor << 16) | 6015;
    const logs = [
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x${code.toString(16)}`,
    ];
    expect(describeFailure(code, logs)).toMatch(/^RequirementFailed at .* \(swapMetItsFloor\)$/);
    expect(describeFailure(code, [])).toBe(`code ${code}; the logs name no program that failed`);
  });
});

