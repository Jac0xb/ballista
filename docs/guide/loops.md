# Loops over rows and counts

This page shows loops in which each row decides what to do from state read during the run: pay
creditors in priority order, collect only the token accounts that hold a balance, process only
the queue entries that are due, and split a balance by weight. The last example repeats a call as
many times as a count read during the run.

A template declares its rows as a **batch**, one **row** per item. In `batch`, `row` names the
accounts in each row, `rowInputs` declares values the caller supplies for each row, and
`maxIterations` caps the number of rows. `step.forEach` holds the steps that run once per row.
Inside it, `account.iteration('name')` is the current row's account and
`expression.rowInput('name')` is the current row's value. [Batch execution](/guide/batching)
covers the rules and limits.

A template can hold up to eight loops. They run one after another, never one inside another, and
every `forEach` runs over the same rows. `step.repeat` loops over a count instead of rows, as the
[last example](#crank-once-per-waiting-entry) shows.

The accounts the caller passes fix how many rows run. What each row does can still depend on what
earlier rows spent, or on the row's own account. `when` skips a single call when its condition is
false. In a run, the row accounts follow the fixed accounts, and the row inputs follow the fixed
inputs. The examples that call another protocol use marked stand-ins (the System program and its
Transfer data) so they compile and run as written.

## Waterfall until the money runs out

Pay creditors from a treasury in priority order, the first row first. Each payment is capped by
what is left, and what is left is known only when the transaction executes.

::: code-group

<<< @/../clients/js/examples/docs/waterfall-until-the-money-runs-out.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/waterfall-until-the-money-runs-out.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#waterfall-until-the-money-runs-out [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#waterfall-until-the-money-runs-out [Rust · Run]

:::

Each row is one creditor account, marked writable (the transaction may change it) because it
receives [lamports](/reference/glossary#lamports). `remaining` starts as the treasury's balance
minus a reserve. Each row pays the smaller of `remaining` and that creditor's `owed` amount, then
subtracts the payment.

`carry: ['remaining']` is what passes the balance from one row to the next. A variable created
before the loop and listed in `carry` can be updated with `step.assign` inside the loop, so row
four sees what rows one to three paid. Variables created inside the loop, such as `pay`, start
fresh on every row. Once the money runs out, `pay` is zero and `when` skips the transfer, so the
later creditors get nothing and the transaction still succeeds. In Rust, the first argument to
`for_each` is the carry: a bit mask with one bit per carried register.

## Consolidate only the funded accounts

Move the whole balance of each token account in the batch into one vault, and skip the accounts
that are empty.

::: code-group

<<< @/../clients/js/examples/docs/consolidate-only-the-funded-accounts.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/consolidate-only-the-funded-accounts.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#consolidate-only-the-funded-accounts [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#consolidate-only-the-funded-accounts [Rust · Run]

:::

Each row reads its own account's balance (the `u64` at offset 64 of an SPL Token account) and
transfers all of it, and `when` skips the transfer when the balance is zero. Sending one plain
Transfer instruction per account needs every amount before signing, and the whole transaction
fails at the first account that turns out to be empty.

## Crank only the ripe entries

A crank is a transaction that processes the items waiting in a protocol's queue. Keepers (bots
that do routine maintenance for a protocol) send them. This template settles only the entries
whose deadline has passed.

::: code-group

<<< @/../clients/js/examples/docs/crank-only-the-ripe-entries.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/crank-only-the-ripe-entries.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#crank-only-the-ripe-entries [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#crank-only-the-ripe-entries [Rust · Run]

:::

Each row is one queue entry. The template reads the entry's deadline, a Unix timestamp at
`DEADLINE_OFFSET`, and compares it with the network's clock when the transaction executes. For
each entry that is due, `step.invoke` makes a [CPI](/reference/glossary#cpi) into the protocol.
The keeper can pass the whole queue and let the run decide. Filtering the queue before sending
would compare the deadlines with a time that has already passed when the transaction runs.

## Distribute a runtime pot pro rata

Split a vault's balance above a reserve (the pot) among holders, each in proportion to a weight
the caller passes.

::: code-group

<<< @/../clients/js/examples/docs/distribute-a-runtime-pot-pro-rata.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/distribute-a-runtime-pot-pro-rata.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#distribute-a-runtime-pot-pro-rata [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#distribute-a-runtime-pot-pro-rata [Rust · Run]

:::

`weightBps` is each holder's share in basis points (hundredths of a percent, so 10,000 is 100%).
The caller chooses the weights, and the template reads the pot when the transaction executes.
Computing the shares off chain would divide a balance that may have changed by then. Each share
is rounded down, and whatever the rounding leaves over stays in the vault.

## Crank once per waiting entry

`step.repeat(count, steps, { max })` is a count loop: it runs its steps `count` times, and it has
no rows. This template reads how many entries wait in a queue when the transaction executes, and
cranks the queue that many times, up to eight. A plain transaction fixes its number of crank
instructions when it is signed.

::: code-group

```ts [TypeScript · Template]
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins, as in the examples above: replace them with the queue program's address, its crank
// instruction data, and the offset of the waiting count in its queue account.
const QUEUE_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CRANK_DATA = Uint8Array.of(2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0);
const WAITING_OFFSET = 8;

/** Crank the queue once for each waiting entry, at most eight times. */
export const crankOncePerWaitingEntry = defineTemplate({
  accounts: {
    queueProgram: { executable: true, address: QUEUE_PROGRAM },
    keeper: { signer: true, writable: true },
    queue: { writable: true, owner: QUEUE_PROGRAM },
  },
  steps: [
    step.repeat(
      expression.min(expression.accountData(account.fixed('queue'), WAITING_OFFSET, 'u64'), expression.u64(8)),
      [
        step.invoke({
          program: account.fixed('queueProgram'),
          accounts: [
            { account: account.fixed('keeper'), signer: true, writable: true },
            { account: account.fixed('queue'), signer: false, writable: true },
          ],
          data: [data.literal(CRANK_DATA)],
        }),
      ],
      { max: 8 },
    ),
  ],
});
```

```rust [Rust · Template]
/// Crank the queue once for each waiting entry, at most eight times.
pub fn crank_once_per_waiting_entry() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // The same stand-ins.
    const QUEUE_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const CRANK_DATA: [u8; 12] = [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
    const WAITING_OFFSET: u64 = 8;

    let mut builder = ProgramBuilder::new();
    let queue_program = builder.account(ACCOUNT_EXECUTABLE, Some(QUEUE_PROGRAM), None, 0);
    let keeper = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    // 16 bytes, so the u64 at offset 8 fits. The TypeScript compiler works this out itself.
    let queue = builder.account(ACCOUNT_WRITABLE, None, Some(QUEUE_PROGRAM), 16);

    let most = builder.const_u64(8);
    let waiting = builder.read(OP_READ_U64, queue, WAITING_OFFSET);
    let count = builder.binary(OP_MIN, waiting, most);
    let crank_ix = builder.blob(&CRANK_DATA);
    let crank = builder.cpi(
        queue_program,
        &[(keeper, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (queue, ACCOUNT_WRITABLE)],
        &[Segment::Literal(crank_ix)],
    );
    // Runs `count` times, at most 8. The 0 is the carry mask: nothing is carried.
    builder.repeat(count, 8, 0, |pass| pass.invoke(crank, None));
    builder.build().expect("template builds")
}
```

:::

- The count is a `u64`, read once, when the loop starts. A count of 0 skips the loop.
- `max`, from 1 to 255, is the most times the loop may run. A run whose count is above it fails
  with `LoopCountExceeded` (6022), so this template caps the count with `min`.
- Finalization, the one-time check before a template is locked, counts the loop's calls at `max`:
  here 8 of the 64 a run may make.
- `carry` and `expression.loopIndex()` (the pass number, from 0) work as they do in `forEach`.
  There are no rows, so `account.iteration` and `expression.rowInput` are rejected inside.

The TypeScript SDK refuses a ninth loop, a `repeat` inside another loop, and a row named inside a
`repeat`. A template built another way with one of these fails finalization with `InvalidLoop`
(6129).
