# `signedQuoteSettlement` against the real programs

- **Snapshot:** slot 451,100,151, clock 1,790,539,782. The Token program is mainnet's (p-token).
- **Tests:** [`tests/signed_quote.rs`](../tests/signed_quote.rs), 13 tests.
- **Template:** [`signed-quote-settlement.ts`](../../../clients/js/examples/protocols/signed-quote-settlement.ts).
- **Quote:** the maker sells wrapped SOL for USDC, both the snapshot's mints:
  - `price` 150,250, which is 150.25 USDC a SOL;
  - `maxAmount` 2 SOL;
  - `expiry` 60 s after the snapshot's clock;
  - made out to the taker.
- **Signing:** the maker signs the quote's 128 bytes with its wallet key, a real Ed25519 key. The
  quote's bytes and the Ed25519 instruction are ported from the Rust runner
  (`protocol_templates_run.rs#signed-quote`).
- **Accounts:** each side's associated token accounts, written under write rule 1. The taker holds
  1,000 USDC and the maker 10 wrapped SOL. The taker pays the fee and signs, and the maker co-signs.

Each figure is marked:
- **asserted:** the tests fail if it changes;
- **measured:** printed by the tests (`--nocapture`) at this commit, and not asserted.

## Verdict

The template behaves against the real programs as it does in Mollusk. Each refusal fails at its own
step, with the same compute units as in Mollusk. The settlement moves exactly the quoted amounts.
No runtime or SDK issue was found, so no test is `#[ignore]`d.

## Runs

| Run | Result | How |
| --- | --- | --- |
| Honest: 1.5 SOL and a lamport | Lands. The taker pays 225,375,001 USDC units, rounded up from 225,375,000.15025. The maker delivers 1,500,000,001 lamports, and both wrapped-SOL accounts' lamports move with their balances | Asserted |
| Clock moved on 60 s, to exactly `expiry` (write rule 3) | Lands | Asserted |
| One second later | Fails at `quoteHasNotExpired` (pc 38), after 3,885 CU | Asserted; pc and CU measured |
| Exactly `maxAmount` | Lands, paying 300,500,000 USDC units, with nothing to round | Asserted |
| One lamport over `maxAmount` | Fails at `withinTheQuotedSize` (pc 47), after 4,536 CU | Asserted; pc and CU measured |
| Quote signed by a stranger's key, the maker still co-signing | The precompile verifies it. Fails at `quoteIsBySigner` (pc 29), after 3,081 CU | Asserted; pc and CU measured |
| Price lowered by one unit after signing | The Ed25519 precompile fails instruction 0 with `InvalidSignature`, custom code 2. Ballista never runs | Asserted |
| The maker's key carrying a stranger's signature over the true quote | The same | Asserted |
| The maker's signature over the quote under the tag `BLSTQT02` | Fails at `quoteIsTagged` (pc 33), after 3,452 CU | Asserted; pc and CU measured |
| Quote made out to another taker | Fails at `quoteIsForThisTaker` (pc 43), after 4,245 CU | Asserted; pc and CU measured |
| Wrapped SOL named as the quote mint | Fails at `paysInTheQuotedMint` (pc 52), after 4,934 CU | Asserted; pc and CU measured |
| USDC named as the base mint | Fails at `deliversTheQuotedMint` (pc 57), after 5,337 CU | Asserted; pc and CU measured |
| Payment into a second USDC account of the taker's | Fails at `paymentReachesTheMaker` (pc 61), after 5,584 CU. The account is made through the System and Token programs | Asserted; pc and CU measured |
| No Ed25519 instruction: the run alone | `ArithmeticOverflow` at `quoteInstructionIndex` (pc 17), after 2,117 CU | Asserted; pc and CU measured |
| The Ed25519 instruction after the run | The same: the run is instruction 0 | Asserted |
| A compute-budget instruction between the Ed25519 instruction and the run | Fails at `quoteIsEd25519` (pc 20), after 2,323 CU | Asserted; pc and CU measured |
| The same compute-budget instruction first | Lands | Asserted |
| The run with the maker's account not signing | Ballista fails with the runtime's `MissingRequiredSignature` after 602 CU, before any step, so no label applies | Asserted; CU measured |
| The same signed quote twice in its window, 1.5 SOL each | Both land. Together they deliver 3 SOL against a `maxAmount` of 2 SOL | Asserted |

No failed transaction moves any of the four balances (asserted).

The replay is the limit the docs warn of: "A quote can settle more than once"
(`docs/examples/protocols/signed-quote.md`). Only the maker's co-signer can refuse the second
settlement, and the no-co-signature run shows that the template requires it.

## Compute units and size

|  | LiteSVM | Mollusk |
| --- | --- | --- |
| Ballista's run, its two transfers included | 8,892 CU | 8,885 CU |
| The quote-token transfer | 76 CU (USDC) | 76 CU (a test mint) |
| The base-token transfer | 83 CU (wrapped SOL) | 76 CU (a test mint) |
| Ballista's own work | 8,733 CU | 8,733 CU |
| The whole transaction | 8,892 CU, 783 bytes, fee 15,000 lamports | No fee |

The CU figures are measured. The transaction's size and fee are asserted, and so is the whole
transaction's CU equalling the run's.

**Does the precompile show in the compute? No.**
- It logs nothing, not even an invoke line (asserted).
- The transaction's 8,892 CU are exactly Ballista's run (asserted).
- Alone in a transaction, the Ed25519 instruction lands having consumed 0 CU (asserted). That
  transaction is 410 bytes, with a fee of 10,000 lamports.

