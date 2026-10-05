# Rust SDK

The `ballista-sdk` crate, in `clients/rust`, authors templates, derives template and registry entry
addresses, builds the instructions that upload and run a template, and decodes errors and a run's
logged output.

Templates are written with `ballista_sdk::template`, which mirrors the TypeScript SDK's
`defineTemplate` and `compileTemplate`. Inputs, accounts and variables keep the same camelCase
names, functions are snake_case, and a template compiles to the same bytes with the same checks.
Tests hold every docs and protocol template to the TypeScript compiler's bytes.

## Install

The crate is not on crates.io yet. Add it from the repository:

```toml
[dependencies]
ballista-sdk = { git = "https://github.com/Jac0xb/ballista" }
# The SDK's instructions and addresses are solana-program 4.1.0 types; use the same version.
solana-program = "=4.1.0"
```

`cargo add ballista-sdk --git https://github.com/Jac0xb/ballista` writes the same line, and
`Cargo.lock` pins the commit you build against.

The examples in `clients/rust/examples` run as they are:

```bash
cargo run -p ballista-sdk --example docs_templates      # every guide template, compiled
cargo run -p ballista-sdk --example docs_runs           # a run of each
cargo run -p ballista-sdk --example protocol_templates  # the protocol templates
```

## Authoring

`use ballista_sdk::template::prelude::*;` brings in everything below, plus `Pubkey`, `pubkey!`
and the program IDs. This template pays each batch row's recipient and keeps the total within a
budget:

<<< @/../clients/rust/examples/docs_rust_reference.rs#author

**The template**

- `Template::new()`, then `.input(name, Type)`, `.registry(name, fields)`, `.account(name, ..)`,
  `.batch(..)`, `.account_group(name)`, `.emit_event()` and `.step(..)` or `.steps(..)`.
- `Type` is `Bool`, `U64`, `I64`, `U128`, `Pubkey` or `Bytes(max_length)`.
- `Batch::new(max).min_iterations(n).account(..).input(..)` declares the rows.

**Accounts**

- `account::signer()`, `writable()`, `readonly()`, `program(address)`, `system_program()`.
- Chain `.signer()`, `.writable()`, `.address(..)`, `.owner(..)`, `.min_data_length(n)`.
- `account::registry(registry, payer).key(expr)` holds a registry entry.
- In a step, a `&str` names a fixed account, and `account::iteration(name)` the current row's.

**Steps** (`step::`)

- `let_`, `snapshot`, `assign`, `require`, `set_registry`, `emit`, `set_return_data`.
- `invoke(program)`, then `.writable(..)`, `.signer(..)`, `.readonly(..)`, `.writable_signer(..)`,
  `.data(..)`, `.account_group(name)` and `.when(condition)`.
- `for_each()` and `repeat(count, max)`, then `.step(..)` and `.carry(name)`.
- `.label("name")` on any step names it in errors.

**Data and expressions**

- `data::literal(bytes)`, `data::u8` to `data::u128`, `data::i64`, `data::pubkey`, `data::bool`,
  `data::bytes`.
- Expression functions are the TypeScript ones in snake_case: `input`, `var`, `lamports`,
  `account_data`, `clock_unix_timestamp`, `pda`, `registry`, `return_data`, `instruction_program`,
  and the rest.
- `group_length(group)`, `group_any(group, filter)` and `group_count(group, filter)` take a
  `GroupFilter::new().program(..).equals(offset, value).except_key(..)`, with an optional
  `.min_data_length(n)`.
