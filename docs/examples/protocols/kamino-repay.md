# Repay what the swap produced

<p class="protocol-line">Kamino · Jupiter</p>

**Status:** Run as real transactions against Jupiter, a Meteora pool and Kamino, copied from
mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

## What it does

Sells collateral through Jupiter and repays a Kamino loan with what the sale produced, in one
transaction.

Deleveraging means selling collateral and repaying a loan with the proceeds. Two numbers are
unknown when you sign: how much the swap returns, and how much you owe by the time the transaction
runs. Interest keeps accruing, so a debt figure fetched by the client is already out of date when
the transaction lands.

The template handles the first number. It:

- requires `borrowedAssetAta`, where the swap pays out, to belong to the borrower
  (`swapPaysTheBorrower`). Kamino repays from any account the borrower may spend, so without this
  a run could leave the rest of the swap in someone else's account;
- requires the route's `platformFeeBps` to be at most `MAX_PLATFORM_FEE_BPS`, a constant that is 0
  (`platformFeeWithinCap`);
- swaps, measures how much of the borrowed token arrived, and requires at least
  `minimumRepayment` (`swapWorthRepaying`);
- repays exactly that amount against the borrower's obligation, Kamino's account of their deposits
  and debts, with `repay_obligation_liquidity_v2`.

It does not read the debt. Kamino repays at most what is owed, and the rest stays in
`borrowedAssetAta`.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/kamino-repay-swap-output.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#kamino-repay [Rust · Template]

<<< @/../clients/js/examples/protocols/run/kamino-repay.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#kamino-repay [Rust · Run]

:::

## Run it

`route` starts its account list with the token program, the signer, and the signer's source and
destination token accounts: here `collateralAta` and `borrowedAssetAta`. The template passes those
four itself; the rest of the route's accounts arrive as the `routeAccounts`
[account group](/guide/account-groups).

Kamino's v2 repayment takes 13 accounts: 9 the template passes, then a second group,
`farmAccounts`, of four. They are the obligation's user state in the reserve's debt farm, that
farm, the lending market authority and Kamino's Farms program. A reserve is Kamino's pool for one
token, and a farm is a Kamino Farms rewards pool attached to it. For a reserve without a debt farm,
as on the main market's SOL and USDC reserves, both farm slots hold the Kamino program, read-only.

The Run tabs pass the 12 declared accounts in order (`jupiter`, `kamino`, `tokenProgram`,
`instructionsSysvar`, `borrower`, `collateralAta`, `borrowedAssetAta`, then Kamino's five from
`obligation` to `reserveLiquiditySupply`), then the inputs `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps`, `platformFeeBps` and
`minimumRepayment`, then `routeAccounts` and `farmAccounts`.

Before the run, **refresh Kamino in the same transaction:** `refresh_reserve` for each reserve the
obligation holds, then `refresh_obligation`. A refresh brings a reserve's interest and price, or an
obligation's values, up to date. Kamino repays only against a reserve and an obligation refreshed
in the same slot, and the template doesn't refresh. `buildKaminoRefreshes` (TypeScript) and
`kamino_refreshes` (Rust), next to the runs, build them.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- **Against the real programs.** `tests/protocols/tests/kamino_repay_swap_output.rs` sells 1 SOL
  through Jupiter and a Meteora pool, behind Kamino's refreshes, for a borrower owing three times
  the route's quote. The debt falls by exactly what the swap produced. The whole transaction took
  141,830 compute units (Solana's measure of execution cost) and 1,006 bytes, using the route's
  lookup table.
- **Failures.** A `minimumRepayment` one unit above the output fails at `swapWorthRepaying`, swap
  included. An attacker's account, approved for the borrower, as `borrowedAssetAta` fails at
  `swapPaysTheBorrower`, before Jupiter is called. A route that charges a platform fee fails at
  `platformFeeWithinCap`, before Jupiter is called.
- **Not tested.** Mainnet itself, a reserve with a debt farm, and Token-2022 tokens: the template
  accepts SPL Token accounts only. Only `borrowedAssetAta` is tied to the borrower; the route's
  accounts and the inputs are the run builder's choice, apart from the capped platform fee.
- A test reads the template and checks that it calls `route` with its accounts in `route`'s order,
  and repays through Kamino's v2 handler exactly what the swap produced, with `farmAccounts` as its
  group (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares, right after Kamino's refreshes
  (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
