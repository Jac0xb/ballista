/**
 * Decode a failed run's error code, and name the step that raised it.
 *
 * The Errors and events page includes the region below; `sdk-examples.test.ts` checks the values
 * its comments show.
 */
import { compileTemplate } from '@jac0xb/ballista';
import { budgetedPayroll } from '../docs/budgeted-payroll.js';

/** The budgeted payroll from Batch execution, whose last step is the `withinBudget` require. */
const compiled = compileTemplate(budgetedPayroll);

// #region decode
import { decodeBallistaError, explainRunError } from '@jac0xb/ballista';

// The budget `require` failing at program counter 8: the kind in the low 16 bits, the context in
// the high 16.
export const decoded = decodeBallistaError((8 << 16) | 6015);
// { code: 530303, kind: 6015, name: 'RequirementFailed', context: 8, source: 'runtime' }

// `compiled` is the template that ran, compiled: here the budgeted payroll from Batch execution.
export const explained = explainRunError((8 << 16) | 6015, compiled)?.message;
// 'RequirementFailed at steps[2] (withinBudget)'
// #endregion decode
