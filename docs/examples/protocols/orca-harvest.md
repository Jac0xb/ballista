# Harvest positions that earned

<p class="protocol-line">Orca</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against Orca's
Whirlpools program and a SOL/USDC pool copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 14,587 of the tested four-row transaction's 60,880
[compute units](/reference/glossary#compute-units); Whirlpools took the rest. Ballista charges no
fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Collects fees from up to 12 Orca Whirlpools positions in one transaction, skipping the ones whose
fees are all at or below `dustFloor`. The positions must share one pool and one holder.

A liquidity provider may hold dozens of positions. Which of them have earned since the last harvest
depends on trades that land after the transaction is signed, so the template checks each one during
the run. Whoever holds a position's NFT, a token with a supply of one, owns the position.

The template reads who owns the fee accounts, `tokenOwnerAccountA` and `tokenOwnerAccountB`, once.
Then, for each position, it:

1. requires the holder of the position's NFT to own both fee accounts
   (`positionBelongsToTheFeeOwner`). Whirlpools checks only the fee accounts' mints, so a run built
   by someone else could otherwise send the fees anywhere;
2. calls `update_fees_and_rewards` if the position has liquidity. Without it, `fee_owed_a` and
   `fee_owed_b` hold only what the last update recorded, and a position that earned looks empty;
3. calls `collect_fees` if either fee is above `dustFloor`.

Skipping a collect saves about 13,300 compute units and leaves dust alone. It doesn't prevent
reverts: `collect_fees` with nothing owed succeeds and moves nothing. What reverts the whole harvest
is a row that fails regardless of its fees: a position the signer isn't allowed to sign for
(Whirlpools' `MissingOrInvalidDelegate`, 6019), one from another pool (`ConstraintHasOne`, 2001), or
one of another holder (`positionBelongsToTheFeeOwner`). None of these depends on trades, so leave
such positions out when you build the run.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/orca-harvest-many-positions.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#orca-harvest [Rust · Template]

<<< @/../clients/js/examples/protocols/run-orca-harvest.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#orca-harvest [Rust · Run]

:::

The offsets come from Orca's `Position` account and the SPL Token account; see
[reading offsets](/examples/protocols/#reading-offsets-from-an-account). The Rust tabs' `program`,
`anchor` and account flags are [shared helpers](/examples/protocols/#rust-helpers).

## Run it

The positions form a [batch](/guide/batching). The Run tabs pass the eight declared accounts in
order, `whirlpoolProgram`, `tokenProgram`, `positionAuthority`, `whirlpool`, `tokenOwnerAccountA`,
`tokenOwnerAccountB`, `tokenVaultA` and `tokenVaultB`, then one row of four accounts per position,
then the input `dustFloor`. A row is:

- `position`;
- `positionTokenAccount`, the token account holding the position's NFT;
- `tickArrayLower` and `tickArrayUpper`, which hold the position's lower and upper ticks, the prices
  its range starts and ends at. Whirlpools stores a pool's ticks 88 to an account, in tick arrays.
  `getOrcaTickArrayAddress`, in the TypeScript run, finds the one holding a tick; a test checks it
  against mainnet's tick arrays.

Pass 1 to 12 rows; the row count comes from the account list, so there is no count to pass. Every
row must be a position in `whirlpool`, held by the owner of the fee accounts. Another holder's
positions need their own run, with that holder's fee accounts. Both pool mints must be SPL Token
mints, as SOL and USDC are.

`positionAuthority` signs for every position. It can be the holder, or a delegate: an account, such
as a keeper bot, that the holder approved on each `positionTokenAccount` with the token program's
`approve`. Through this template the fees still go to the holder's accounts, and since a harvest
never spends from them, a delegate needs no other approval.

::: warning Approving a keeper hands it the positions
The approval isn't limited to this template. Outside it, the delegate can call Whirlpools'
`collect_fees` itself and send the fees to accounts of its own, or move the NFT, and with it the
position. Approve only a keeper you would trust with the positions themselves.
:::

A row that collects costs about 24,000 compute units, so eight fit the default limit of 200,000;
for more, add a compute-budget instruction that raises the limit. Rows that share tick arrays fit
about ten to a transaction; more need an address lookup table.

Whirlpools numbers its errors from 6000, as Ballista does, so a failed run's code alone can't say
which program refused; see [which program failed](/guide/errors-and-events#which-program-failed).
The TypeScript run's `describeFailure` reads the logs to tell.

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/orca_harvest_many_positions.rs` earns fees with real swaps
  through the pool, then harvests:
  - Four rows: fees in both tokens; fees in token B only, with the NFT held in a Token-2022
    account; out of range, with no fees; and no liquidity. The first two update and collect, the
    third only updates, and the fourth makes no call. The holder receives exactly the first two
    rows' fees. The run took 60,880 compute units and 747 bytes.
  - A row whose only fee equals `dustFloor` updates and leaves the fee owed, while a row above the
    floor collects.
- **Failures.** A stranger signing fails in Whirlpools with `MissingOrInvalidDelegate`, which the
  test tells apart from Ballista's own 6019 by the logs. A row from another pool fails with
  `ConstraintHasOne`, and a row from another holder at `positionBelongsToTheFeeOwner`; in both, the
  first row's collect reverts too. A stranger's account in both fee slots, or in token B's alone,
  fails at `positionBelongsToTheFeeOwner`.
- **A delegate.** A keeper approved on the NFT signs the run, and the holder's own accounts receive
  the fees.
- **Limits.** Eight earning rows land within the default 200,000 compute units, with no
  compute-budget instruction, using 192,418; nine run out. Ten rows sharing tick arrays fit one
  transaction with a compute-budget instruction; eleven don't.
- **Whirlpools alone.** `collect_fees` with nothing owed succeeds and moves nothing
  (`tests/protocols/tests/orca_setup.rs`).
- Every Whirlpools call passes the same accounts, in the same order and with the same signer and
  writable flags, as Orca's own Rust client (`tests/protocols/tests/orca_cpis.rs`).
- An opt-in test checks the Orca offsets against devnet accounts.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
