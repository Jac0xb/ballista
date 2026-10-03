# Settle at a signed quote

<p class="protocol-line">Ed25519 · Instructions sysvar · SPL Token</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against the Token
program and the USDC and wrapped SOL mints copied from mainnet, with Solana's Ed25519 precompile,
and in Mollusk, a harness that runs Solana programs without a validator; not yet run on devnet or
mainnet.

**Cost:** Ballista's own work took 8,733 of the tested transaction's 8,892
[compute units](/reference/glossary#compute-units); the two token transfers took the rest. Ballista
charges no fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Settles a trade at a price a maker signed off chain. The taker pays the signed price, the maker
delivers, and no single settlement can go past what the maker signed: the price, the size, the
taker, the tokens and the expiry.

The maker, who quotes, signs a quote off chain and sends it to the taker, who accepts it. Settling
moves the maker's tokens, so the maker signs the transaction too. Because the template holds the
trade to the quote, the service that co-signs for the maker only has to check that the transaction
runs this template and nothing else that could spend the maker's accounts.

Three Solana pieces make this work:

- An **Ed25519 signature** is 64 bytes that prove the holder of a key, here the maker's wallet
  key, signed exactly these bytes.
- A **precompile** is a program built into the Solana runtime; the Ed25519 precompile checks the
  signatures in its instruction, and a bad one fails the whole transaction.
- The **Instructions sysvar** is a read-only account the runtime fills with the transaction's
  instructions, so the template can read the Ed25519 instruction before its own.

The quote is 128 bytes: the tag `BLSTQT01`, then `price`, `maxAmount` and `expiry` as eight-byte
little-endian integers, then the 32-byte addresses of `taker`, `baseMint` and `quoteMint`. The
maker delivers the base token, and the taker pays in the quote token. Amounts are in base units, a
token's smallest unit. `price` has six decimals, so 1,000,000 means one quote unit per base unit.
`expiry` is the last Unix timestamp at which the quote can settle.

The precompile proves only that some key signed some bytes. The template ties them to the maker
and to this trade, checking in order that:

1. the instruction directly before the run is the Ed25519 precompile;
2. it holds one self-contained signature over 128 bytes: the signature, the key and the message
   all sit in its own data, so the bytes the template reads are the bytes the precompile checked;
3. the signing key is `maker`'s;
4. the message starts with `BLSTQT01`, the tag that separates quotes from everything else the
   maker signs (a signature covers bytes, not what they mean);
5. the clock hasn't passed `expiry`;
6. the quote's `taker` is the wallet signing as `taker`;
7. `amount` is at most `maxAmount`;
8. the taker pays from an account in `quoteMint`, and the maker delivers from one in `baseMint`
   (a token transfer only moves between accounts of one mint, so this pins both sides);
9. the account the taker pays into belongs to the maker, not one the taker picked.

It then prices the trade at `amount × price ÷ 1,000,000`, rounded up in the maker's favor, and
makes two SPL Token transfers: the taker pays that to the maker, and the maker delivers `amount` to
the taker.

::: warning A quote can settle more than once
This template keeps no state, so it can't count settlements. Until a quote expires, it can settle
again unless the maker's co-signer refuses a second settlement of the same quote. `maxAmount` caps
each settlement, not the quote as a whole: in a test, settling the same quote twice delivered 3 SOL
against a `maxAmount` of 2 SOL. A template that must refuse replays itself can keep a per-maker
nonce in a [registry entry](/guide/registries).
:::

## Template

::: code-group

<<< @/../clients/js/examples/protocols/signed-quote-settlement.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#signed-quote [Rust · Template]

<<< @/../clients/js/examples/protocols/run/signed-quote.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#signed-quote [Rust · Run]

:::

The Rust tabs' `program`, `anchor` and account flags are
[shared helpers](/examples/protocols/#rust-helpers).

The first steps come from the SDK's `ed25519Signature` helper, which returns them with a `field`
reader for the signed message. `field` refuses a read past the message, and a template that uses
`field` without the steps doesn't compile. The Rust template has no helper: it writes the same
checks out with `ProgramBuilder` and compiles to the same bytes.

The helper's `signer` must be a key the transaction's builder can't choose. With an input, or the
key of an account nothing constrains, the builder could sign a quote with a key of their own. Here
it is the key of `maker`, which must also sign the transaction, so a quote settles only if its
signer signs the settlement too. The helper refuses an input, but it can't see an account's
constraints: those are the template's to get right.

## Run it

The maker signs the quote's 128 bytes with its wallet key and sends the quote and the 64-byte
signature to the taker. The taker builds one transaction with two instructions, in this order: the
Ed25519 instruction, carrying the maker's key, the signature and the quote, then the run. The taker
and the maker both sign it.

The Run tabs build both. `quoteMessage` (TypeScript) and `Quote::message` (Rust) write the 128
bytes the maker signs. `buildEd25519Instruction` and `ed25519_instruction` build the Ed25519
instruction with one signature and each of its three instruction indexes set to `u16::MAX`
(0xffff), which means "this instruction's own data". `buildSignedQuoteRun` and `run_signed_quote`
return the two instructions in order.

The Run tabs pass the eight declared accounts, `instructions`, `tokenProgram`, `taker`, `maker`,
`takerQuoteAccount`, `makerQuoteAccount`, `makerBaseAccount` and `takerBaseAccount`, then the input
`amount`. `instructions` is the Instructions sysvar. The four token accounts are writable, and the
SPL Token program must own them, so Token-2022 accounts are rejected.

::: warning The Ed25519 instruction goes directly before the run
The template reads the signature from the instruction just before its own. With nothing before the
run, it fails at `quoteInstructionIndex`. With any other instruction in between, even a memo, it
fails at `quoteIsEd25519`. `currentInstructionIndex` gives the top-level instruction's index, so if
another program calls the run through a CPI, the signature must sit directly before that program's
instruction.
:::

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/signed_quote.rs` runs it against copies of mainnet's Token
  program and the USDC and wrapped SOL mints, with the Ed25519 precompile. Selling 1.5 SOL and a
  lamport pays 225,375,001 USDC units, rounded up. Settling at exactly `expiry` lands and one second
  later fails; exactly `maxAmount` lands and one lamport more fails; USDC named as the base mint
  fails at `deliversTheQuotedMint`. The same quote settled twice in its window lands both times, 3
  SOL against a `maxAmount` of 2 SOL. No failed run moves any balance. A settlement costs 8,892
  compute units, and the transaction is 783 bytes with a 15,000 lamport fee. The precompile uses no
  compute units: it adds one signature to the fee and 276 of the 783 bytes.
- **End to end in Mollusk.** `tests/ballista/src/lib.rs` (`signed_quote_settles_only_as_the_maker_signed`)
  uploads the template as the TypeScript SDK compiles it and runs it in Mollusk after a real
  Ed25519 instruction. At a price of 2,500,000 (2.5 quote units per base unit), taking 3,000,001
  base units pays the maker 7,500,003, rounded up from 7,500,002.5, and delivers 3,000,001 to the
  taker.
- **Failures in the precompile.** A signature or signed price with one bit flipped fails the
  Ed25519 instruction with `InvalidSignature`, so Ballista never runs.
- **Failures in the template**, each at its own step: nothing before the run
  (`quoteInstructionIndex`); a memo in between (`quoteIsEd25519`); another key's signature
  (`quoteIsBySigner`); two signatures, a 127-byte message, or any one of the three instruction
  indexes set to 0 instead of `u16::MAX` (`quoteIsOneSelfContainedSignature`); a message the maker
  signed under the tag `BLSTQT02` (`quoteIsTagged`); an expiry one second before the clock
  (`quoteHasNotExpired`); another taker (`quoteIsForThisTaker`); one base unit over `maxAmount`
  (`withinTheQuotedSize`); a quote for another quote mint (`paysInTheQuotedMint`); a payment
  account the taker owns (`paymentReachesTheMaker`).
- **Not tested.** Devnet and mainnet. Both suites build their own Ed25519 instruction and run, in
  the same layout as the Run tabs (the LiteSVM test checks its copy against Solana's own builder),
  so no test runs the Run tabs' code against the program. The TypeScript run is only type-checked.
- `clients/js/src/compiler.test.ts` checks the `ed25519Signature` helper: its steps and header
  check, that it refuses an input as `signer`, and that `field` stays inside the message.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
