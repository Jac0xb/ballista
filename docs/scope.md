# Scope and limits

This page describes what Ballista does and does not do, the design choices behind that scope, and
the limits that follow from them.

## What Ballista is {#product-statement}

Ballista stores a template on chain once and lets anyone run it later with new inputs. A template
is a bounded sequence of Solana program calls with checks between them, and a run either completes
every step or changes nothing. Ballista is not an automation network, a custody service that holds
funds, or a general-purpose programming language.

What it offers: callers stop rebuilding the same instructions for every transaction, every run
applies the same checks, and the program that runs templates is small enough to measure its cost in
compute units (Solana's measure of execution cost).

## Design choices {#deliberate-scope-cuts}

The table below uses a few terms. A register is a numbered slot that holds one value while a
template runs. A CPI (cross-program invocation) is a call from the template to another program. A
PDA (program-derived address) is an address computed from a program ID and seeds, which only that
program can sign for. Keepers are bots that submit transactions for a fee. Zod is the TypeScript
validation library the SDK uses for template documents.

| Choice | Instead of | Why |
| --- | --- | --- |
| Flat tables of fixed-size records | A nested tree of commands | The program reads records in place and can check exact size bounds |
| One loop over the rows the caller supplies, with a maximum set by the template | Nested or unbounded loops | Batches stay possible, and every run is known to finish |
| Registers that exist only during one run | State stored between runs | Templates stay immutable and each run starts fresh |
| SDK helpers that compile to ordinary CPIs | Protocol-specific logic in the program | The program does not depend on any particular protocol |
| Only the signatures the transaction already carries | Ballista signing as its own PDA | Ballista never holds funds or authority of its own |
| No scheduler or keeper rules | Built-in automation | Deciding who may run a template, and preventing repeat runs, are left to the programs it calls |
| Authoring validated with Zod, plus shared encoders and decoders | Generated clients as the main API | Named inputs and accounts, and clear errors before a transaction is built |

Named bindings (`let` and `snapshot`) are names for registers, not stored variables. They make
before-and-after checks around a CPI readable without adding state. A carried binding keeps its
value from one row of the loop to the next, so a batch can enforce a total, but it never outlives
the transaction.

`assertPda` and `assertAta` derive addresses on chain from at most 15 seeds of up to 32 bytes each.
Without a bump, the derivation searches for the canonical bump, so its compute cost varies. With a
bump supplied, it derives once. Either way these checks only compare addresses; they never let a
template sign as a PDA.

## Reading templates without copying {#zero-copy-boundary}

The template account's header and every bytecode record are fixed-size structures with no
alignment requirements, built with Rust's zerocopy crate. The program reads them directly from
account memory. Section lengths come from the program header and must add up to the payload's
exact length, and parsing a stored template allocates no `Vec`, `String`, or `Box`. During a run,
registers hold computed values and slices that point into the input bytes or the template, while
the account list and instruction data for each CPI are built in reusable buffers.

The program checks a template's structure and types once, when the template is finalized. A run
checks the template account, the input encoding, the runtime accounts against their constraints,
and every operation as it executes, but it does not hash or re-verify the stored bytecode.

## Hard limits

| Resource | Limit |
| --- | ---: |
| Compiled template payload | 10,240 bytes |
| Runtime accounts per run | 120 |
| Inputs / run data | 32 / 1,024 bytes |
| Registers / bytecode instructions | 64 / 128 |
| CPIs per run, counting each loop iteration | 64 |
| Accounts per CPI | 64 |
| Instruction data per CPI | 4,096 bytes |
| Accounts per batch row | 1 to 8 |
| PDA seeds / bytes per seed | 15 / 32 |
| Readable CPI return data | 1,024 bytes |

[Limits](/reference/limits) has the full list, including the limits the TypeScript SDK adds and
when each one is checked.

The number of loop iterations is the number of runtime accounts after the fixed accounts, less any
account group members, divided by the accounts per row. The division must be exact, and the result
must fall between the template's minimum and maximum.

Solana's own transaction limits often bind first: account locks, compute, instruction
restrictions, and transaction size all apply. A version 1 transaction can be up to 4,096 bytes but
lists at most 64 account addresses and cannot use address lookup tables, so it fits a run of about
60 runtime accounts once the template account and the Ballista program are counted. Larger runs,
up to Ballista's 120, need a version 0 transaction with an address lookup table, which is limited
to 1,232 bytes.

## Dependency policy

- The JavaScript package pins its dependencies to exact versions, recorded in `pnpm-lock.yaml`.
  Its Solana Kit peer dependency accepts versions from 8.2.0 up to, but not including, 9.
- Dependency install scripts are blocked by default; only `esbuild` is allowed to run one.
- A change to the wire format regenerates the shared example payloads in `fixtures/` with
  `pnpm fixtures`. The Rust verifier tests, the Mollusk tests (which run the program in a simulated
  Solana runtime), and the TypeScript tests all read the same files.
- Each program version is deployed immutably at its own address. Stored templates cannot be
  upgraded; moving to a new version means creating them again under the new deployment.
- The program's Rust dependencies are pinned to exact versions, built on Pinocchio 0.11 (a
  lightweight library for Solana programs), and checked with `cargo build-sbf`, the Solana program
  build tool.

## What the examples do not imply {#product-boundary-for-use-cases}

The [examples](/examples/) show what a template can express when a suitable program instruction and
permission model already exist. They do not mean Ballista can create a protocol operation that does
not exist, find a swap route, keep oracle prices fresh, sign for a user, or run itself later.
