import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../drift-rebalance-exact.js';
import { DRIFT_V2, MARGINFI_V2 } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildDriftRebalanceRun(input: {
  templateAddress: Address;
  owner: Address;
  walletAta: Address;
  marginfiGroup: Address;
  marginfiAccount: Address;
  marginfiBank: Address;
  marginfiVault: Address;
  marginfiVaultAuthority: Address;
  driftState: Address;
  driftUser: Address;
  driftUserStats: Address;
  driftSpotMarketVault: Address;
  minimumMoved: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { minimumMoved: input.minimumMoved },
    accounts: {
      marginfi: pinned(MARGINFI_V2),
      drift: pinned(DRIFT_V2),
      tokenProgram: pinned(TOKEN_PROGRAM),
      owner: at(input.owner),
      walletAta: at(input.walletAta),
      marginfiGroup: at(input.marginfiGroup),
      marginfiAccount: at(input.marginfiAccount),
      marginfiBank: at(input.marginfiBank),
      marginfiVault: at(input.marginfiVault),
      marginfiVaultAuthority: at(input.marginfiVaultAuthority),
      driftState: at(input.driftState),
      driftUser: at(input.driftUser),
      driftUserStats: at(input.driftUserStats),
      driftSpotMarketVault: at(input.driftSpotMarketVault),
    },
  });
}
