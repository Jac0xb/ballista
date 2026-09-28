# Template registry

Status: approved design · 2026-09-27 · runtime phase 5, after phases 1–4 (math, loops, output,
introspection) merge. Branch `claude/runtime-registry`, from the combined runtime branch.

## Goal

Give a template state that outlives a run and that only its own runs can change. With the clock
and the math opcodes Ballista already has, that is enough to express spending caps, limits that
refill over time, per-caller counters, and allowlists. Access control needs no program feature of
its own: it is a step that reads an entry.

## What must stay true

- A finalized template never changes. The template account stays read-only in every run.
- Only runs of a template can write that template's entries.
- Ballista signs only to create an entry's own account, inside the step that opens it. A template
  never gets a signature it can use in its own CPIs.
- The run path trusts finalized templates. Every structural rule is checked at finalize; only
  what depends on run-time values (the entry's key, whether it exists) is checked at run time.
- The dispatch loop's outer match gains no arms, guards or `if`s. The new opcodes reach
  `extended_instruction` through its fallback, and their work runs in `#[inline(never)]` helpers.
- An entry holds its rent and nothing else.

## Entries

An entry is an account Ballista owns, one per template, registry and key.

| | |
| --- | --- |
| Address | `find_program_address(["registry", template, [registry index], key], ballista)` |
| Header, 72 bytes | magic `BREG` (4), version `1` (1), registry index (1), reserved zeros (2), template address (32), key (32) |
| Fields | the registry's declared layout, zeroed at creation, starting at byte 72 |
| Size | header plus the registry's size, fixed when the template is finalized |
| Lifetime | never closed; its rent stays locked |

- **The key** is 32 bytes the template computes: the caller's address for a per-caller entry, an
  input or an account key for other scopes, or all zeros for one template-wide entry.
- **Anyone can read** an entry: it is an ordinary account. Only the running template can write it.
- **A new template address starts fresh.** Entries are keyed by template address, so publishing a
  new version of a template starts every limit from zero.

## Opening an entry

A template opens each entry it uses once, at the root, before touching it. Opening either checks
an existing entry or creates a missing one, so a client never needs a separate instruction.

- **The entry exists** (owned by Ballista, with data): Ballista checks its size, magic, version,
  registry index, template address and key against what the template declares and computed. Any
  mismatch fails with `InvalidRegistryEntry`. This check is what stops a caller from passing
  another caller's entry, or a fresh entry under a different key, to dodge a limit.
- **The entry does not exist** (no data, owned by the System program): Ballista derives the
  address, fails with `InvalidRegistryEntry` if the account passed is not it, then creates it:
  - with no lamports: `create_account` from the payer, rent-exempt for the entry's size, owned by
    Ballista;
  - with lamports already there (someone funded the address to block `create_account`): a
    transfer of whatever rent is still missing, then `allocate` and `assign`;
  - each call signed with the entry's own seeds; then the header is written.
- **The payer** is an account the template declares as a signer and writable. It pays only when an
  entry is created.
- **Creation is a CPI.** It counts as up to three CPIs toward `MAX_EXPANDED_CPIS`, and like
  `INVOKE` it may not follow `SET_RETURN_DATA`.

## Reading and writing fields

After an entry is opened, the template reads and writes its fields by naming the entry's account,
a field offset and a width; the run keeps no table of open entries. Writes take effect at once; a
failed run rolls them back with the transaction.

- **No lost updates.** An open entry stays marked as borrowed for the rest of the run, so a CPI
  that passes it writable fails with `RegistryReentry`, whatever program it calls. Only Ballista
  can change a Ballista-owned account, and a nested run cannot write an entry it was not passed
  writable, so no run can change an entry between this run's read and its write. A CPI to Ballista
  that leaves the entry out still runs. The check reuses the borrow flag the invoke path already
  tests, so it costs nothing.

## Opcodes

| Opcode | Operands | Effect |
| --- | --- | --- |
| 75 `OPEN_REGISTRY` | `a` entry account, `b` key register (`pubkey`, or none for the zero key), `c` payer account; immediate: registry index, registry size, System program account | Checks or creates the entry, then keeps it borrowed for the run |
| 76 `READ_REGISTRY` | `dst`, `a` entry account; immediate: field offset, a read opcode as the width selector | the field, typed as that read |
| 77 `WRITE_REGISTRY` | `a` value register, `b` entry account; immediate: field offset, width selector | writes the value (`bool`, `u64`, `i64`, `u128` or `pubkey`); its type must match the width |

Verifier rules, `InvalidRegistry` unless noted:
- `OPEN_REGISTRY` only at the root, never inside a loop; each entry account opened once; at most
  8 open entries;
  registry index below 8; field size 1 to 512 bytes, not counting the header; every open of one
  registry index declares the same size.
- The entry account is a fixed account declared writable and nothing else: not a signer or
  executable, pinned to no address or owner, and with no data-length floor. Each of those would
  fail every run's account checks, or the creation. The payer is a fixed account declared signer
  and writable. The System program account is a fixed account pinned to the System program.
- No CPI lists an entry account writable, before its open or after it: after, the CPI would
  always fail with `RegistryReentry`. A row account or a group member that turns out to be the
  entry is left to that run-time check.
- The key register is a set `pubkey` register.
- `READ_REGISTRY` and `WRITE_REGISTRY` name an entry account opened at a lower pc, and a field
  range inside its registry's size.
- No read opcode or `READ_ACCOUNT_BYTES` names an entry account, before its open or after it:
  fields are read only with `READ_REGISTRY`. Before the open, a CPI could still change the entry
  after such a read. The entry's key, owner, lamports and data length stay readable.
- No `OPEN_REGISTRY` after `SET_RETURN_DATA`.

## Errors

| Code | Kind | When |
| --- | --- | --- |
| 6025 | `InvalidRegistryEntry` (runtime) | wrong address at creation; wrong owner, size or header |
| 6026 | `RegistryReentry` (runtime) | a CPI passes an open entry writable |
| 6132 | `InvalidRegistry` (verifier) | any rule above |

An entry account passed read-only fails earlier, at account validation, with the existing
`AccountConstraintFailed` (6020).

## TypeScript

```ts
defineTemplate({
  inputs: { amount: { type: 'u64' } },
  registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    caller: { signer: true, writable: true },
    limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
    systemProgram: account.systemProgram(),
  },
  steps: [
    // 1 SOL a caller, refilling over a day.
    ...rateLimit({ registry: 'limits', cap: expression.u64(1_000_000_000),
      refillPerSecond: expression.u64(11_574), amount: expression.input('amount') }),
  ],
});
```

The cap and the refill rate are literals. They must not come from the caller: whoever builds the
transaction sets every input, so a cap taken from an input is a limit the caller picks. Values the
author controls work too, such as a registry field only an author-only branch writes.

- The compiler places each `OPEN_REGISTRY` at the start of the root, in declaration order, and maps
  field names to offsets. A key may read the fields of entries declared before its own; one that
  reads a later entry, or its own, is refused at compile time, since its read would precede the
  open.
- `expression.registry(entryAccount, 'spent')` reads a field and `step.setRegistry(entryAccount,
  'spent', value)` writes one. They name the entry's account, so two entries of one registry (for
  example a sender's and a receiver's) stay distinct. `expression.accountKey` and
  `account.systemProgram()` are new helpers the example needs.
