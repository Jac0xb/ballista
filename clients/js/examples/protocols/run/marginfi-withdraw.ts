// #region run
import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../marginfi-withdraw-all-with-floor.js';
import { MARGINFI_V2 } from '../shared.js';
import { marginfiHealthAccounts, type MarginfiBalance } from './marginfi.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildMarginfiWithdrawRun(input: {
  templateAddress: Address;
  marginfiGroup: Address;
  marginfiAccount: Address;
  authority: Address;
  bank: Address;
  bankLiquidityVault: Address;
  bankLiquidityVaultAuthority: Address;
  /** Where marginfi pays the withdrawal, and the treasury it is swept to: both the authority's own. */
  destinationAta: Address;
  treasuryAta: Address;
  /** Every balance the marginfi account still holds after this one is emptied. */
  remainingBalances: readonly MarginfiBalance[];
  minimumWithdrawn: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { minimumWithdrawn: input.minimumWithdrawn },
    accounts: {
      marginfi: pinned(MARGINFI_V2),
      tokenProgram: pinned(TOKEN_PROGRAM),
      marginfiGroup: at(input.marginfiGroup),
      marginfiAccount: at(input.marginfiAccount),
      authority: at(input.authority),
      bank: at(input.bank),
      bankLiquidityVault: at(input.bankLiquidityVault),
      bankLiquidityVaultAuthority: at(input.bankLiquidityVaultAuthority),
      destinationAta: at(input.destinationAta),
      treasuryAta: at(input.treasuryAta),
    },
    accountGroups: { healthAccounts: marginfiHealthAccounts(input.remainingBalances) },
  });
}
// #endregion run
