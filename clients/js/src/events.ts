import { encodeBase58 } from './base58.js';

/** The run event's length: `BEV1`, version, rows, invokes reached, executed mask, template. */
export const RUN_EVENT_LENGTH = 47;

/** ASCII `BEV1`, the run event's first four bytes. */
const RUN_EVENT_MAGIC = [0x42, 0x45, 0x56, 0x31] as const;

/** Ballista's run event, which a template with `emitEvent: true` logs after each successful run. */
export interface RunEvent {
  /** The template's bytecode version, `1`. */
  version: number;
  /** The batch rows the run was given, up to 255. A `repeat` loop's passes are not counted. */
  iterations: number;
  /** The invokes the run reached, counting each loop pass. */
  expanded: number;
  /** Bit `n` is set if reached invoke `n` ran, and clear if its `when` skipped it. */
  executed: bigint;
  /** The template account that ran, base58-encoded. */
  templateAddress: string;
}

/**
 * Decodes Ballista's run event from one `Program data:` field: exactly 47 bytes that start with
 * `BEV1`. Returns `undefined` for any other bytes, such as a template's `emit`, whose tag cannot
 * start with `BEV`.
 *
 * It does not know which program logged the bytes, and any program can log these. Take the field
 * from a line `parseProgramData` attributes to Ballista.
 */
export function decodeRunEvent(data: Uint8Array): RunEvent | undefined {
  if (data.length !== RUN_EVENT_LENGTH || !RUN_EVENT_MAGIC.every((byte, index) => data[index] === byte)) {
    return undefined;
  }
  return {
    version: data[4]!,
    iterations: data[5]!,
    expanded: data[6]!,
    executed: new DataView(data.buffer, data.byteOffset, data.byteLength).getBigUint64(7, true),
    templateAddress: encodeBase58(data.subarray(15, RUN_EVENT_LENGTH)),
  };
}

/** One `Program data:` log line and the program that logged it. */
export interface ProgramData {
  /** The program that logged the line: the innermost invocation open around it. */
  program: string;
  /** That invocation's stack height: 1 for a transaction's own instruction, 2 for a program it calls. */
  height: number;
  /** The invocation's number, counting the transaction's invocations from 0 in log order. */
  invocation: number;
  /** The line's fields, base64-decoded. `sol_log_data` logs each slice as one field; Ballista logs one. */
  fields: Uint8Array[];
}

const INVOKE = /^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) invoke \[(\d+)\]$/;
const END = /^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) (?:success$|failed: )/;
const DATA = 'Program data: ';

/**
 * Every `Program data:` line in a transaction's logs, with the program that logged it. A line does
 * not name its program, so this follows the `invoke`, `success` and `failed` lines around it.
 * Lines with the same `invocation` came from the same call: a template's `emit` lines and the run
 * event that names the template, say.
 *
 * Throws a `TypeError` if the logs do not nest: a data line outside every invocation, a height
 * that skips a level, or an invocation that ends out of order. Logs cut off at Solana's log limit
 * still parse, up to the `Log truncated` line.
 */
export function parseProgramData(logs: readonly string[]): ProgramData[] {
  const stack: { program: string; invocation: number }[] = [];
  const lines: ProgramData[] = [];
  let invocations = 0;
  for (const line of logs) {
    if (line.startsWith(DATA)) {
      const open = stack.at(-1);
      if (!open) throw new TypeError(`${JSON.stringify(line)} is outside every invocation`);
      const fields = line.slice(DATA.length).split(' ').map(decodeBase64);
      lines.push({ program: open.program, height: stack.length, invocation: open.invocation, fields });
      continue;
    }
    const invoke = INVOKE.exec(line);
    if (invoke) {
      if (Number(invoke[2]) !== stack.length + 1) throw new TypeError(`${JSON.stringify(line)} skips a level`);
      stack.push({ program: invoke[1]!, invocation: invocations });
      invocations += 1;
      continue;
    }
    const end = END.exec(line);
    if (end) {
      if (stack.at(-1)?.program !== end[1]) {
        throw new TypeError(`${JSON.stringify(line)} ends an invocation that is not the innermost open one`);
      }
      stack.pop();
    }
  }
  return lines;
}

function decodeBase64(field: string): Uint8Array {
  let binary: string;
  try {
    binary = atob(field);
  } catch {
    throw new TypeError(`${JSON.stringify(field)} is not base64`);
  }
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
