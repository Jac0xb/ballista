# Deposit exactly what a swap produced

<p class="protocol-line">Jupiter · Kamino</p>

**Status:** Run as real transactions against Jupiter, a Meteora pool and Kamino, copied from
mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

## What it does

Swaps through Jupiter and deposits exactly what the swap produced into Kamino, in one transaction.

Jupiter's `route` instruction carries the amount in, the *quoted* amount out, `slippageBps` and
`platformFeeBps`. Jupiter reports the amount that actually came out only as an event, which it
emits by making a CPI (a call from one program to another) to itself. An event isn't return data,
so a calling program can't read it with `get_return_data`. The only reliable source is the
destination token account, after the swap has run.

Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2` needs that number as its
`liquidity_amount`. A plain transaction has to fix it before the swap runs. Guess too high and the
deposit fails; guess too low and the rest stays in your token account.

The template requires the route's `platformFeeBps` to be at most `MAX_PLATFORM_FEE_BPS`, a
constant that is 0 (`platformFeeWithinCap`), before the swap. It reads the destination balance
before and after the swap, requires the difference to
be at least `minimumOut` (`swapMetItsFloor`), and asks Kamino to deposit exactly that difference.
Kamino mints whole cTokens, its receipts for a deposit, and takes only what they are worth, so less
than one cToken's worth (a base unit or so) can stay behind.

It does not guard against:

- **A bad route.** The route arrives as `routePlan`, `inAmount`, `quotedOutAmount` and `slippageBps`, so what
  it sells and its quote are up to whoever builds the run. Set `minimumOut` from your own quote.
- **Spending the owner's other token accounts.** The owner signs `route`, and Jupiter passes that
  authority to every step.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/jupiter-deposit-exact-output.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#jupiter-deposit [Rust · Template]

<<< @/../clients/js/examples/protocols/run/jupiter-deposit.ts [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#jupiter-deposit [Rust · Run]

:::

## Run it

`route` starts its account list with the token program, the signing owner, and the owner's source
and destination token accounts. The template passes those four itself, and the deposit draws from
the destination it measured. The rest of Jupiter's list changes from route to route, so it arrives
as the `routeAccounts` [account group](/guide/account-groups): a list of any length that the caller
supplies at run time. Group members keep the writable flag (permission to be modified) that the
transaction gave them, and are never passed as signers.

Kamino's v2 deposit takes 17 accounts: 14 the template passes, then a second group,
`farmAccounts`, of three. They are the obligation's user state in the reserve's collateral farm,
that farm, and Kamino's Farms program. For a reserve without a farm, both farm slots hold the
Kamino program, read-only.

The Run tabs pass the 15 declared accounts in order (`jupiter`, `kamino`, `tokenProgram`,
`instructionsSysvar`, `owner`, `sourceAta`, `destinationAta`, then Kamino's eight from `obligation`
to `reserveDestinationDepositCollateral`), then the inputs `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps`, `platformFeeBps` and
`minimumOut`, then `routeAccounts` and `farmAccounts`.

Before the run:

- **Refresh Kamino in the same transaction:** `refresh_reserve` for each reserve the obligation
  holds, then `refresh_obligation`. Kamino deposits only into an obligation refreshed in the same
  slot, and the template doesn't refresh. `buildKaminoRefreshes` (TypeScript) and
  `kamino_refreshes` (Rust), next to the runs, build them.
- **Create the farm user state once,** with `init_obligation_farms_for_reserve`, before an
  obligation's first deposit into a reserve with a collateral farm.

A fuller TypeScript runner, `clients/js/examples/protocols/run-jupiter-deposit.ts`, takes the Swap
API's response directly: it refuses data that isn't `route`, checks that the account list starts
with the four accounts above, forwards the rest as the group, and fills `farmAccounts`.

[Getting a Jupiter route](/examples/protocols/#jupiter-routes) says how to request the route from
Jupiter's Swap API and what to keep from its response.

## What has been tested

- **Against the real programs.** `tests/protocols/tests/jupiter_deposit_exact_output.rs` sells 1 SOL
  for USDC through Jupiter and a Meteora pool, then deposits into Kamino's USDC reserve, which has
  a collateral farm. Of the 121,391,105 USDC units the swap produced, Kamino took all but 1, its
  cToken rounding. The whole transaction, refreshes included, took 151,372 compute units (Solana's
  measure of execution cost) and 1,085 bytes.
- **Failures.** A `minimumOut` one unit above the fill fails at `swapMetItsFloor`, and nothing is
  deposited. Kamino refuses a deposit with no refresh in its slot (`ObligationStale`), and one into
  a farmed reserve without the farm accounts (`FarmAccountsMissing`, in
  `tests/protocols/tests/kamino_contract.rs`). A route that charges a platform fee fails at
  `platformFeeWithinCap`, before Jupiter is called.
- A test reads the template and checks that it calls `route` with its accounts in `route`'s order,
  that the deposit is v2 with `farmAccounts` as its group, and that the runner above forwards only
  what follows those four accounts (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
