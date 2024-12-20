/**
 * This code was AUTOGENERATED using the codama library.
 * Please DO NOT EDIT THIS FILE, instead use visitors
 * to add features, then rerun codama to update it.
 *
 * @see https://github.com/codama-idl/codama
 */

import {
  getAddressEncoder,
  getProgramDerivedAddress,
  getU8Encoder,
  getUtf8Encoder,
  type Address,
  type ProgramDerivedAddress,
} from '@solana/web3.js';

export type TaskDefinitionSeeds = {
  payer: Address;

  taskId: number;
};

export async function findTaskDefinitionPda(
  seeds: TaskDefinitionSeeds,
  config: { programAddress?: Address | undefined } = {}
): Promise<ProgramDerivedAddress> {
  const {
    programAddress = 'BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2' as Address<'BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2'>,
  } = config;
  return await getProgramDerivedAddress({
    programAddress,
    seeds: [
      getUtf8Encoder().encode('task-definition'),
      getAddressEncoder().encode(seeds.payer),
      getU8Encoder().encode(seeds.taskId),
    ],
  });
}