- Operators build arithmetic: `a + b`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`.
- Methods build comparisons and logic: `.eq`, `.ne`, `.lt`, `.lte`, `.gt`, `.gte`, `.and`, `.or`,
  `.not`, `.min`, `.max`, `.mul_div`, `.cast`. Prefer `.not()` to `!`, which negates a whole chain.

**Helpers**

- `system_transfer`, `token_transfer`, `assert_pda`, `assert_ata`.
- `create_associated_token_account` and `ensure_associated_token_account`, which take an
  `AtaAccounts { .. }` with named fields.
- `ed25519_signature` and `rate_limit`, as the TypeScript helpers of the same names.

[Template language](/reference/language) says what each step and expression does.

### Compiling

`template.compile()` returns a `CompiledTemplate`, or a `CompileError` with the TypeScript
compiler's message. It refuses a call to a program with no pinned address, a data read of an
account that pins neither owner nor address, and a template past a [limit](/reference/limits).
`.unsafe_unpinned()` on an account turns the pin rules off for it.

- `bytes` and `hash` are what you upload.
- `stats` has the sizes: bytes, instructions, registers, calls.
- `source(pc)` and `explain_error(code)` name the step a failure points at.
- `registry_index(name)` gives a registry's index, for its entry addresses.

The on-chain program checks none of the pin rules. `ProgramView::parse(&compiled.bytes)?.verify()`
runs the checks it does run when it finalizes a template; the
[trust model](/guide/trust-model#finalization-checks) lists them.

### Output, introspection and registries

`emit` logs a tagged line, `set_return_data` returns bytes to the caller, and `emit_event` makes
every successful run log its run event:

<<< @/../clients/rust/examples/docs_rust_reference.rs#output

Introspection reads the transaction's other instructions through the Instructions sysvar, declared
as an account pinned to `INSTRUCTIONS_SYSVAR_ID`:

<<< @/../clients/rust/examples/docs_rust_reference.rs#introspection

A [registry](/reference/language#registries) keeps state between runs, in entries Ballista owns:

<<< @/../clients/rust/examples/docs_rust_reference.rs#registry

[Reading a run's output](#reading-a-runs-output) decodes logs and return data from a transaction.

## Addresses and hashing

<<< @/../clients/rust/examples/docs_rust_reference.rs#addresses

- `find_template_pda(creator, template_id)` derives a template's address from the seeds
  `"template"`, the creator's address, and the 16-bit template ID.
- `find_registry_entry_address(template, registry_index, key)` derives an entry's from `"registry"`
  (`REGISTRY_SEED`), the template's address, the registry index, and the 32-byte key, `[0; 32]` for
  an entry without one. It panics if the index is not below `MAX_REGISTRIES` (8).
- `template_hash(payload)` is the payload's SHA-256 hash, which the upload instructions carry.

Both address functions derive under `ballista_sdk::ID`, the address of the pre-release devnet
build ([status](/guide/security#audit-status)). Each has a `_for_program` variant that takes your
deployment's program ID, as every instruction builder does.

The crate also exports `TEMPLATE_SEED`, `BALLISTA_ID` (the same as `ID`), and the program IDs
`SYSTEM_PROGRAM_ID`, `TOKEN_PROGRAM_ID`, `TOKEN_2022_PROGRAM_ID`, `ASSOCIATED_TOKEN_PROGRAM_ID`,
`INSTRUCTIONS_SYSVAR_ID`, and `ED25519_PROGRAM_ID`.

## Calling your own program

`account::program` pins the program to a `Pubkey`. An Anchor program's instruction data starts
with the handler's [discriminator](/reference/glossary#discriminator), which
`anchor_discriminator(name)` returns for the handler's snake_case name. The arguments follow in
Borsh order, which writes integers little-endian and addresses as 32 bytes, as the `data::`
functions encode them.

<<< @/../clients/rust/examples/docs_rust_reference.rs#own-program

## Lifecycle instructions

<<< @/../clients/rust/examples/docs_rust_reference.rs#upload

| Function | Instruction |
| --- | --- |
| `create_template_instruction` | Creates, verifies, and finalizes a template in one instruction |
| `begin_template_instruction` | Starts a chunked upload by creating the template account with the payload's length and hash |
| `write_template_chunk_instruction` | Writes the next chunk; chunks go in order |
| `finalize_template_instruction` | Checks the hash, verifies the bytecode, and makes the template permanent and runnable |
| `cancel_template_instruction` | Closes an unfinished upload and returns its lamports to the creator |

The creator signs each one. Create and begin also pass the System Program, which creates the
template account. Each function has a `_for_program` variant for another deployment.

A legacy or v0 transaction holds at most 1,232 bytes
([limits](/reference/limits#transaction-ceilings)). In a legacy transaction that the creator signs
and pays for, with nothing else in it, that fits a 960-byte payload in
`create_template_instruction`, or a 1,023-byte chunk; a v0 transaction fits 2 bytes less of each.
Another signer or instruction, such as a compute-budget instruction, takes more of the room.

## Inputs and the run instruction

<<< @/../clients/rust/examples/docs_rust_reference.rs#run

`compiled.run(template)` builds a run by name, as TypeScript's `buildRunInstruction` does:

- `.input(name, value)` takes an integer, a `bool`, a `Pubkey`, or bytes.
- `.account(name, address)` binds each fixed account. Its signer and writable flags come from the
  declaration.
- `.row(Row::new().account(..).input(..))` or `.rows(..)` adds batch rows.
- `.group(name, metas)` adds an account group's members, which never sign.
- `.instruction()` checks every name, type and row count, then returns the `Run` instruction, or a
  `RunError`.

The instruction lists the template account first, read-only, then the fixed accounts, each row's
accounts, and the group members, all in declaration order. `.encode_inputs()` returns only the
input bytes; start from `compiled.run_inputs()` when there is no run to build, such as a nested
run's inputs.

## Reading a run's output {#reading-a-runs-output}

A `Program data:` line doesn't name the program that logged it. `program_data(logs)` follows the
`invoke`, `success`, and `failed` lines around each one, and returns it with its program, its stack
height (1 for a run that is one of the transaction's instructions, more for a nested run), and its
invocation number. Lines with the same `invocation` came from one call, so a template's `EMIT`
lines pair with the run event that names it. `ballista_output(&ballista_sdk::ID)` then tells a run
event from an `EMIT` by the [tag rule](/reference/language#output), and gives `None` for another
program's line, even one with the same bytes.

<<< @/../clients/rust/examples/docs_rust_reference.rs#run-output

- `decode_run_event(bytes)` decodes exactly 47 bytes that start with `BEV1`: `version`,
  `iterations`, `expanded` (the invokes reached), `executed` (a bit per invoke reached, set if it
  ran), and `template_address`. [Wire format](/reference/wire-format#run-event) has the layout.
- `program_data` returns a `LogError` naming the line when the logs don't nest. Logs cut off at
  Solana's log limit parse up to the `Log truncated` line, so check for that line when you need
  every event.
- A failed transaction still logs what ran before the failure. Check that it succeeded first.
- Return data names the program that set it, not the template. Read it from the return data that
  simulation or the transaction's metadata reports, as `returned_total` does. A `Program return:`
  log line names the program whose call just ended instead, so after a run that sets none, it can
  hold a called program's bytes under Ballista's name.

## Decoding failures

<<< @/../clients/rust/examples/docs_rust_reference.rs#decode-failure

`decode_ballista_error` returns a `DecodedError` with the error's `name`, its `source` (runtime or
verifier), and its `context`. The low 16 bits of a code name the error, and the high 16 bits say
where it happened: for most runtime errors the program counter. The kinds matched above give an
account index, an input index, or a count instead. [Error codes](/reference/errors#context) lists
the context of every kind.

With the compiled template at hand, `compiled.explain_error(code)` names the step and its label
instead, as `RequirementFailed at steps[2] (withinBudget)`.

Anchor programs number their errors from 6000 too, so a code in Ballista's ranges may come from a
program the run called. Decode it only when the transaction's first `Program <address> failed:` log
line names Ballista. `None` means the code is outside Ballista's ranges.

## Decoding a template account {#borrowed-decoding}

<<< @/../clients/rust/examples/docs_rust_reference.rs#template-account

`TemplateAccount::parse` reads a template account's header and payload, and `finalized_program`
returns a `ProgramView` of the bytecode, failing if the template is not finalized. Both point into
the account bytes instead of copying them. `verify` returns counts such as the worst-case number of
CPIs.

## Using TypeScript-compiled templates {#sharing-compiler-artifacts}

A build step can save `compiled.bytes` from the TypeScript compiler to a file; the guides use a
`.bvm` extension. A Rust service can then hash, upload, inspect, and run those exact bytes. The
example payloads in `fixtures/` come from the TypeScript compiler, and the Rust tests, including
the Mollusk tests that run the program in a simulated Solana runtime, execute them against the
program.
