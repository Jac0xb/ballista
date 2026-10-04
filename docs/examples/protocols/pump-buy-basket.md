# Buy a basket within a budget

<p class="protocol-line">pump.fun</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against pump.fun's
bonding-curve program and three live curves copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 13,226 of the tested three-coin transaction's 255,496
[compute units](/reference/glossary#compute-units); pump.fun took the rest. Ballista charges no
fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Buys up to seven pump.fun coins in one transaction, each on its bonding curve, and fails the whole
transaction if together they cost more than `budget`.

pump.fun's `buy` caps one coin's cost with its `max_sol_cost`. Nothing caps the total. Each
coin's price moves with every trade that lands before yours, so the only total a plain transaction
can promise is the sum of every buy's worst case. The template measures what each buy actually
took from the buyer while the transaction runs, and keeps a running total.

For each coin, the template:

1. requires the coin's token account to belong to the buyer (`tokensGoToTheBuyer`). pump.fun
   checks only the account's mint, so a run built by someone else could otherwise send the coins
   anywhere;
2. requires the coin's curve not to have graduated (`curveNotGraduated`), by its `complete` flag.
   A graduated coin trades on PumpSwap, and pump.fun would refuse the buy with its own
   `BondingCurveComplete` (6005);
3. buys `amount` base units for at most `maxSolCost` lamports, which pump.fun enforces;
4. adds what the buyer's balance dropped by to the total: the price, pump.fun's and the creator's
   fees, and any rent pump.fun charged;
5. requires the total to be at most `budget` (`withinBudget`).

Any failure reverts the whole basket. A graduated coin fails the run rather than being skipped, so
a basket that lands holds every coin you asked for.

It does not guard against **another set of coins**. The rows name the mints, so a run built by
someone else could buy other coins within your budget.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/pump-fun-buy-basket.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#pump-buy-basket [Rust · Template]

<<< @/../clients/js/examples/protocols/run/pump-buy-basket.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#pump-buy-basket [Rust · Run]

:::

`complete` is the byte at offset 48 of a `BondingCurve`, after Anchor's discriminator and five
`u64`s. The owner pin on `bondingCurve` means pump.fun wrote it, and pump.fun's own seeds check
ties it to the row's mint. The Rust template takes its program addresses, offsets and
`token_2022_account()` from the [shared helpers](/examples/protocols/#rust-helpers).

## Run it

The coins form a [batch](/guide/batching). The Run tabs pass the 11 declared accounts in order,
`pumpProgram`, `global`, `buyer`, `systemProgram`, `tokenProgram`, `eventAuthority`,
`globalVolumeAccumulator`, `userVolumeAccumulator`, `feeConfig`, `feeProgram` and
`buybackFeeRecipient`, then one row of seven accounts per coin. The inputs are `budget`, then each
row's `amount` and `maxSolCost`. A row is:

- `mint`, `bondingCurve` and `curveTokenAccount`, the curve's own token account;
- `buyerTokenAccount`, the buyer's associated token account, which must already exist;
- `creatorVault`, derived from the creator the curve records;
- `bondingCurveV2`, a PDA pump.fun has taken since its April 2026 upgrade, whether or not it exists;
- `feeRecipient`: an ordinary fee recipient, or a reserved one for a mayhem-mode coin.

:::: details Deriving pump.fun's accounts
::: code-group

<<< @/../clients/js/examples/protocols/run/pump-fun.ts#pump-accounts [TypeScript]

<<< @/../clients/rust/examples/protocol_templates_run.rs#pump-accounts [Rust]

:::
::::

Two coins fit a plain transaction. Three limits set how many more fit:

- **Compute.** A buy costs pump.fun about 75,000 compute units, so more than two coins need a
  compute-budget instruction first.
- **Transaction size.** A coin whose accounts are not in a lookup table adds 215 bytes, so two fit
  without one. With pump.fun's shared accounts in a table, four fit. With each coin's accounts in
  one too, all seven fit.
- **Calls.** Each buy makes eight calls: pump.fun, its fee program, a token transfer, four SOL
  transfers and its event log. A transaction holds at most 64, so the template stops at seven
  coins.

The template pins Token-2022, which mints every coin pump.fun creates with `create_v2`. Older coins
use SPL Token and need that pin changed. It calls pump.fun's `buy`, which trades SOL-paired coins
only. `buy_v2` also trades coins paired with USDC, but it takes about ten accounts per coin, more
than a batch row holds.

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/pump_fun_buy_basket.rs` buys three coins in one run: two
  ordinary coins and one in mayhem mode. Every coin arrived in full, and the buyer paid exactly
  what the same three buys cost through pump.fun alone: 160,822,897 lamports, including the rent
  for the buyer's volume record. With pump.fun's shared accounts in a lookup table, the run took
  255,496 compute units and 924 bytes.
- **The budget.** A budget equal to the total lands. One lamport less fails at `withinBudget`, and
  every buy reverts. A budget that covers only the first coin stops the run at the second.
- **Limits.** Two coins land in a plain transaction, in 180,958 compute units and 1,004 bytes.
  Seven land with every account in lookup tables, in 590,735 compute units and 492 bytes, and their
  calls fill 60 of the transaction's 64.
- **Failures.** A graduated coin fails at `curveNotGraduated`, and the coin before it reverts;
  pump.fun alone refuses the same buy with `BondingCurveComplete`. A stranger's token account fails
  at `tokensGoToTheBuyer`, though pump.fun alone pays the stranger. The buyer's account of another
  coin fails in pump.fun with Anchor's `ConstraintTokenMint` (2014). A `maxSolCost` one lamport
  short fails in pump.fun with `TooMuchSolRequired` (6002).
- **Accounts.** The semantics test checks `buy`'s accounts and writable flags against a mainnet
  transaction, and the run helpers' derived addresses against mainnet's.

::: warning Check before you upload
pump.fun changes often: its April 2026 upgrade added two accounts to every `buy` and `sell`. Check
the accounts against pump.fun's current IDL and a recent transaction before you upload.
:::

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