**Where it does show:**
- **The fee.** Its signature is charged like the transaction's own. The settlement pays 15,000
  lamports: the taker's, the maker's and the precompile's signatures (asserted).
- **The size.** It takes 276 of the 783 bytes: 240 bytes of instruction data, 4 bytes framing them,
  and the Ed25519 program's 32-byte key. The data is 16 bytes of count and offsets, the 32-byte key,
  the 64-byte signature and the 128-byte quote. The 240 is asserted, and the split is computed.
- **The compute limit, as headroom only.** Without a compute-budget instruction, Ballista's run sees a
  limit of 203,000 CU. That is 200,000 for the run and 3,000 the runtime reserves for the precompile
  instruction. With the run alone the limit is 200,000 (measured).

LiteSVM has no block cost model, so these tests cannot see what the scheduler charges for verifying
the signature.

## Differences from Mollusk

The Mollusk test is `signed_quote_settles_only_as_the_maker_signed` in `tests/ballista/src/lib.rs`.
- **The same.** Each refusal both tests make fails at the same step, with the same CU: 2,117, 3,081,
  3,452, 3,885, 4,245, 4,536, 4,934 and 5,584. The instruction between the Ed25519 instruction and
  the run is a memo there and a compute-budget instruction here. It costs 2,323 CU in both. A price
  changed after signing fails the precompile, instruction 0, with `InvalidSignature` in both.
- **7 CU more for the settlement, all in the Token program.** Mollusk's quote uses two 6-decimal
  test mints, at 76 CU a transfer. Here the maker delivers wrapped SOL, and p-token takes 83 CU to
  move a native account, whose lamports it moves too.
- **Signatures and fees.** Mollusk takes the signer flags on trust and charges no fee. LiteSVM
  verifies every signature and charges 5,000 lamports for each, the precompile's included.
- **Not re-run here.** Two signatures in one instruction, a 127-byte message, and each of the three
  instruction indexes set to 0. Mollusk fails all five at `quoteIsOneSelfContainedSignature`. Both
  harnesses verify with the same agave-precompiles code.
- **New here.**
  - Settling at exactly `expiry` and at exactly `maxAmount`.
  - `deliversTheQuotedMint`.
  - The Ed25519 instruction after the run, and the compute-budget instruction first.
  - The maker's key with a stranger's signature.
  - The replay.
  - The missing co-signature.

## The precompile build

`litesvm` now has `features = ["precompiles"]`. This pulls in agave-precompiles 4.3.0, which builds
OpenSSL from source (openssl-src 300.6.1+3.6.3) through solana-secp256r1-program's
`openssl-vendored`. That takes perl, make and a C compiler, which the Mac and ubuntu-latest both
have.

Build times, measured once each on an M3 Max (16 cores), for a debug `cargo test --no-run`:
- **Cold:** 20.8 s before, 35.2 s after: 14.4 s more, and 391 crates instead of 368.
- **Over a warm target:** switching the feature on takes 29.0 s and rebuilds 45 crates. OpenSSL's
  build script takes 20.6 s of that, on the critical path.
- **CI:** `Swatinem/rust-cache` caches the dependencies, keyed on the lockfile, so the first run after
  this change pays the OpenSSL build. It was not timed on CI.

The rest of the suite is unchanged: 177 passed and 4 were ignored, before and after. `Cargo.lock`
only gains packages: 25, among them ed25519-dalek 1.0.1, curve25519-dalek 3.2.0 and rand 0.7.3
beside the newer versions already there. `solana-ed25519-program` and `solana-precompile-error`,
both in the tree once the feature is on, become direct dependencies.

## Notes for the harness

- **Precompile failures.** The precompile logs nothing, so `tx::Failure` finds no `failed` line. It
  falls back to the failing instruction's program, which correctly names the Ed25519 program. The
  code is the `PrecompileError` as a custom code. `tx.rs` says of that fallback that "the runtime
  rejected the instruction before its program ran"; for a precompile, the program ran and was
  silent.
- **A missing signature.** When a declared signer does not sign, Ballista fails with the runtime's
  `MissingRequiredSignature`. That carries no Ballista code, so `assert_requirement_failed` cannot
  name it. The test compares the program and the instruction error instead.
- **The ported instruction.** The Rust runner's `ed25519_instruction` is byte for byte Solana's own
  `new_ed25519_instruction_with_signature` (asserted).

## Stale docs

`docs/` was out of bounds for this change. On `docs/examples/protocols/signed-quote.md`:
- **Status** says the template runs in Mollusk and is "not yet run on devnet or mainnet". It now
  also runs in LiteSVM, against mainnet's Token program with the real USDC and wrapped SOL mints. It
  is still not run on devnet or mainnet.
- **What has been tested** lists three things as not tested:
  - a failure at `deliversTheQuotedMint`;
  - a settlement at exactly `expiry`;
  - settling one quote twice.

  All three run here.
- **The replay warning** does not say what a second settlement means for `maxAmount`. The cap
  bounds each settlement, not the quote: two settlements delivered 3 SOL against a 2 SOL maximum.
  Two other texts read as limits on the total:
  - the template's comment on `maxAmount`, "The most base-token base units the maker delivers";
  - the page's claim that neither side "can stretch the trade past what the maker signed".

`docs/examples/protocols/index.md` ("Running against the protocols") says that the signed quote runs
in Mollusk (`tests/ballista/`). It now runs in `tests/protocols/` too.

Neither suite runs the Run tabs' own code. This one ports `Quote::message` and
`ed25519_instruction`, and checks the port against Solana's own builder.
