/**
 * Test-only templates for Ballista's runtime features: count loops beside row loops, `EMIT`,
 * `SET_RETURN_DATA`, and a nested Ballista run whose return data the outer run reads.
 *
 * They are not public examples. `src/protocol-examples.test.ts` compiles them into
 * `fixtures/protocol-scenarios.json`, and `tests/protocols/tests/runtime_scenarios.rs` runs them
 * against the real mainnet programs; `tests/protocols/findings/runtime-scenarios.md` says what
 * each one proves.
 */
export { nestedSplitSellPayout } from './nested-split-sell.js';
export { ballistaRelay, returnClaim } from './relay.js';
export { splitSellInner, splitSellPayout } from './split-sell.js';
