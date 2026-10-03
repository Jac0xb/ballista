/**
 * Simulate a run before sending it, then read the result: why it would fail, or the events it
 * logged and the value it returned. Simulation needs no signature, so a dApp can run it before
 * asking a wallet to sign.
 *
 * The Errors and events page includes the regions below. `sdk-examples.test.ts` runs them against
 * a stand-in RPC that answers as a validator does, through Kit's own response handling.
 */
// #region simulate
import {
  compileTransaction,
  getBase64EncodedWireTransaction,
  type Rpc,
  type SimulateTransactionApi,
  type TransactionError,
  type TransactionMessage,
  type TransactionMessageWithFeePayer,
} from '@solana/kit';

import { explainRunError, failedProgram, type CompiledTemplate } from '../../src/index.js';

/** Simulates a message as it stands, unsigned, against the cluster's latest blockhash. */
export async function simulate(
  rpc: Rpc<SimulateTransactionApi>,
  message: TransactionMessage & TransactionMessageWithFeePayer,
) {
  const transaction = getBase64EncodedWireTransaction(compileTransaction(message));
  const { value } = await rpc
    .simulateTransaction(transaction, { encoding: 'base64', replaceRecentBlockhash: true, sigVerify: false })
    .send();
  return value;
}

export type Simulation = Awaited<ReturnType<typeof simulate>>;

/** Why a simulated run fails: the step, if Ballista refused it, or else the program that did. */
export function whyItFails(simulation: Simulation, compiled: CompiledTemplate): string | undefined {
  if (!simulation.err) return undefined;
  const logs = simulation.logs ?? [];
  const code = customCode(simulation.err);
  // Explains the code only if the logs show that Ballista failed, not a program it called.
  const explanation = code === undefined ? undefined : explainRunError(code, compiled, { logs });
  return explanation?.message ?? `${failedProgram(logs) ?? 'The runtime'} refused the transaction`;
}

/** A program's custom error code. Kit's RPC returns it as a `bigint`, though its type says `number`. */
function customCode(error: TransactionError): number | bigint | undefined {
  if (typeof error !== 'object' || !('InstructionError' in error)) return undefined;
  const [, instructionError] = error.InstructionError;
  return typeof instructionError === 'object' && 'Custom' in instructionError ? instructionError.Custom : undefined;
}
// #endregion simulate

// #region events
import { BALLISTA_PROGRAM_ADDRESS, decodeRunEvent, parseProgramData, type RunEvent } from '../../src/index.js';

/** The run events and `emit` outputs Ballista logged. */
export function runOutputs(simulation: Simulation): { events: RunEvent[]; emits: Uint8Array[] } {
  const events: RunEvent[] = [];
  const emits: Uint8Array[] = [];
  // A failed transaction keeps the lines logged before it failed, so read them only on success.
  if (simulation.err) return { events, emits };
  // A `Program data:` line does not name its program; parseProgramData follows the invoke stack.
  for (const line of parseProgramData(simulation.logs ?? [])) {
    if (line.program !== BALLISTA_PROGRAM_ADDRESS) continue;
    for (const field of line.fields) {
      // The run event starts with BEV1, and no template's emit tag can start with BEV.
      const event = decodeRunEvent(field);
      if (event) events.push(event);
      else emits.push(field);
    }
  }
  return { events, emits };
}
// #endregion events

// #region return-data
import { getBase64Encoder, getU64Decoder } from '@solana/kit';

import { BALLISTA_ADDRESS } from '../../src/kit.js';

/**
 * The `u64` a run returned. A transaction's return data is its last instruction's, so the run must
 * come last. The data names the program that set it: this proves Ballista set it, not which
 * template ran.
 */
export function returnedU64(simulation: Simulation): bigint {
  const returned = simulation.returnData;
  if (simulation.err || returned?.programId !== BALLISTA_ADDRESS) {
    throw new Error('The run failed, or Ballista did not set the return data');
  }
  return getU64Decoder().decode(getBase64Encoder().encode(returned.data[0]));
}
// #endregion return-data
