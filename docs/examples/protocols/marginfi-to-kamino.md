# Move a position into Kamino

<p class="protocol-line">marginfi · Kamino</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against marginfi and
Kamino programs and accounts copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 6,438 of the tested transaction's 162,666
[compute units](/reference/glossary#compute-units); the protocols took the rest. Ballista charges no
fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Withdraws a whole position from marginfi and deposits exactly what came out into Kamino, in one
transaction.

Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount)` needs the
amount up front, but the amount only exists once the marginfi withdrawal has run. A plain
transaction has to guess it. Guess too high and the deposit fails, taking the withdrawal with it.
Guess too low and the rest sits in the wallet instead of in Kamino.

The template records the balance of `walletAta`, withdraws everything from marginfi into it, and
measures how much arrived. If that is less than `minimumMoved`, the run reverts
(`worthRebalancing`). Otherwise it deposits exactly that amount into the owner's obligation, the
Kamino account that records their deposits and debts.

- Kamino mints whole cTokens (its deposit receipts) only, so less than one cToken's worth, a base
  unit or so, can stay in `walletAta`.
- The template doesn't check who owns `walletAta`, but Kamino's deposit requires `owner` to own
  it, so a run that names another account fails as a whole.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/marginfi-to-kamino-rebalance.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#marginfi-to-kamino [Rust · Template]

<<< @/../clients/js/examples/protocols/run/marginfi-to-kamino.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#marginfi-to-kamino [Rust · Run]

:::

The Rust tabs' `program`, `anchor` and account flags are
[shared helpers](/examples/protocols/#rust-helpers).

## Run it

marginfi's withdrawal takes its eight accounts, then the `healthAccounts`
[account group](/guide/account-groups), filled as for
[Withdraw everything, with a minimum](/examples/protocols/marginfi-withdraw#run-it). It is empty when
the withdrawn balance was the account's only one.

Kamino's v2 deposit takes 17 accounts: 14 the template passes, then a second group,
`farmAccounts`, of three. They are the obligation's user state in the reserve's collateral farm,
that farm, and Kamino's Farms program. A reserve is Kamino's pool for one token, and a farm is a
Kamino Farms rewards pool attached to it. For a reserve without one, both farm slots hold the
Kamino program, read-only.

The Run tabs pass the 19 declared accounts in order (`marginfi`, `kamino`, `tokenProgram`,
`instructionsSysvar`, `owner`, `walletAta`, marginfi's five from `marginfiGroup` to
`marginfiVaultAuthority`, then Kamino's eight from `obligation` to
`reserveDestinationDepositCollateral`), then the input `minimumMoved`, then `healthAccounts` and
`farmAccounts`.

Before the run:

- **Refresh Kamino in the same transaction.** Kamino deposits only into an obligation refreshed in
  the same slot, and the template doesn't refresh; see
  [Refreshing Kamino](/examples/protocols/#kamino-refreshes).
- **Create the farm user state once,** with `init_obligation_farms_for_reserve`, before an
  obligation's first deposit into a reserve with a collateral farm. The main market's SOL and USDC
  reserves both have one.

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/marginfi_to_kamino_rebalance.rs` moves a 100 USDC marginfi
  balance, the account's only one, into Kamino's USDC reserve, which has a collateral farm, behind
  Kamino's refreshes. Kamino was asked for exactly what marginfi released and kept back no more than
  its cToken rounding, and the marginfi balance closed. The whole transaction took 162,666 compute
  units and 1,009 bytes.
- **Failures.** A `minimumMoved` one unit above the deposit fails at `worthRebalancing`, with
  nothing moved.
- **Not tested.** Mainnet itself, a marginfi account with other balances, a reserve without a
  collateral farm, a `walletAta` that isn't `owner`'s, and Token-2022 tokens: the template accepts
  SPL Token accounts only. The input is the run builder's choice.
- A test reads the template and checks that the deposit is v2, of exactly the amount it measured,
  with `farmAccounts` as its group, and that the withdrawal passes `healthAccounts` after its eight
  accounts (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares, right after Kamino's refreshes
  (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
