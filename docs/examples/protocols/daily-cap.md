# Cap a caller's daily swaps

<p class="protocol-line">Jupiter · Registry</p>

**Status:** Run as real transactions against Jupiter, a Meteora pool and a Raydium pool, copied from
mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

## What it does

Sells SOL through Jupiter, and limits each caller to 1.728 SOL at once, refilling daily. A caller
who spends it all can spend it again as it refills, so about 3.456 SOL can move in any 24 hours.

A daily limit has to remember earlier sales, and a run's values are gone when it ends. So the
template keeps each caller's total in a [registry](/guide/registries) entry: an account that
Ballista owns and only this template's runs can change. Each caller has their own entry, picked by
their address, and must sign, so no caller can use another's.

The entry holds `spent`, in lamports (billionths of a SOL), and `lastSpend`, the time of the last
sale. The [`rateLimit`](/reference/language#registries) helper lowers `spent` by 20,000 lamports
for each second since `lastSpend`, but not below zero. It then adds `inAmount`, the amount the
route sells, and fails at `withinRateLimit` if the total is over the cap of 1,728,000,000. At that
rate the cap refills in a day, continuously rather than at midnight. The cap and the rate are
constants, so the caller can't change them.

Because the cap counts lamports, the route must sell wrapped SOL (wSOL), SOL held in a token
account. The Swap API wraps SOL before `route` and unwraps it after. The template:

1. requires `sourceAta`, the route's source token account, to hold wSOL (`spendsWrappedSol`).
   `inAmount` is in the smallest units of whatever the route sells, so otherwise 150 USDC would
   count as 0.15 SOL;
2. requires `sourceAta` to belong to the caller (`sourceBelongsToTheCaller`);
3. charges `inAmount` to the entry, or fails at `withinRateLimit`;
4. requires the route's `platformFeeBps` to be at most `MAX_PLATFORM_FEE_BPS`, a constant that is 0
   (`platformFeeWithinCap`);
5. calls `route` with that same `inAmount`;
6. requires exactly `inAmount` to have left `sourceAta` (`soldWhatTheCapCharged`), since Jupiter
   doesn't require its steps to move the source account it is given. Without this check, a second
   wSOL account of the caller's at `sourceAta` passed the first two checks while the route sold
   150 USDC, charged as 0.15 SOL.

It does not guard against:

- **Spending the caller's other token accounts.** The caller signs `route`, and Jupiter passes that
  authority to every step, so a further step could spend another of the caller's token accounts
  without the cap counting it.
- **Swaps outside this template.** The cap counts only this template's runs. The caller can still
  swap through Jupiter directly.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-daily-cap-swap.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#jupiter-daily-cap [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/jupiter-daily-cap.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#jupiter-daily-cap [Rust · Run]

:::

The Rust template writes out the steps that `rateLimit` returns.

## Run it

`route` starts its account list with the token program, the signer, and the signer's source and
destination token accounts. The template passes the first three itself; the rest of the route's
accounts, from the destination token account on, arrive as the `actionAccounts`
[account group](/guide/account-groups).

The Run tabs pass the six declared accounts, `actionProgram` (Jupiter), `tokenProgram`, `actor`,
`sourceAta` (the actor's wSOL account), `spend` (the actor's entry) and `systemProgram`, then the
inputs `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`, then the
group. `splitJupiterRoute` (TypeScript) and `RouteQuote::split` (Rust) split the Swap API's `route`
data into `routePlan` and the four numbers after it. `findRegistryEntryAddress` (TypeScript) and
`find_registry_entry_address` (Rust) find the entry from the template's address, registry index 0
and the actor's address.

The actor's first run creates the entry, with no separate instruction, and the actor pays its
[rent](/reference/glossary#rent), the lamports Solana requires an account to hold for its size:
1,097,280 lamports, about 0.0011 SOL, on mainnet, for this 88-byte entry. Nothing closes an entry,
so the rent isn't returned.

[Getting a Jupiter route](/examples/protocols/#jupiter-routes) says how to request the route from
Jupiter's Swap API and what to keep from its response.

## What has been tested

- **Against the real programs.** `tests/protocols/tests/jupiter_daily_cap.rs` sells 1 SOL for USDC
  through Jupiter and a Meteora pool, in place of `route` in the transaction Jupiter's API built.
  The first run creates the caller's entry and buys the same USDC as that transaction alone. A
  second sale fails at `withinRateLimit` before Jupiter is called, and still fails 13,599 seconds
  later. At 13,600 seconds it lands, and `spent` ends exactly at the cap. Another caller, with the
  first's limit spent, still sells, on an entry of their own.
- **Cost.** The transaction took 82,728 compute units (Solana's measure of execution cost) when it
  created the entry, and 71,942 when the entry already existed. Ballista's own share, its run less
  Jupiter's `route`, was 9,644 and 7,909. The transaction is 807 bytes, 109 more than Jupiter's
  own, and the template 1,014 bytes.
- **Failures.** Each of these fails, and the whole transaction reverts, so no entry is created or
  changed: a route selling 150 USDC through a Raydium pool (`spendsWrappedSol`, before Jupiter is
  called); the same route with a second wSOL account of the caller's at `sourceAta`
  (`soldWhatTheCapCharged`, after the route); a caller passing another caller's entry, before or
  after it exists (`InvalidRegistryEntry`, before the first step); a route that charges a platform
  fee (`platformFeeWithinCap`, before Jupiter is called).
- A test reads the template and checks `route`'s accounts, that no input but `inAmount` reaches
  the limit, the order of the checks, and that the signing actor keys and pays for the entry
  (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- **Not tested.** Neither gap above has a test.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