- `rateLimit(...)` is an SDK helper, like `ed25519Signature`. It reads `spent` and `lastSpend`,
  takes `now` as the clock or `lastSpend` if the clock reads earlier, refills `spent` by
  `(now − lastSpend) × refillPerSecond` without going below zero (in `u128`, so a long gap cannot
  overflow; a fresh entry's `lastSpend` of 0 refills fully), adds `amount`, requires the total to
  be at most `cap` (`withinRateLimit`), and writes `spent` and `now` back. The clock can step back
  between slots; `lastSpend` never does, so no run refills the same seconds twice.
- The Rust `ProgramBuilder` gains `open_registry`, `read_registry` and `write_registry`.
- A client derives an entry's address and bump with `findRegistryEntryAddress(template,
  registryIndex, key)` from `@jac0xb/ballista/kit`, or `find_registry_entry_address` in the Rust
  SDK. `registryIndex(compiled, name)` gives a registry's index by name, from the compiled
  template's `registryOrder`. Both SDKs test against vectors the program's own derivation
  produces, in `fixtures/registry-entry-addresses.txt`.

**Allowlists are steps.** A registry such as `allowed: { ok: 'u64' }` keyed by the caller; ordinary
runs require `ok == 1`; an author-only branch (the author a pinned signer) opens the entry keyed
by an input address and sets it.

## Testing

- **Verifier:** each rule rejected with its specific error.
- **Executor:** open existing, create (with and without pre-funded lamports), wrong address, wrong
  owner, another template's entry, another key's entry, wrong size, reads and writes by width,
  `RegistryReentry`.
- **Mollusk, end to end:** a TypeScript-compiled rate-limited transfer. The first run creates the
  entry and the payer pays rent; later runs spend within the cap; one over the cap fails at
  `withinRateLimit`; after the clock moves, the refill lets it land again.
- **Property tests:** the generator produces registries, and generated programs never hit
  structural errors.
- **Certora:** the typing rule covers the new opcodes; the new errors are value-dependent.
- **Compute units:** a new ceiling case for open-and-update; every existing case measured, since
  the invoke path gains the reentry check.
- **Real protocols:** in `tests/protocols`, a per-caller daily cap on a Jupiter swap, with the clock
  moved under write rule 3.

## Docs

The trust model says Ballista "keeps no state between runs" and "never signs". Both change: it
keeps state in registry entries only, and it signs only to create an entry. The language
reference, limits and errors pages gain the registry. The docs session updates the pages.

## Out of scope

- Closing entries or reclaiming their rent.
- Opening entries inside loops, for example one entry per row.
- Entries whose size changes after creation, and layouts that change between template versions.
- One template writing another template's entries.
