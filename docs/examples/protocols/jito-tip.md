# Pay a Jito tip only from profit

<p class="protocol-line">Jito · Jupiter</p>

**Status:** Compiles and passes the verifier; not yet run against Jupiter or Jito.

## What it does

Runs a Jupiter trade and pays a Jito tip only if the trade's profit covers it.

Jito recommends putting the tip in the same transaction as the trade, so that a failed trade pays
no tip. That covers failure, but not the more common case: a trade that succeeds but earns less
than the tip. A plain transaction can't compare its own profit with its own tip, so it pays the tip
in full either way.

The searcher is the account that runs the trade and pays the tip. The template records its balance
in lamports (the smallest unit of SOL), runs the trade, and requires the profit to cover the tip
plus `minimumEdge`. If the profit falls short, the run reverts before the tip is paid.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/jito-profit-guarded-tip.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#jito-tip [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/jito-tip.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#jito-tip [Rust · Run]

:::

Profit is measured in lamports, so this suits a trade that ends in SOL. For a trade that ends in a
token, compare the token account's balance (the `u64` at byte offset 64) instead.

The tip amount is an input, fixed before signing like any ordinary tip. A template could instead
compute the tip as a share of the profit: a tip is a plain SOL transfer, and Jito accepts one made
through a CPI (one program calling another). But Jito's block engine, which runs the tip auction,
is closed source, and public write-ups disagree on whether it ranks a tip computed during execution
by its simulated value or by an amount read from the instruction. A tip that is paid but ranked as
zero is worse than no tip, so this template bids a fixed amount and only decides whether to pay it.

## Run it

The trade is Jupiter's `route` instruction, which starts its account list with the token program
and the signer. The template passes those two itself; the rest of the route's accounts, including
its token accounts, arrive as the `strategyAccounts` [account group](/guide/account-groups).
`strategyData` is Jupiter's instruction data without its eight-byte discriminator; the template
adds the `route` discriminator itself.

The Run tabs pass the five declared accounts in order, then the inputs `strategyData`,
`tipLamports` and `minimumEdge`, then the group. `jitoTip` is one of Jito's eight tip accounts.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- The template compiles and passes Ballista's verifier.
- A test checks that it calls `route` with its accounts in `route`'s order
  (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Jupiter or Jito.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
