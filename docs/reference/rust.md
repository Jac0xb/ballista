# Rust SDK

The `ballista-sdk` crate, in `clients/rust`, authors templates, derives template and registry entry
addresses, builds the instructions that upload and run a template, encodes run inputs, and decodes
errors and a run's logged output. Its `ProgramBuilder` writes the same bytecode as the TypeScript
compiler: declaring the same records in the same order produces identical bytes, and a test checks
this against the TypeScript-compiled `fixtures/system-transfer.hex`.

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
cargo run -p ballista-sdk --example author_template   # author templates
cargo run -p ballista-sdk --example run_template      # build a run and decode failures
```

`protocol_templates` builds the thirteen [protocol templates](/examples/protocols/), byte-identical
to the TypeScript fixtures, and `protocol_templates_run` and `protocol_runs` build their runs.

## Authoring with the builder

`ProgramBuilder` works one level below the TypeScript authoring API. You declare accounts and
inputs, then emit bytecode instructions, and each call that computes a value returns the
[register](/reference/glossary#register) that holds it. The flag, type, and opcode constants come
from `ballista_common`, which `ballista_sdk` re-exports. This template pays each batch row's
recipient and keeps the total within a budget:

<<< @/../clients/rust/examples/docs_rust_reference.rs#author

| Call | Effect |
| --- | --- |
| `account(flags, address, owner, min_data_len)` | Declares a fixed account. `address` and `owner` optionally pin it |
| `row_account(flags, address, owner, min_data_len)` | Declares one account of the batch row |
| `batch(max, min)` | Sets the maximum and minimum number of batch rows |
| `input(value_type, max_len)` | Declares an input; `max_len` applies only to `VALUE_BYTES` |
| `load_input(input)`, `const_u64(value)` | Put an input or a constant into a new register |
| `blob(bytes)` | Stores literal bytes, such as a discriminator, and returns their `(offset, len)` |
| `cpi(program, accounts, segments)` | Declares a CPI: its program, its `(account, flags)` pairs, and its data segments. Returns its index |
| `for_each(carry_mask, body)` | Loops over the batch rows. Bit *n* of `carry_mask` carries register *n* from row to row and past the loop |
| `invoke(cpi, condition)` | Performs a CPI; with a `bool` register as `condition`, only when it is true |
| `binary(opcode, left, right)` | A two-operand instruction such as `OP_ADD` or `OP_LTE` |
| `mov(dst, source)` | Copies one register into another, to update a carried register |
| `require(condition)` | Fails the run unless the `bool` register `condition` is true |

The other methods:

- `const_bool`, `const_i64`, `const_u128`, `const_pubkey`, and `const_bytes` load constants.
- `account_key`, `account_owner`, `account_lamports`, `account_data_len`, and `account_is_empty`
  read an account's fields; `read` and `read_dynamic` read its data; `clock_slot` and
  `clock_timestamp` read the clock.
- `not`, `select(condition, if_true, if_false)`, and `cast(opcode, value)` cover the remaining
  value operations.
- `derive_pda` and `create_pda` compute [PDAs](/reference/glossary#pda); `row_input` declares a row
  input; `account_groups` with `cpi_with_group` forwards account groups; `set_cpi_max_data_len`
  sets a CPI's maximum data length; `flags` sets the header flags.
- `op`, `emit`, `register`, `register_count`, and `pubkey` work with raw records, as do the free
  functions `record`, `range_immediate`, and `segment_width`. The `*_mut` accessors exist for
  negative tests.

[Template language](/reference/language#steps) says what each step does, and
[Wire format](/reference/wire-format) lists every opcode. Callers pass fixed accounts in the order
you declare them, then each row's accounts in the same way, and encode inputs in declaration order.

### Checking a template

`build()` writes the header and checks only the payload size. `ProgramView::parse(..)?.verify()`
runs the verifier the program runs when it finalizes a template; the
[trust model](/guide/trust-model#finalization-checks) lists what it checks. The builder applies none
of the TypeScript compiler's rules, so these pass `build()` and fail `verify()`:

- **A read past `min_data_len`.** `read(opcode, account, offset)` needs the account's
  `min_data_len` to be at least `offset` plus the read's width: `ReadOutOfBounds` (6124).
  `read_dynamic` is checked when it runs instead.
- **A `bytes` segment in a CPI.** `cpi` counts a `bytes` register as 0 bytes, and `verify` requires
  the CPI's maximum data length to equal the total with each `bytes` register at its maximum
  length. Set it with `set_cpi_max_data_len`, or `verify` fails with `InvalidCpi` (6115).
- **A `create_pda` bump of another type.** The bump is a `u64` register: `TypeMismatch` (6119).

`create_pda(program, bump, seeds)` derives the address with the bump you supply. Unlike
`derive_pda`, it does not search for the canonical bump, so a bump from an input lets the caller
pick another valid address for the same seeds. A bump above 255, or one that gives no valid
address, fails the run with `InvalidPdaDerivation` (6017).

Pin each invoked program's address, and each read account's owner or address: the builder does not
require it as the TypeScript compiler does, and an unpinned program account lets the caller choose
which program runs.

### Math

| Call | Effect |
| --- | --- |
| `mul_div(a, b, c)`, `mul_div_ceil(a, b, c)` | `a × b ÷ c` for three `u64` or three `u128` registers, rounded down or up. The product is exact, so only the result has to fit |
| `pow10(exponent)` | `10^exponent` as a `u128`, from a `u64` register holding 0 to 38 |
| `binary(OP_REM, a, b)` | `a mod b`, with the sign of `a` |
| `binary(OP_SHL, a, b)`, `binary(OP_SHR, a, b)` | Shifts a `u64` or `u128` register by the `u64` in `b` |
| `binary(OP_BIT_AND, a, b)`, `OP_BIT_OR`, `OP_BIT_XOR` | Bitwise operations on matching `u64` or `u128` registers |
| `read(OP_READ_I32, account, offset)` | Four bytes as a signed number, into an `i64` register. Every call that takes a read opcode accepts `OP_READ_I32` |

[Template language](/reference/language#computation) lists each operation's types and the errors it
can raise.

### Loops

`repeat(count, max, carry_mask, body)` emits a count loop and returns its instruction index. The body
runs as many times as the `u64` register `count` holds when the loop starts. A count above `max`, 1
to 255, fails the run with `LoopCountExceeded`; it is not clamped. `carry_mask` works as it does for
`for_each`, and `loop_index` gives the current pass, counting from 0. Loops run one after another
and never nest. A `repeat` body has no rows, and a template with a batch needs a `for_each`.
[Limits](/reference/limits) has the counts.

### Output and run events

| Call | Effect |
| --- | --- |
| `emit_data(parts)` | Logs the encoded `parts` as one `Program data:` line. The first part is a `Segment::Literal` tag |
| `set_return_data(parts)` | Sets the encoded `parts` as the run's return data |
| `flags(PROGRAM_FLAG_EMIT_EVENT)` | Makes every successful run log its run event |

Both calls encode `Segment`s as `cpi` does and return the instruction's index. The builder checks
none of the [output rules](/reference/language#output), such as the tag rule and where return data
may go; `verify` rejects a break with `InvalidOutput` (6130). The plain `emit` method is different:
it appends a raw instruction record.

<<< @/../clients/rust/examples/docs_rust_reference.rs#output

[Reading a run's output](#reading-a-runs-output) decodes both from a transaction.

### Introspection and byte reads

Introspection reads the other instructions in the transaction through the Instructions sysvar.
Declare it as a fixed account pinned to `INSTRUCTIONS_SYSVAR_ID`, and pass it read-only, as
`AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR_ID, false)`.

| Call | Effect |
| --- | --- |
| `introspect(opcode, sysvar, index, position)` | One fact, picked by `opcode` from the table below. `index` and `position` are `u64` registers, or `NO_INDEX` |
| `read_instruction_data(read_opcode, sysvar, index, offset)` | A value from instruction `index`'s data at the `u64` offset in `offset`, typed by an `OP_READ_*` opcode |
| `read_instruction_bytes(sysvar, index, offset, len)` | `len` bytes, 1 to 1,024, of instruction `index`'s data |
| `read_account_bytes(account, offset, len)` | `len` bytes, 1 to 1,024, of an account's data, read in place |
| `bytes_len(value)` | The length of a `bytes` register, as a `u64` |

| `opcode` | Operands | Result |
| --- | --- | --- |
| `OP_INSTRUCTION_COUNT` | None | The number of instructions in the transaction |
| `OP_INSTRUCTION_INDEX` | None | The index of the transaction instruction that is running. Under a CPI, such as a nested run, that is the outer instruction |
| `OP_INSTRUCTION_PROGRAM` | `index` | The program instruction `index` calls, as a `pubkey` |
| `OP_INSTRUCTION_ACCOUNT_COUNT` | `index` | How many accounts it lists |
| `OP_INSTRUCTION_ACCOUNT` | `index`, `position` | The address of its account at `position` |
| `OP_INSTRUCTION_ACCOUNT_FLAGS` | `index`, `position` | That account's flags, as a `u64`: bit 0 is signer, bit 1 is writable |
| `OP_INSTRUCTION_DATA_LEN` | `index` | The length of its data |

<<< @/../clients/rust/examples/docs_rust_reference.rs#introspection

[Template language](/reference/language#introspection) says what each source returns and when a
read fails. Declare an account that `read_account_bytes` reads without `ACCOUNT_WRITABLE`, and pass
it read-only: neither the builder nor `verify` checks this, and the run fails with
`WritableAccountBytesRead`.

The Rust SDK has no counterpart to TypeScript's `ed25519Signature` or `rateLimit`. The
`protocol_templates` example writes both by hand, in `signed_quote_settlement` and
`jupiter_daily_cap_swap`; [Settle at a signed quote](/examples/protocols/signed-quote) walks through
the first.

### Registries

A [registry](/reference/language#registries) keeps state between runs, in entries that Ballista owns,
one for each key the template computes.

| Call | Effect |
| --- | --- |
| `open_registry(entry, key, payer, index, size, system_program)` | Checks registry `index`'s entry in account `entry`, or creates it, paid by `payer`. `key` is `Some` of a `pubkey` register, or `None` for 32 zero bytes; `size` is the field bytes. Returns the instruction's index |
| `read_registry(entry, offset, read_opcode)` | Reads the field at `offset` into a new register, with the width and type of `read_opcode` |
| `write_registry(entry, offset, read_opcode, value)` | Writes register `value` into the field. `read_opcode` is `OP_READ_BOOL`, `OP_READ_U64`, `OP_READ_I64`, `OP_READ_U128`, or `OP_READ_PUBKEY`, matching `value`'s type |

<<< @/../clients/rust/examples/docs_rust_reference.rs#registry

The builder checks none of the [registry rules](/reference/language#registries); `verify` rejects a
break with `InvalidRegistry` (6132). Pass each entry writable, at the address
`find_registry_entry_address` derives. A run fails with `InvalidRegistryEntry` (6025) when the
account is not that entry, and when two entries of one registry get equal keys: both name one
account, and its second open fails.

## Addresses and hashing

<<< @/../clients/rust/examples/docs_rust_reference.rs#addresses

- `find_template_pda(creator, template_id)` derives a template's address from the seeds
  `"template"`, the creator's address, and the 16-bit template ID.
- `find_registry_entry_address(template, registry_index, key)` derives an entry's from `"registry"`
  (`REGISTRY_SEED`), the template's address, the registry index, and the 32-byte key, `[0; 32]` for
  an entry without one. It panics if the index is not below `MAX_REGISTRIES` (8).
- `template_hash(payload)` is the payload's SHA-256 hash, which the upload instructions carry.

Both address functions derive under `ballista_sdk::ID`, the address of the pre-release devnet
build, which rejects templates from this repository
([status](/guide/security#audit-status)). Each has a `_for_program` variant that takes your
deployment's program ID, as every instruction builder does.

The crate also exports `TEMPLATE_SEED`, `BALLISTA_ID` (the same as `ID`), and the program IDs
`SYSTEM_PROGRAM_ID`, `TOKEN_PROGRAM_ID`, `ASSOCIATED_TOKEN_PROGRAM_ID`, `INSTRUCTIONS_SYSVAR_ID`, and
`ED25519_PROGRAM_ID`.

## Calling your own program

A template holds an address as its 32 bytes, from `Pubkey::to_bytes()`. An Anchor program's
instruction data starts with the handler's [discriminator](/reference/glossary#discriminator), which
`anchor_discriminator(name)` returns for the handler's snake_case name. The arguments follow in
Borsh order, which writes integers little-endian and addresses as 32 bytes, as `DATA_REG_*`
segments encode them.

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

`RunInputs` encodes values in the order the template declares them: `bool`, `u64`, `i64`, `u128`,
`pubkey`, and `bytes`, which gets a length prefix. If the template declares account groups, call
`groups(&[..])` first, with one length per group. Row inputs follow the fixed inputs, one set per
row.

`run_instruction` puts the template account first, read-only, followed by your runtime accounts in
the template's order: fixed accounts, then each batch row's accounts, then account group members,
group by group.

## Reading a run's output {#reading-a-runs-output}

A `Program data:` line doesn't name the program that logged it. `program_data(logs)` follows the
`invoke`, `success`, and `failed` lines around each one, and returns it with its program and stack
height: 1 for a run that is one of the transaction's instructions, more for a nested run.
`ballista_output(&ballista_sdk::ID)` then tells a run event from an `EMIT` by the
[tag rule](/reference/language#output). It gives `None` for another program's line, even one with
the same bytes.

<<< @/../clients/rust/examples/docs_rust_reference.rs#run-output

- `decode_run_event(bytes)` decodes the 47-byte run event: `version`, `iterations`, `expanded` (the
  invokes reached), `executed` (a bit per invoke reached, set if it ran), and `template`.
  [Wire format](/reference/wire-format#run-event) has the layout.
- `program_data` fails with `LogError::Truncated` when the logs stop inside an invocation, as they
  do past Solana's log limit, and with `LogError::Malformed(line)` when they don't nest.
- A failed transaction still logs what ran before the failure. Check that it succeeded first.
- Return data names the program that set it, not the template. Read it from the return data that
  simulation or the transaction's metadata reports, as `returned_total` does. A `Program return:`
  log line names the program whose call just ended instead, so after a run that sets none, it can
  hold a called program's bytes under Ballista's name.

## Decoding failures

<<< @/../clients/rust/examples/docs_rust_reference.rs#decode-failure

`decode_ballista_error` returns a `DecodedError` with the error's `name`, its `source` (runtime or
verifier), and its `context`. The low 16 bits of a code name the error, and the high 16 bits say
where it happened: for most runtime errors the program counter, and
`ProgramView::parse(&payload)?.instructions[pc]` is that instruction. The kinds matched above give
an account index, an input index, or a count instead. [Errors and events](/guide/errors-and-events)
lists the context of every kind.

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
