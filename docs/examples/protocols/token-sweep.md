# Sell a whole balance

<p class="protocol-line">Jupiter · SPL Token</p>

**Status:** Tested locally in LiteSVM against Jupiter and Raydium programs and accounts copied from
mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work in the measured run took 7,375
[compute units](/reference/glossary#compute-units), beyond what Jupiter's route used. Ballista
charges no fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Sells everything in a token account through Jupiter, whatever the balance turns out to be when the
transaction runs. Typical sources are a fee account, an airdrop claim, a vesting withdrawal or the
leftovers from an earlier swap. A Jupiter route normally sells a fixed amount, set when the route is
built, so a lower balance fails the route and a higher one leaves the difference behind.

Jupiter's `route` carries the amount to sell as `in_amount`, after the route plan and before the
quote. So your client passes the route plan and the quote's numbers separately, and the template
writes the instruction data itself. It:

- requires the seller to own both token accounts (`sweepsTheSellersOwnBalance`,
  `proceedsGoToTheSeller`), since a route's step can pay any account of the output mint;
- reads the balance and fails unless it is above `dustFloor` (`worthSelling`);
- passes that balance to Jupiter as `in_amount`, with the quoted output scaled to match
  (`quotedOutAmount × balance / quotedInAmount`);
- requires the route's `platformFeeBps` to be at most `MAX_PLATFORM_FEE_BPS`, a constant that is 0
  (`platformFeeWithinCap`), before Jupiter is called;
- requires the proceeds to be at least that scaled quote, less `slippageBps` (`saleMetTheQuote`);
- requires the account to hold no more than `dustFloor` afterwards (`nothingMeaningfulLeftBehind`).

It guards against the market moving after the quote. It does not guard against:

- **A bad quote.** Whoever builds the run supplies `quotedInAmount`, `quotedOutAmount` and
  `slippageBps`, which set the least the sale accepts.
- **Spending the seller's other token accounts.** The seller signs `route`, and Jupiter passes that
  authority to every step.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/token-sweep-into-swap.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#token-sweep [Rust · Template]

<<< @/../clients/js/examples/protocols/run/token-sweep.ts [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#token-sweep [Rust · Run]

:::

The route plan splits its input by percentage, so the same plan can sell more or less than it was
quoted for. Above the quote, the extra size's price impact has to fit within `slippageBps`, and the
swap has to stay within what the route's pool accounts cover. On a deep pool the slippage limit
comes first. Past either limit the sale fails inside Jupiter or the pool, so quote for roughly the
balance you expect.

Each token account's declaration requires the original SPL Token program to own it, so a
Token-2022 account is rejected. The owner checks above are different: they read the wallet stored
in the token account, its `owner` field at byte offset 32. The balance is its `amount`, at 64.

## Run it

`route` starts its account list with the token program, the signer, and the signer's source and
destination token accounts. The template passes those four itself; the rest of the route's
accounts arrive as the `routeAccounts` [account group](/guide/account-groups).

The Run tabs pass the five declared accounts, `jupiter`, `tokenProgram`, `seller`, `sourceAta` and
`destinationAta`, then the inputs `routePlan`, `quotedInAmount`, `quotedOutAmount`, `slippageBps`,
`platformFeeBps` and `dustFloor`, then the group. `splitJupiterRoute` (TypeScript) and
`RouteQuote::split` (Rust) split the Swap API's `route` data into the plan and the quote's numbers.

[Getting a Jupiter route](/examples/protocols/#jupiter-routes) says how to request the route from
Jupiter's Swap API and what to keep from its response.

## What has been tested

- **Against the real programs.** `tests/protocols/tests/token_sweep.rs` sells USDC for SOL through
  Jupiter and Raydium's SOL/USDC pool, in place of `route` in the transaction Jupiter's API built.
  With the route quoted for 150 USDC, balances 3% over, 3% under and ten times that each sold in
  full and met the scaled quote. The run added 52 bytes to Jupiter's transaction.
- **Failures.** Each of these fails, and the whole transaction reverts: a balance at `dustFloor`
  (`worthSelling`); another wallet's source or an attacker's destination
  (`sweepsTheSellersOwnBalance`, `proceedsGoToTheSeller`, before Jupiter is called); a route that
  pays the attacker while the seller's account is measured (`saleMetTheQuote`); a balance 10,000
  times the quote (inside Raydium); a route that charges a platform fee (`platformFeeWithinCap`,
  before Jupiter is called).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
