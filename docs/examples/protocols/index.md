# Protocol templates

Twelve example templates that work with real Solana protocols: Jupiter, Kamino, marginfi, Drift,
Orca, Pyth and Jito. Each one works with a value that only exists while the transaction runs, such as
what a swap returned or what a position has earned. The source files are in
`clients/js/examples/protocols/`.

Treat them as starting points, not tested integrations. None has been run against the real
protocols. [What has been tested](#what-has-been-tested) lists what has been checked.

| Template | Protocol | Decided during the run |
| --- | --- | --- |
| [Deposit exactly what a swap produced](/examples/protocols/jupiter-deposit) | Jupiter → Kamino | How much the swap produced |
| [Swap checked against an oracle](/examples/protocols/jupiter-oracle-swap) | Jupiter + Pyth | Whether the swap paid at least the oracle price, less a tolerance |
| [Sell a whole balance](/examples/protocols/token-sweep) | SPL Token → Jupiter | How much there is to sell |
| [Pay a Jito tip only from profit](/examples/protocols/jito-tip) | Jupiter → Jito | Whether the trade's profit covered the tip |
| [Act only on a fresh price](/examples/protocols/pyth-gate) | Pyth → Jupiter | Whether the price is recent, precise and in range |
| [Compound the fees you collected](/examples/protocols/orca-compound) | Orca | How much the position had earned |
| [Harvest only the positions that earned](/examples/protocols/orca-harvest) | Orca | Which positions have earned enough to collect |
| [Repay what the swap produced](/examples/protocols/kamino-repay) | Jupiter → Kamino | How much the swap produced |
| [Liquidate with a minimum payout](/examples/protocols/kamino-liquidate) | Kamino | How much collateral the liquidator received |
| [Withdraw everything, with a minimum](/examples/protocols/marginfi-withdraw) | marginfi | How much the withdrawal returned |
| [Move funds from marginfi to Drift](/examples/protocols/drift-rebalance) | marginfi → Drift | How much marginfi released, to deposit in Drift |
| [Settle PnL, then withdraw](/examples/protocols/drift-settle) | Drift | Whether the withdrawal reached the wallet |

## What has been tested

- **Compiling and verifying.** CI compiles every template
  (`clients/js/src/protocol-examples.test.ts`) and checks the result with the same verifier the
  Ballista program runs before it stores a template (`common/src/template/verify.rs`).
- **Rust.** Each Rust template is byte-identical to the TypeScript one, and each Rust run passes
  the accounts, flags and inputs its template declares (`clients/rust/tests/protocol_templates.rs`).
- **Running against the protocols.** No test runs any of these templates against the real
  protocols. Their account lists and instruction arguments have not been checked against the
  deployed programs.
- **Account offsets.** The Orca, Pyth and SPL Token offsets are checked against real devnet
  accounts by an opt-in test; see [reading offsets](#reading-offsets-from-an-account). The Kamino,
  Drift and marginfi templates don't read those protocols' accounts. They compare SPL token
  balances before and after each call.
- **Jupiter calls.** Six templates call Jupiter's `route` instruction:
  [deposit](/examples/protocols/jupiter-deposit), [oracle swap](/examples/protocols/jupiter-oracle-swap),
  [sell](/examples/protocols/token-sweep), [repay](/examples/protocols/kamino-repay),
  [price gate](/examples/protocols/pyth-gate) and [Jito tip](/examples/protocols/jito-tip). Each
  sends the `route` discriminator and passes the first accounts of `route` itself, in the order
  Jupiter's published interface lists them: the token program, the signer and, where the template
  measures them, the source and destination token accounts. The rest of the route's accounts
  arrive as an account group. A test that reads the templates checks this
  (`clients/js/src/protocol-semantics.test.ts`), but none has been run against a real route.
  Request routes from Jupiter's Swap API with `useSharedAccounts: false`; the default,
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
| | `price` `i64` | 73 |
| | `conf` `u64` | 81 |
| | `exponent` `i32` | 89 |
| | `publish_time` `i64` | 93 |
| Orca `Position` | `liquidity` `u128` | 72 |
| | `fee_owed_a` `u64` | 112 |
| | `fee_owed_b` `u64` | 136 |
| SPL Token account | `amount` `u64` | 64 |

Pyth and Orca are Anchor programs (Anchor is the most common Solana program framework). Anchor
starts each account's data with an eight-byte discriminator, a tag that identifies the account
type, and the Pyth and Orca offsets above count those eight bytes. Anchor instructions start with a
discriminator too. The templates compute discriminators instead of copying them: the first eight
bytes of `sha256("global:<handler>")` for an instruction and of `sha256("account:<Name>")` for an
account.

::: warning Check before you upload
The tests cover the Ballista side only. They can't tell you whether a protocol has changed since.
Before you upload one of these templates, check each call's accounts, arguments and offsets
against the protocol's current IDL (its published interface description). Require an owner and a
minimum data length for every account a template reads, and prefer fields the protocol documents
as public.
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
