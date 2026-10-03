// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace AUTHOR with the author's address, and the
// program and data with the call the list guards.
const AUTHOR = new Uint8Array(32).fill(7);
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CALL_DATA = Uint8Array.of(2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0);

// True only in the author's runs: `caller` must sign, so only the author can match AUTHOR. A
// template has no `if` step; it branches with `expression.select(condition, a, b)`, which gives `a`
// when the condition is true and `b` otherwise.
const isAuthor = expression.equal(expression.accountKey('caller'), expression.pubkey(AUTHOR));

/** Only listed callers make the call. The author's runs add or remove a member instead. */
export const listedCallersOnly = defineTemplate({
  // Every run passes both inputs, but only the author's runs read them.
  inputs: {
    member: { type: 'pubkey' }, // author's runs: whose entry to set
    allow: { type: 'bool' }, // author's runs: the flag to set
  },
  registries: { allowed: { ok: 'bool' } },
  accounts: {
    caller: { signer: true, writable: true },
    // One entry per run: the member's in the author's runs, the caller's own in everyone else's,
    // so only the author picks the key. The author pays the rent for each new member's entry.
    entry: account.registry('allowed', {
      key: expression.select(isAuthor, expression.input('member'), expression.accountKey('caller')),
      payer: 'caller',
    }),
    systemProgram: account.systemProgram(),
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    pool: { writable: true },
  },
  steps: [
    // The author's runs write `allow` into the member's entry; `false` removes them, and the entry
    // stays, since entries are never closed. A write can't be skipped, so everyone else's runs
    // write back the flag already there, which changes nothing.
    step.setRegistry(
      'entry',
      'ok',
      expression.select(isAuthor, expression.input('allow'), expression.registry('entry', 'ok')),
    ),
    // Everyone but the author must be listed. Anyone else fails at `listed` with RequirementFailed
    // (6015), and the entry their run created is undone too, so they pay no rent.
    step.require(expression.or(isAuthor, expression.registry('entry', 'ok')), 'listed'),
    // The call the list guards. The author's runs skip it, so they only set flags.
    step.invoke({
      program: account.fixed('protocolProgram'),
      accounts: [
        { account: account.fixed('caller'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.literal(CALL_DATA)],
      when: expression.not(isAuthor),
    }),
  ],
});
// #endregion template

// #region run
import { getAddressEncoder, type Address } from '@solana/kit';

import { compileTemplate, registryIndex } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction, findRegistryEntryAddress } from '@jac0xb/ballista/kit';

const PROTOCOL_PROGRAM_ADDRESS = SYSTEM_PROGRAM_ADDRESS; // the same stand-in

export async function runListedCallersOnly(run: {
  templateAddress: Address;
  caller: Address;
  pool: Address;
  /** The author's runs only: the member to add or remove. */
  set?: { member: Address; allow: boolean };
}) {
  const compiled = compileTemplate(listedCallersOnly);
  // The key the template computes: the member in the author's runs, the caller in everyone else's.
  const key = run.set?.member ?? run.caller;
  const [entry] = await findRegistryEntryAddress(run.templateAddress, registryIndex(compiled, 'allowed'), key);
  return buildKitRunInstruction({
    compiled,
    templateAddress: run.templateAddress,
    // Every run passes both inputs. Only the author's runs read them.
    inputs: { member: Uint8Array.from(getAddressEncoder().encode(key)), allow: run.set?.allow ?? false },
    accounts: {
      caller: { address: run.caller },
      entry: { address: entry },
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      pool: { address: run.pool },
    },
  });
}
// #endregion run
