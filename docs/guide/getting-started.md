# Getting started

In this walkthrough you upload a template to a local Solana validator and run it, in TypeScript or
Rust. The template sweeps a vault: it sends everything above a minimum balance, the reserve, to
another account. The caller picks the reserve; the template reads the balance while the transaction
runs and works out the amount.

> **Status: pre-release.** The program hasn't been audited and isn't on mainnet. The current build
> runs only locally, in the test suite; the older devnet build rejects templates from this
> repository. [Details](/guide/security#audit-status)

Steps 1 to 5 build one file, in order: in TypeScript a file such as `sweep.mts`, and in Rust the
body of `fn main() -> Result<(), Box<dyn std::error::Error>>` in `src/main.rs`, ending with
`Ok(())`. Pick a language on any code block and the others follow. The TypeScript imports the SDK
from this repository's source; in your file, change `../../src/index.js` to `@jac0xb/ballista` and
`../../src/kit.js` to `@jac0xb/ballista/kit`.

## Install {#install-the-workspace}

```bash
# TypeScript (Node.js 22 or later). pnpm, yarn, and bun work too.
npm install @jac0xb/ballista @solana/kit

# Rust. ballista-sdk uses solana-program 4.1.0, so the client crates must match it.
cargo new sweep && cd sweep
cargo add ballista-sdk solana-program@=4.1.0 solana-rpc-client@4 solana-keypair@3 \
  solana-signer@3 solana-transaction@4 solana-transaction-error@3 solana-commitment-config@3 \
  solana-message@4
```

Then start a local validator with the Ballista program loaded. This needs the
[Solana CLI](https://solana.com/docs/intro/installation), and builds the program from source:

```bash
git clone https://github.com/Jac0xb/ballista.git
cargo build-sbf --manifest-path ballista/programs/ballista/Cargo.toml
solana-test-validator --reset \
  --bpf-program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD ballista/target/deploy/ballista.so
```

Leave it running. When the file is complete, run it with `npx tsx sweep.mts` or `cargo run`.

## 1. Connect

::: code-group

<<< @/../clients/js/examples/start/connect.ts#connect [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#connect [Rust]

:::

`fundedSigner` creates a wallet and asks the validator's faucet for 1 SOL, which works only on a
local validator or devnet. `send` signs and sends a transaction: version 0 in TypeScript, legacy in
Rust. In TypeScript, `emptyMessage` is that transaction before its instructions; step 3 uses it to
size the upload.

## 2. Define the template

::: code-group

<<< @/../clients/js/examples/start/sweep.ts#define [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#define [Rust]

:::

The template takes one input, `reserve`, and three accounts. Each account entry says what the
caller's account must be: `signer` means it must sign the transaction, `writable` that the
transaction must let it change, `executable` that it must be a program, and `address` fixes it to
one exact address. The steps read the vault's balance in [lamports](/reference/glossary#lamports),
stop the run unless it is above the reserve, and transfer the difference. The label
`'aboveReserve'` names the check in error messages.

Compiling is deterministic, and the Rust builder produces the same bytes; [Author it in
Rust](#author-it-in-rust) explains the difference.

## 3. Upload

::: code-group

<<< @/../clients/js/examples/start/getting-started.ts#upload [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#upload [Rust]

:::

The upload stores the template in its own account, checks it, and finalizes it: locks it for good,
so it can never change. The account's address is a [PDA](/reference/glossary#pda) of the Ballista
program, derived from the creator's address and the template ID. `7` is the template ID, which the
creator picks: any number from 0 to 65,535 that the creator has not used yet.

A transaction holds at most 1,232 bytes, so a large template is uploaded in pieces, one transaction
each ([Limits](/reference/limits#transaction-ceilings)):

- In TypeScript, the plan fits every instruction to the transaction `send` builds: one
  `CreateTemplate` when the template fits, and otherwise a `BeginTemplate`, `WriteTemplateChunk`s
  and a `FinalizeTemplate`.
- In Rust, `create_template_instruction` puts the whole template in one transaction, which fits a
  template of up to 960 bytes. [Template lifecycle](/guide/template-lifecycle#upload) uploads larger
  ones in pieces.

To read a stored template back, see [Inspecting a template](/guide/inspecting-templates).

## 4. Run

::: code-group

<<< @/../clients/js/examples/start/getting-started.ts#run [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#run [Rust]

:::

The vault and the creator are different wallets: anyone can run a finalized template, and the
creator does not sign runs. The run instruction lists the template account first (both SDKs add
it), then the template's accounts in the order it declares them, and carries the inputs in
declaration order. The TypeScript builder takes them by name and checks them against `compiled`; in
Rust you pass them in order yourself. The vault signs because the template declares it as a signer.
The run leaves the vault with exactly the 2,000,000-lamport reserve.

## 5. Read a failure

::: code-group

<<< @/../clients/js/examples/start/getting-started.ts#failure [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#failure [Rust]

:::

This run asks for a reserve larger than the vault's balance, so the `require` step stops it and
nothing moves. The transaction fails with custom error 202623, which is `0x0003177F`. Its low 16
bits, 6015, are the error kind `RequirementFailed`, and its high 16 bits, 3, are the context, which
says where the failure happened. For `RequirementFailed` the context is the program counter: the
index of the failing bytecode instruction. For other kinds it can be an account index or an input
index instead; [Error codes](/reference/errors#context) lists which.

`explainRunError` uses the compiled template's source map to name the step, `steps[1]`, and its
label. Rust has no source map, and the builder's calls don't return program counters.
`decode_ballista_error` splits the code the same way, and for program counter `n` the failing
instruction is `ProgramView::parse(&payload)?.instructions[n]`: here the `require`, opcode 40.

## 6. Call your own program

The sweep calls the System program. To call your own program, give the template the program's
address as 32 bytes and build the instruction data it expects. For an Anchor program, that is the
instruction's 8-byte [discriminator](/reference/glossary#discriminator), then its arguments.

::: code-group

<<< @/../clients/js/examples/start/own-program.ts#own-program [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#own-program [Rust]

:::

`addressBytes` turns a Kit address or a base58 string into the 32 bytes a template takes; Kit's
`getAddressEncoder().encode()` returns a read-only array, which the template's types reject.
`anchorDiscriminator('deposit')` is the first 8 bytes of `sha256("global:deposit")`. In Rust,
`Pubkey::to_bytes()` and `ballista_sdk::anchor_discriminator` do the same. Fixing the program's
address, as here, stops a caller from passing a different program;
[Accounts and CPIs](/guide/accounts-and-cpis#generic-cpi) covers the rest of a call.

## Author it in Rust

The Rust tab of step 2 builds the template with `ProgramBuilder`. It works at a lower level than
the TypeScript compiler: you declare accounts and inputs in order, and each value lives in a
[register](/reference/glossary#register), a numbered slot that holds a value during a run. For
this template the builder's output is byte-identical to the compiler's.

The TypeScript compiler also checks that every program the template calls has a fixed address and
that every account whose data it reads has a fixed owner or address. It works out each account's
minimum data length, and it records which step produced each instruction so that errors can name
the step. With the Rust builder, those decisions are yours; see [Pins](/guide/trust-model#pins). A
template built either way can be uploaded and run from either language.

## Run the shipped examples

Run these from the repository you cloned, after `pnpm install` at its root:

```bash
pnpm --dir clients/js exec tsx examples/start/getting-started.ts   # this page, on the local validator
pnpm --dir clients/js exec tsx examples/transfer.ts        # compile and print the payload
pnpm --dir clients/js exec tsx examples/run-transfer.ts    # build a Solana Kit run instruction offline
cargo run -p ballista-sdk --example author_template        # build two templates in Rust
cargo run -p ballista-sdk --example run_template           # encode inputs and decode errors in Rust
```

The Rust `author_template` example also builds a payroll template that keeps a running total across
rows and enforces a budget. [Batch execution](/guide/batching#carry-a-total-across-rows) shows the
TypeScript version.

## Next

- See [what templates can do](/guide/runtime-values) that a transaction cannot.
- Learn the [template lifecycle](/guide/template-lifecycle).
- Add [snapshots and assertions](/guide/assertions).
- Build a [30-recipient payroll](/examples/payments#bounded-sol-payroll).
- Send large runs in a [version 1 transaction](/guide/transaction-v1), which allows up to 4,096
  bytes.
