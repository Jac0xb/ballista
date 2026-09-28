import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../marginfi-withdraw-all-with-floor.js';
import { MARGINFI_V2 } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildMarginfiWithdrawRun(input: {
  templateAddress: Address;
  marginfiGroup: Address;
  marginfiAccount: Address;
  authority: Address;
  bank: Address;
  bankLiquidityVault: Address;
  bankLiquidityVaultAuthority: Address;
  destinationAta: Address;
  treasuryAta: Address;
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
  });
}
