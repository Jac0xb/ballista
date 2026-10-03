# Pay a Jito tip only from profit

<p class="protocol-line">Jito · Jupiter</p>

**Status:** Run as real transactions against Jupiter, a Meteora pool, a Raydium pool and a Jito tip
account, copied from mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on
mainnet itself, or through Jito's tip auction.

## What it does

Runs a Jupiter trade and pays a Jito tip only if the trade's profit covers it.

Jito recommends putting the tip in the same transaction as the trade, so that a failed trade pays
no tip. But a trade that succeeds and earns less than the tip still pays it in full: a plain
transaction can't compare its own profit with its own tip.

The trade is a round trip from SOL back to SOL. Jupiter's `route` moves only token accounts, so it
trades wrapped SOL (wSOL), SOL held in a token account; the Swap API wraps SOL before `route` and
unwraps it after. A wSOL balance is in lamports (billionths of a SOL), the same unit as the tip.
The searcher is the wallet that signs, trades and pays the tip. The template:

1. requires `wsolAccount` to hold wSOL and to belong to the searcher;
2. records its balance, requires the route's `platformFeeBps` to be at most
   `MAX_PLATFORM_FEE_BPS`, a constant that is 0 (`platformFeeWithinCap`), then runs the round trip as one Jupiter `route` from that account back to
   it;
3. requires the balance to have grown by at least `tipLamports` plus `minimumEdge`, so a loss or a
   thin profit reverts before any tip is paid;
4. pays `tipLamports` from the searcher to `jitoTip`, which must be owned by Jito's Tip Payment
   program, as Jito's tip accounts are.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/jito-profit-guarded-tip.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#jito-tip [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/jito-tip.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#jito-tip [Rust · Run]

:::

The tip is a fixed input, not a share of the profit computed during the run. Jito's block engine,
which runs the tip auction, is closed source, and it is unclear whether it ranks a computed tip by
its simulated value or by an amount read from the instruction. A tip paid but ranked as zero would
be worse than none, so the template bids a fixed amount and only decides whether to pay it.

## Run it

`route` starts its account list with the token program, the signer, and the source and destination
token accounts. The template passes those four itself, with `wsolAccount` as both source and
destination; the rest of the route's accounts arrive as the `strategyAccounts`
[account group](/guide/account-groups).

Jupiter's Swap API won't quote a route from a token back to itself, so quote two legs, SOL to USDC
and back, and join them into one `route` as the Run tabs' comments describe. Only single-step legs
can be joined, and the SDK has no helper for it: `round_trip` in `tests/protocols/tests/jito_tip.rs`
is the only implementation.

The Run tabs pass the six declared accounts, `systemProgram`, `strategyProgram`, `tokenProgram`,
`searcher`, `wsolAccount` and `jitoTip`, then the inputs `routePlan`, `inAmount`, `quotedOutAmount`,
`slippageBps`, `platformFeeBps`, `tipLamports` and `minimumEdge`, then the group. The joined `route`
data is split into the first five.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- **Against the real programs.** `tests/protocols/tests/jito_tip.rs` runs the round trip through
  Jupiter, 1 SOL to USDC on Meteora and back to SOL on Raydium, with the run in place of `route` in
  the transaction Jupiter's API built for the first leg. At the copied prices the round trip loses
  to the pools' fees, so a test wallet first sells 500 SOL into the Raydium pool, and the round trip
  buys its SOL back cheaper. A tip of that profit less `minimumEdge` is then paid in full.
- **Failures.** One lamport more fails at `profitCoversTheTip` and pays nothing, and so does a
  1,000-lamport tip on the losing round trip. A USDC account as `wsolAccount` fails at
  `wsolAccountHoldsWrappedSol`, another wallet's wSOL at `searcherOwnsTheWsolAccount`, and a plain
  wallet as `jitoTip` at its owner constraint. A route that charges a platform fee fails at
  `platformFeeWithinCap`, before Jupiter is called.
- **Jupiter's side.** Sent to Jupiter directly, a profitable round trip grows the searcher's wSOL
  and leaves its lamports alone, apart from the fee.
- **Not tested.** Jito's block engine isn't part of LiteSVM, so the tests show the tip is paid, not
  how the auction ranks it.
- A test reads the template and checks that it calls `route` with the token program, the searcher
  and `wsolAccount` twice, and that the profit check reads the wSOL balance, not lamports
  (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
