# Remember state between runs

The values a run computes are gone when the run ends. To keep something for later runs, such as
how much a caller has spent or who may run the template, a template declares a **registry**: a
named set of fields. The values live in **entries**. An entry is an account that holds one copy of
the registry's fields, and a **key**, 32 bytes the template computes, picks which entry a run uses.
Key the entry by the caller's address, and each caller gets one of their own.

An entry is an account Ballista owns. The first run to open it creates it, and a payer the template
names pays its [rent](/reference/glossary#rent), about 0.0015 SOL for 16 bytes of fields. Nothing
closes an entry, so the rent is never returned. Only runs of its template can change it, anyone can
read it, and the same template published at a new address starts with no entries.

This page works through three examples: a run counter, a daily spending limit per caller, and an
allowlist. Every rule is under [Registries](/reference/language#registries) in the language
reference.

## Declare a registry

This template counts each caller's runs.

::: code-group

```ts [TypeScript · Template]
import { account, defineTemplate, expression, step } from '@jac0xb/ballista';

/** Count each caller's runs, in an entry of their own. */
export const countRuns = defineTemplate({
  registries: { runs: { count: 'u64' } },
  accounts: {
    caller: { signer: true, writable: true },
    // The caller's entry in `runs`, keyed by the caller's address. The caller pays to create it.
    callerRuns: account.registry('runs', { key: expression.accountKey('caller'), payer: 'caller' }),
    // Creating an entry calls the System program.
    systemProgram: account.systemProgram(),
  },
  steps: [
    step.setRegistry(
      'callerRuns',
      'count',
      expression.add(expression.registry('callerRuns', 'count'), expression.u64(1)),
    ),
  ],
});
```

<<< @/../clients/rust/examples/docs_language.rs#count-runs [Rust · Template]

:::

- `registries` names each registry and its fields: here `runs`, with one `u64`, `count`.
- `account.registry('runs', { key, payer })` declares the account that holds the caller's entry of
  `runs`. Before the first step, every run checks that account, or creates it.
- `key` is the caller's address, and the caller must sign, so a caller can open only their own
  entry. Leave `key` out for one entry that every run shares.
- `payer` pays the rent when a run creates the entry, and nothing after that.
- `account.systemProgram()` is there because creating an entry calls the System program.
- The step reads the field with `expression.registry` and writes it with `step.setRegistry`. Both
  name the entry's account, `callerRuns`, not the registry, so a template can open two entries of
  one registry, such as a sender's and a receiver's. If their keys come out equal, the run fails;
  see [Opening an entry](/reference/language#opening-an-entry).
- A write lands at once, so later steps read the new value. If the run fails, Solana undoes it.

::: warning Don't let the caller choose the key
The caller sets every input. An entry keyed by an input is one the caller chooses, so a caller
could open a fresh entry, with a fresh limit, on every run. Key an entry that limits callers by a
signer's address, or leave the key out.
:::

## A daily limit per caller

Let each caller send at most 1 SOL at once, with the allowance refilling over about a day. The
`rateLimit` helper returns the steps; the Rust version writes them out.

::: code-group

```ts [TypeScript · Template]
import { account, defineTemplate, expression, rateLimit, systemTransfer } from '@jac0xb/ballista';

/** Send SOL, at most 1 SOL at once per caller, refilling over about a day. */
export const dailyLimitPerCaller = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    caller: { signer: true, writable: true },
    recipient: { writable: true },
    callerLimit: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
    systemProgram: account.systemProgram(),
  },
  steps: [
    ...rateLimit({
      registry: 'callerLimit', // the entry's account, not the registry
      cap: expression.u64(1_000_000_000), // 1 SOL
      refillPerSecond: expression.u64(11_574), // 1 SOL over 86,400 seconds, rounded down
      amount: expression.input('amount'),
    }),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('caller'),
      to: account.fixed('recipient'),
      lamports: expression.input('amount'),
    }),
  ],
});
```

```ts [TypeScript · Run]
import { type Address } from '@solana/kit';

import { compileTemplate, registryIndex } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction, findRegistryEntryAddress } from '@jac0xb/ballista/kit';

export async function runDailyLimitPerCaller(run: {
  templateAddress: Address;
  caller: Address;
  recipient: Address;
  amount: bigint;
}) {
  const compiled = compileTemplate(dailyLimitPerCaller);
  // The caller's entry: registry `limits`, keyed by the caller's address, as the template keys it.
  const [callerLimit] = await findRegistryEntryAddress(
    run.templateAddress,
    registryIndex(compiled, 'limits'),
    run.caller,
  );
  return buildKitRunInstruction({
    compiled,
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      caller: { address: run.caller },
      recipient: { address: run.recipient },
      callerLimit: { address: callerLimit },
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    },
  });
}
```

<<< @/../clients/rust/examples/docs_language.rs#daily-limit-per-caller [Rust · Template]

<<< @/../clients/rust/examples/docs_language.rs#run-daily-limit-per-caller [Rust · Run]

:::

- `limits` holds the two fields `rateLimit` uses: `spent`, a `u64`, and `lastSpend`, an `i64` Unix
  time. Each caller has their own entry, `callerLimit`.
- Each run lowers `spent` by the seconds since `lastSpend` times `refillPerSecond`, but not below
  zero. Then it adds `amount`, requires the total to be at most `cap`, and writes `spent` and the
  time back. A new entry starts with nothing spent.
- At 11,574 lamports a second, a full 1 SOL refills in 86,401 seconds, just over a day. The limit
  refills continuously rather than resetting at midnight, so over any 24 hours a caller can send up
  to about 2 SOL: the full 1 SOL, plus what refills in that time.
- A run over the limit fails with `RequirementFailed` (6015) at the step labeled
  `withinRateLimit`, and the transfer does not happen.
- The Run tabs derive the caller's entry with `findRegistryEntryAddress` (from
  `@jac0xb/ballista/kit`) or `find_registry_entry_address` (Rust), from the template address, the
  registry's index and the caller's address. `registryIndex(compiled, 'limits')` gives the index,
  here 0.

`rateLimit` refuses a `cap` or `refillPerSecond` that the caller could set, such as an input. It
can't see the key, so key the entry by a signer's address, as here, or leave the key out for one
limit that every caller shares. Give each `rateLimit` in a template its own `name`: it names the
requirement, `within<Name>`, so a failure says which limit was hit. The full rules are under
[Spending limits](/reference/language#spending-limits).

## An allowlist

Access control needs no feature of its own: it is a step that reads an entry. Here an entry's `ok`
flag says whether its key may make the call. The author's runs set flags, and everyone else's runs
must find their own flag set. The call is a marked stand-in (the System program's Transfer) so the
example runs as written.

::: code-group

```ts [TypeScript · Template]
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace AUTHOR with the author's address, and the
// program and data with the call the list guards.
const AUTHOR = new Uint8Array(32).fill(7);
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CALL_DATA = Uint8Array.of(2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0);

// True only in the author's runs: `caller` must sign, so only the author can match AUTHOR.
const isAuthor = expression.equal(expression.accountKey('caller'), expression.pubkey(AUTHOR));

/** Only listed callers make the call. The author's runs add or remove a member instead. */
export const listedCallersOnly = defineTemplate({
  inputs: {
    member: { type: 'pubkey' }, // author's runs: whose entry to set
    allow: { type: 'bool' }, // author's runs: the flag to set
  },
  registries: { allowed: { ok: 'bool' } },
  accounts: {
    caller: { signer: true, writable: true },
    // The author's runs open the member's entry. Everyone else's runs open their own.
    entry: account.registry('allowed', {
      key: expression.select(isAuthor, expression.input('member'), expression.accountKey('caller')),
      payer: 'caller',
    }),
    systemProgram: account.systemProgram(),
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    pool: { writable: true },
  },
  steps: [
    // The author-only branch: set the member's flag. Other runs write back the flag already there.
    step.setRegistry(
      'entry',
      'ok',
      expression.select(isAuthor, expression.input('allow'), expression.registry('entry', 'ok')),
    ),
    // Everyone but the author must be listed.
    step.require(expression.or(isAuthor, expression.registry('entry', 'ok')), 'listed'),
    // The call the list guards. The author's runs skip it.
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
```

```ts [TypeScript · Run]
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
```

<<< @/../clients/rust/examples/docs_language.rs#listed-callers-only [Rust · Template]

<<< @/../clients/rust/examples/docs_language.rs#run-listed-callers-only [Rust · Run]

:::

- **The author-only branch.** `isAuthor` is true only when `caller`, who must sign, is the author.
  A template has no `if` step, so this one branches with `expression.select(condition, a, b)`,
  which gives `a` when the condition is true and `b` otherwise.
- **One entry per run.** The key is `member` in the author's runs and the caller's own address in
  everyone else's, so only the author's runs take the key from an input.
- **Setting a flag.** In the author's runs, the first step writes `allow` into the member's entry.
  A write can't be skipped, so in anyone else's it writes back the flag already there, which
  changes nothing.
- **The check.** The `require` passes for the author, and for callers whose flag is set. Anyone
  else's run fails at `listed` with `RequirementFailed` (6015). The whole transaction fails, so the
  entry that run created is undone too, and the caller pays no rent.
- **The call** has `when: expression.not(isAuthor)`, so the author's runs only set flags. The
  author pays the rent for each new member's entry.
- **Inputs.** Every run must pass `member` and `allow`, but only the author's runs read them. The
  Run tabs derive the entry from the same key the template computes.
- **Removing a member** sets their flag to `false`. The entry stays, since entries are never closed.
