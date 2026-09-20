# Row inputs and dynamic account groups

Design for two additions to the Ballista bytecode, and for naming the completed format version 1.

## Why

A template today is a fixed transaction shape. Two gaps follow from that:

- A batch row carries accounts and nothing else, so every iteration runs with the same instruction
  data. Paying a different amount to each recipient needs a staging account.
- A CPI's account list is positional and frozen at authoring time. A template that swaps through
  Jupiter is pinned to one route shape, and a template that chooses between three swaps at run
  time would need every route's accounts declared up front.

The target scenario is: a template snapshots account data, decides which of up to three Jupiter
swaps to run, and runs the chosen ones, where each swap can be any route over any token. Each
swap needs its own route bytes and its own list of AMM accounts, and a skipped swap should cost
nothing to supply.

## Row inputs

A template with a batch may declare a row of inputs alongside its row of accounts.

- **Header.** `reserved[0]` becomes `row_input_count` (0 to 8). The inputs table holds the fixed
  descriptors followed by the row descriptors, `input_count + row_input_count` in total.
- **Run data.** After the group-length prefix (below) come the fixed input values, then
  `iterations × row_input_count` values in iteration order, each encoded as fixed inputs are. The
  iteration count is already known from the account count before parsing.
- **Bytecode.** `LOAD_INPUT` with bit `0x80` set in `a` loads row input `a & 0x7f` of the current
  iteration. This is the convention iteration accounts use.
- **Verifier.** Row inputs require a batch (`batch_stride > 0`). A row load is accepted only inside
  `forEach`, its offset must be below `row_input_count`, and its type comes from the row descriptor.
  `input_count + row_input_count` stays within `MAX_INPUTS` (32), and
  `input_count + batch_max_iterations × row_input_count` must not exceed `MAX_INPUT_VALUES` (256),
  which bounds the parsed values at about 10 KB of heap.
- **Errors.** Too few or malformed values report `InvalidRunInputs` with the value index as context,
  counting row values after the fixed ones. Trailing bytes report the total value count.

## Dynamic account groups

A template may declare up to eight named groups of accounts whose members and count are chosen by
the caller at run time. A CPI may forward one group after its declared accounts.

- **Header.** `reserved[1]` becomes `account_group_count` (0 to 8).
- **Run data prefix.** Run data begins with `account_group_count` bytes, one per group, giving the
  number of accounts in that group. A group may be empty.
- **Runtime accounts.** Fixed accounts, then batch rows, then the groups in declaration order. The
  batch iteration count is `(total − fixed − Σ group lengths) / stride`, so batches and groups
  coexist without ambiguity.
- **CPI descriptor.** `reserved0` becomes `account_group`: `NO_INDEX` for none, otherwise a group
  index. The CPI's accounts are its declared records followed by every account of the group. The
  runtime already caps a CPI at the caller's privileges, so a template cannot escalate through a
  group.
- **Group accounts are opaque.** They have no constraints, cannot be read, and cannot be named by
  any instruction. Anything a template must check about a swap (balances, owners, mints) is read
  from declared accounts; a swap's user token accounts are declared, unpinned slots.
- **Group accounts never sign.** They are forwarded with the transaction's writable flag and with
  signer cleared, so a template can only delegate a signature through a slot its author declared.
  The accounts a route needs beyond the user's own are pools and vaults, which never sign.
- **Groups are per CPI, not per row.** An invoke inside `forEach` forwards the same group on every
  iteration. A group per row is a possible follow-on (a length byte per row and an iteration bit on
  the reference) and is not part of this design.
- **Limits.** Declared accounts plus the group must fit `MAX_CPI_ACCOUNTS` (64), checked at run time
  and reported as runtime error 6021 `CpiAccountLimitExceeded` with the total as context.
  `MAX_RUNTIME_ACCOUNTS` rises from 60 to 120 and the entrypoint's account capacity to 128, so three
  route-sized groups fit. The transaction size limit of 1,232 bytes remains the practical bound;
  three swaps need an address lookup table and single-hop routes.
- **Errors.** A truncated prefix reports `InvalidRunInputs` with context 0. Group lengths that exceed
  the supplied accounts, or leave a remainder that is not a whole number of rows, report
  `InvalidAccountRange` with the account count as context, as today.
- **Verifier.** `account_group_count ≤ 8` (`TooManyAccountGroups` otherwise); every descriptor's
  `account_group` is `NO_INDEX` or below the count. Static capacity (`fixed + stride × max_iterations ≤ MAX_RUNTIME_ACCOUNTS`) is unchanged;
  groups are bounded at run time by the same total.

## The three-swap template

Fixed accounts: Jupiter program (pinned, executable), the user authority (signer), and one unpinned
source and destination token account per swap. Fixed inputs: three `bytes` route payloads. Three
groups: `routeA`, `routeB`, `routeC`. Steps: snapshot the balances the decision depends on, compute
three booleans, and three `invoke` steps each guarded by `when`, each forwarding its group, followed
by balance-delta `require` checks for the swaps that ran. At run time the caller supplies route
bytes and group accounts only for the swaps its own quote selected and passes empty groups for the
rest.

## SDK surface

TypeScript schema and compiler:

- `batch.rowInputs: Record<name, InputDefinition>`; `expression.rowInput(name)` inside `forEach`.
- `accountGroups: string[]` on the template; `invoke({ ..., accountGroup: name })`.
- `CompiledTemplate` gains `rowInputOrder` and `accountGroupOrder`; stats gain `rowInputs` and
  `accountGroups`.
- `encodeRunInputs(compiled, values, { rows?, groupLengths? })` writes prefix, fixed values, rows.
- `buildRunInstruction` takes `batchInputs?: Record<name, RunInputValue>[]` parallel to `batchRows`
  and `accountGroups?: Record<name, AccountBinding[]>`; the metas are appended in group order and
  the prefix is derived from their lengths.

Rust builder and client:

- `ProgramBuilder::row_input(value_type, max_len) -> u8` returning the row offset,
  `load_row_input(offset)`, `groups(count)`, and `cpi_with_group(program, accounts, segments, group)`.
- `RunInputs::groups(&[u8])` writes the prefix and must be called first; row values are appended
  after fixed values with the existing methods, in iteration order.

Both compilers stay byte-identical on the shared fixtures, which gain a payroll with per-row
amounts and a group-forwarding transfer.

## Version 1

The program is deployed once and immutably, so the format carries no version history. When this
work is complete the bytecode is version 1: `TEMPLATE_PROGRAM_VERSION = 1`, magic `BVM1`, the
template PDA seed `template`, crate and package versions `1.0.0`, and the docs speak of the format
without a version. Fixtures, the Certora constant programs, and the wire-format reference are
regenerated once at that point.

## Tests

- Executor unit tests: row-input loads inside and outside a loop, prefix parsing, group forwarding
  flags, the 64-account cap, batch and group coexistence.
- Verifier table tests: every new rejection.
- Generator strategies produce row inputs and groups so the no-panic and generated-program
  properties cover them.
- Mollusk: payroll paying three different amounts; a padded System transfer forwarding a two-account
  group; an empty group under a false `when`; a group that overflows the CPI cap; a missing prefix.
- TypeScript: schema validation, compilation, run encoding, and the new fixtures.

## Documentation

Batching gains row inputs and the payroll example pays different amounts. A new guide page covers
dynamic account groups with the three-swap template. The limits page, wire-format reference, and
errors page are updated. The version-1 naming replaces every mention of the current version.
