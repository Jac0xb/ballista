import { defineTemplate, expression, step } from '../../src/index.js';

/**
 * The inputs from "Named inputs" on the expressions page, which shows them alone. The step is
 * there only so the template compiles.
 */
export const namedInputs = defineTemplate({
  inputs: {
    amount: { type: 'u64' },
    deadline: { type: 'i64' },
    enabled: { type: 'bool' },
    routeData: { type: 'bytes', maxLength: 512 },
  },
  accounts: {},
  steps: [step.require(expression.input('enabled'))],
});

// #region encode
import { compileTemplate, encodeRunInputs } from '../../src/index.js';

/** The run data for inputs `amount: u64`, `deadline: i64`, `enabled: bool` and `routeData: bytes`. */
export function encodeNamedInputs(routeData: Uint8Array) {
  return encodeRunInputs(compileTemplate(namedInputs), {
    amount: 25_000n,
    deadline: 1_800_000_000n,
    enabled: true,
    routeData, // a u16 length, then the bytes
  });
}
// #endregion encode
