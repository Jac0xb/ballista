# Protocol templates

Thirteen example templates. Twelve work with real Solana protocols: Jupiter, Kamino, marginfi,
Orca, Pyth and Jito, and one of those, the daily cap, also keeps state between runs. The thirteenth
settles a trade at a price someone signed off chain. Each one works with a value that only exists
while the transaction runs, such as what a swap returned or what a position has earned. The source
files are in `clients/js/examples/protocols/`.

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against copies of the
protocols' mainnet programs and accounts; not yet run on devnet or mainnet.
[What has been tested](#what-has-been-tested) has the details. Treat the templates as starting
points, and check them against the protocols' current programs before you use them.

| Template | Protocol | Decided during the run |
| --- | --- | --- |
| [Deposit what a swap produced](/examples/protocols/jupiter-deposit) | Jupiter → Kamino | How much the swap produced |
| [Swap checked against an oracle](/examples/protocols/jupiter-oracle-swap) | Jupiter + Pyth | Whether the swap paid at least the oracle price, less a tolerance |
| [Sell a whole balance](/examples/protocols/token-sweep) | SPL Token → Jupiter | How much there is to sell |
| [Cap a caller's daily swaps](/examples/protocols/daily-cap) | Jupiter + registry | How much wrapped SOL this caller can sell now, as its cap refills |
| [Repay what a swap produced](/examples/protocols/kamino-repay) | Jupiter → Kamino | How much the swap produced |
| [Liquidate with a minimum payout](/examples/protocols/kamino-liquidate) | Kamino | How much collateral the liquidator received |
| [Withdraw everything, with a minimum](/examples/protocols/marginfi-withdraw) | marginfi | How much the withdrawal returned |
| [Move a position into Kamino](/examples/protocols/marginfi-to-kamino) | marginfi → Kamino | How much marginfi released, to deposit in Kamino |
| [Compound collected fees](/examples/protocols/orca-compound) | Orca | How much the position had earned |
| [Harvest positions that earned](/examples/protocols/orca-harvest) | Orca | Which positions have earned enough to collect |
| [Act only on a fresh price](/examples/protocols/pyth-gate) | Pyth → Jupiter | Whether the price is recent, precise and in range |
| [Tip only from profit](/examples/protocols/jito-tip) | Jupiter → Jito | Whether the trade's profit covered the tip |
| [Settle at a signed quote](/examples/protocols/signed-quote) | Ed25519 → SPL Token | Whether the maker signed this quote for this taker, and it hasn't expired |

## What has been tested

- **Compiling and verifying.** CI compiles every template
  (`clients/js/src/protocol-examples.test.ts`) and checks the result with the same verifier the
  Ballista program runs before it stores a template (`common/src/template/verify.rs`).
- **Rust.** Each Rust template is byte-identical to the TypeScript one, and each Rust run passes
  the accounts, flags and inputs its template declares (`clients/rust/tests/protocol_templates.rs`).
- **Running against copies of the protocols.** The twelve protocol templates run as signed
  transactions in LiteSVM, a local Solana runtime, against the protocols' programs and accounts
  copied from mainnet at a single slot. The tests never touch the network (`tests/protocols/`). The
  signed quote runs the same way against a copy of mainnet's Token program, with the Ed25519
  precompile, and also in Mollusk, a harness that runs Solana programs without a validator
  (`tests/ballista/`). None has run on devnet or mainnet. Each page's "What has been tested" says
  what its runs showed, and its Cost line how much of the measured transaction was Ballista's own
  work.
- **Account offsets.** The Orca, Pyth and SPL Token offsets are also checked against real devnet
  accounts by an opt-in test; see [reading offsets](#reading-offsets-from-an-account). The Kamino
  and marginfi templates don't read those protocols' accounts. They read SPL token accounts: their
  balances before and after each call and, where it matters, who owns them.
- **Jupiter calls.** The seven templates in the table that call Jupiter use its `route` instruction.
  Each sends the `route` discriminator and passes the first accounts of `route` itself, in the order
  Jupiter's published interface lists them: the token program, the signer and, where the template
  measures them, the source and destination token accounts. The rest of the route's accounts
  arrive as an account group. A test that reads the templates checks this
  (`clients/js/src/protocol-semantics.test.ts`), and the protocol tests send real routes, recorded
  from Jupiter's API, through Jupiter's own program. Each also caps the route's platform fee at
  `MAX_PLATFORM_FEE_BPS`, 0 unless its author raises it. Request routes from Jupiter's Swap API with `useSharedAccounts: false`; the default,
  `shared_accounts_route`, is a different instruction with its accounts in a different order.

## Reading offsets from an account

Some templates read a protocol account's data at a fixed byte position, called an offset. Nothing
at build time can check an offset. A wrong one doesn't cause an error: it reads a believable
number from the wrong field.

`pnpm test:devnet` (`clients/js/src/devnet-offsets.test.ts`) reads real Orca, Pyth and SPL Token
accounts on devnet and checks that the values at these offsets make sense: a plausible Unix
timestamp, a price exponent between −18 and 0, tick bounds in the right order, and a token balance
that matches the one the RPC reports. It needs no keypair and no SOL. It is opt-in, and CI does not
run it.

Pyth's `PriceUpdateV2` account needs extra care. Near the start it stores a verification level.
`Full` takes one byte and `Partial` takes two, so in a `Full` account every later field sits one
byte earlier. Offsets worked out from the struct's `LEN` constant match the `Partial` layout. The
templates read the verification level first and require `Full`. That fixes the layout, and `Full`
is also the stronger guarantee: the update carries all the required signatures, not just some.

| Layout | Field | Offset |
| --- | --- | ---: |
| Pyth `PriceUpdateV2`, `Full` | `verification_level` `u8` | 40 |
| | `feed_id` (32 bytes) | 41 |
| | `price` `i64` | 73 |
| | `conf` `u64` | 81 |
| | `exponent` `i32` | 89 |
| | `publish_time` `i64` | 93 |
| Orca `Position` | `liquidity` `u128` | 72 |
| | `fee_owed_a` `u64` | 112 |
| | `fee_owed_b` `u64` | 136 |
| SPL Token account | `mint` | 0 |
| | `owner` | 32 |
| | `amount` `u64` | 64 |

Pyth and Orca are Anchor programs (Anchor is the most common Solana program framework). Anchor
starts each account's data with an eight-byte discriminator, a tag that identifies the account
type, and the Pyth and Orca offsets above count those eight bytes. Anchor instructions start with a
discriminator too. The templates compute discriminators instead of copying them: the first eight
bytes of `sha256("global:<handler>")` for an instruction and of `sha256("account:<Name>")` for an
account.

::: warning Check before you upload
The tests use a copy of mainnet from one slot. They can't tell you whether a protocol has changed
since. Before you upload one of these templates, check each call's accounts, arguments and offsets
against the protocol's current IDL (its published interface description). Require every account a
template reads to be owned by the program you expect and long enough for the fields you read. If a
template measures a token account, also require that account's `owner` field to be the signer.
Prefer fields the protocol documents as public.
:::

## Refreshing Kamino {#kamino-refreshes}

Kamino's v2 deposit, repayment and liquidation work only against reserves and an obligation
refreshed in the same slot, and no template refreshes them. A reserve is Kamino's pool for one
token, and an obligation is a borrower's record of deposits and debts. Put the refreshes before the
run, in the same transaction:

1. `refresh_reserve` for each reserve the obligation holds, deposits then borrows, in the order the
   obligation lists them. Each names the reserve's Scope price account last. The main market prices
   by Scope alone, so the Pyth and Switchboard slots before it hold the Kamino program, which Kamino
   reads as "none".
2. `refresh_obligation`, with the same reserves in the same order.

`buildKaminoRefreshes` and `kamino_refreshes` build both. Beside them, `kaminoFarmPair` and
`kamino_farm_pair` fill a farm's two slots in a v2 instruction's `farmAccounts`, with the Kamino
program standing in for a farm the reserve doesn't have.

:::: details The refresh helpers
::: code-group

<<< @/../clients/js/examples/protocols/run/kamino.ts#kamino-refreshes [TypeScript]

<<< @/../clients/rust/examples/protocol_templates_run.rs#kamino-refreshes [Rust]

:::
::::

## Rust helpers {#rust-helpers}

The Rust templates share these helpers and constants: the account flags, the program addresses and
account offsets, `anchor` for an Anchor instruction's discriminator, `program` to declare a pinned
program, and the token-account declarations and checks.

::: details The Rust helpers

<<< @/../clients/rust/examples/protocol_templates.rs#helpers

:::

## Running them

You upload a template once. After that, each use is a single run instruction, which a bot or
service can build in TypeScript or Rust. Building a run doesn't need the template's bytecode, only
three things from its author: the order of the declared accounts, the order of the inputs, and,
for a template that takes one, the accounts in each [account group](/guide/account-groups) or batch
row.

Each page shows the template and a run of it, in TypeScript and in Rust.

- TypeScript runs are in `clients/js/examples/protocols/run/`. They bind accounts by name, and
  `buildKitRunInstruction` puts them in the template's order.
- Rust templates are in `clients/rust/examples/protocol_templates.rs`, built with
  `ProgramBuilder`. Rust runs are in `clients/rust/examples/protocol_templates_run.rs`.
