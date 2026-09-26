/**
 * Templates for the live-protocol examples.
 *
 * Every one compiles in CI and is checked against the on-chain verifier through
 * `fixtures/protocol-examples.json`, so an SDK or wire-format change cannot quietly break them.
 * What CI cannot check is the other side: the protocols' own account layouts and discriminators,
 * which move when they upgrade. Re-derive those from the current IDL before uploading.
 */
export { driftRebalanceExact } from './drift-rebalance-exact.js';
export { driftSettleWhenProfitable } from './drift-settle-when-profitable.js';
export { jitoProfitGuardedTip } from './jito-profit-guarded-tip.js';
export { jupiterDepositExactOutput } from './jupiter-deposit-exact-output.js';
export { jupiterOracleCheckedSwap } from './jupiter-oracle-checked-swap.js';
export { kaminoLiquidateWithProof } from './kamino-liquidate-with-proof.js';
export { kaminoRepaySwapOutput } from './kamino-repay-swap-output.js';
export { marginfiWithdrawAllWithFloor } from './marginfi-withdraw-all-with-floor.js';
export { orcaCompoundFees } from './orca-compound-fees.js';
export { orcaHarvestManyPositions } from './orca-harvest-many-positions.js';
export { pythFreshPriceGate } from './pyth-fresh-price-gate.js';
export { tokenSweepIntoSwap } from './token-sweep-into-swap.js';
