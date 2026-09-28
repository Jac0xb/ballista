# Rust SDK

The `ballista-sdk` crate, in `clients/rust`, lets Rust code author templates, derive template
addresses, build the instructions that upload and run a template, encode run inputs, and decode
Ballista error codes. Its `ProgramBuilder` writes the same bytecode as the TypeScript compiler:
declaring the same records in the same order produces identical bytes, and a test checks this
against the TypeScript-compiled `fixtures/system-transfer.hex`.

```toml
[dependencies]
ballista-sdk = "1"
# The SDK's instructions and addresses are solana-program 4.1.0 types; use the same version.
solana-program = "=4.1.0"
```

Two runnable examples cover the basics: one authors templates, the other builds a run and decodes
failures.

```bash
cargo run -p ballista-sdk --example author_template
cargo run -p ballista-sdk --example run_template
```

Three more cover the protocol templates in the [examples](/examples/protocols/):
`protocol_templates` builds all twelve with `ProgramBuilder`, byte-identical to the TypeScript
fixtures, and `protocol_templates_run` and `protocol_runs` build run instructions for them.

## Authoring with the builder

`ProgramBuilder` works one level below the TypeScript authoring API. You declare accounts and
inputs, then emit bytecode instructions directly, and each call that computes a value returns the
register that holds it. A register is a numbered slot that holds one value during a run. The flag,
type, and opcode constants come from the shared `ballista_common` crate, which `ballista_sdk`
re-exports.

The example below pays `amount` lamports (the smallest unit of SOL) to each batch row's recipient
through the System Program, keeps a running total, and requires the total to stay within `budget`.

```rust
use ballista_sdk::{
    ballista_common::template::{
        ProgramView, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
        OP_ADD, OP_LTE, VALUE_U64,
    },
    ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
};

let mut builder = ProgramBuilder::new();
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
builder.batch(30, 1);

let amount_input = builder.input(VALUE_U64, 0);
let budget_input = builder.input(VALUE_U64, 0);
let amount = builder.load_input(amount_input);
let budget = builder.load_input(budget_input);
let total = builder.const_u64(0);
let discriminator = builder.blob(&[2, 0, 0, 0]);
let transfer = builder.cpi(
    system,
    &[(treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (recipient, ACCOUNT_WRITABLE)],
    &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
);
builder.for_each(1 << total, |body| {
    body.invoke(transfer, None);
    let sum = body.binary(OP_ADD, total, amount);
    body.mov(total, sum);
});
let within = builder.binary(OP_LTE, total, budget);
builder.require(within);

let payload = builder.build()?;
ProgramView::parse(&payload)?.verify()?;
```

| Call | Effect |
| --- | --- |
| `account(flags, address, owner, min_data_len)` | Declares a fixed account and returns its reference. `address` and `owner` optionally pin the account's address or owner program |
| `row_account(flags, address, owner, min_data_len)` | Declares one account of the batch row |
| `batch(max, min)` | Sets the maximum and minimum number of batch rows |
| `input(value_type, max_len)` | Declares an input; `max_len` applies only to `VALUE_BYTES` |
| `load_input(input)`, `const_u64(value)` | Put an input or a constant into a new register |
| `blob(bytes)` | Stores literal bytes, such as an instruction discriminator, and returns their `(offset, len)` |
| `cpi(program, accounts, segments)` | Declares a CPI (a call to another program): its program, its `(account, flags)` pairs, and the segments of its instruction data. Returns the CPI's index |
| `for_each(carry_mask, body)` | Emits a loop over the batch rows. Bit *n* of `carry_mask` keeps register *n*'s value from one row to the next and after the loop |
| `invoke(cpi, guard)` | Performs a declared CPI. If `guard` names a `bool` register, the CPI runs only when that register is true |
| `binary(opcode, a, b)` | Emits a two-operand instruction such as `OP_ADD` or `OP_LTE` |
| `mov(dst, src)` | Copies one register into another; used to update a carried register |
| `require(register)` | Fails the run unless the `bool` register is true |

Other methods cover the rest of the instruction set, such as `read` and `read_dynamic` for account
data, `derive_pda` and `create_pda`, `row_input`, and `account_groups` with `cpi_with_group`. The
sections below cover math, loops, output, and introspection, and
[Wire format](/reference/wire-format) lists every opcode.

Declaration order matters. Callers pass fixed accounts in the order you declare them, then each
batch row's accounts in the order you declared the row accounts, and they encode inputs in the
order you declare the inputs.

`builder.build()` writes the header from the final counts and checks only the payload size. Check
the result with `ProgramView::parse(..).verify()` before uploading; it runs the same verifier the
program runs when a template is finalized.

The builder does not apply the TypeScript compiler's pinning rules, which require an invoked
program to pin its address and a read account to pin its owner or address. Pin them yourself: an
unpinned program account lets the caller choose which program runs.

### Math

| Call | Effect |
| --- | --- |
| `mul_div(a, b, c)`, `mul_div_ceil(a, b, c)` | `a × b ÷ c` for three `u64` or three `u128` registers, rounded down or up. The product is exact, up to 256 bits, so only the result has to fit |
| `pow10(exponent)` | `10^exponent` as a `u128`, from a `u64` register holding 0 to 38 |
| `binary(OP_REM, a, b)` | `a mod b` for matching `u64`, `i64`, or `u128` registers. The result takes the sign of `a` |
| `binary(OP_SHL, a, b)`, `binary(OP_SHR, a, b)` | Shifts a `u64` or `u128` register by the `u64` in `b`. `OP_SHR` rounds down |
| `binary(OP_BIT_AND, a, b)`, and the same with `OP_BIT_OR` or `OP_BIT_XOR` | Bitwise operations on matching `u64` or `u128` registers |
| `read(OP_READ_I32, account, offset)` | Reads four bytes as a signed number into an `i64` register. Every call that takes a read opcode accepts `OP_READ_I32` |

A zero divisor fails the run with `DivisionByZero`. A result too large for its type, a left shift
that would drop a set bit, an exponent above 38, or `i64::MIN` modulo -1 fails it with
`ArithmeticOverflow`. Operands of the wrong type fail verification with `TypeMismatch`.

### Loops

`repeat(count, max, carry_mask, body)` emits a count loop and returns its instruction index. The
body runs as many times as the `u64` register `count` holds when the loop starts. `max`, from 1 to
255, caps it: a larger count fails the run with `LoopCountExceeded`. `carry_mask` works as it does
for `for_each`, and `loop_index` gives the current pass, counting from 0.

A template holds up to eight loops (`MAX_LOOPS`) of either kind. They run one after another and
never nest. A template with a batch needs at least one `for_each`. A `repeat` body has no rows, so
it cannot name a row account or row input. The verifier rejects a ninth loop, a loop inside
another, a `max` of 0, or a row inside `repeat` with `InvalidLoop`. A batch without a `for_each`,
or a `for_each` inside another loop, gets `InvalidBatch`. The limit of 64 CPIs per run counts every
loop at its maximum.

### Output

| Call | Effect |
| --- | --- |
| `emit_data(parts)` | Logs the encoded `parts` as one `Program data:` line. The first part must be a `Segment::Literal` tag of at least 4 bytes (`MIN_EMIT_TAG_LEN`) that does not start with `BEV` (`RUN_EVENT_TAG_FAMILY`), so the log cannot pass for Ballista's run event |
| `set_return_data(parts)` | Sets the encoded `parts` as the run's return data, the bytes a program hands back to its caller. At most once, outside every loop, and with no `invoke` after it, because invoking a program clears return data |

Both encode `Segment`s as `cpi` does, write no register, and return the instruction's index. Each
can encode at most 1,024 bytes, counting a `bytes` register at its maximum length. The builder
checks none of this; the verifier rejects a break with `InvalidOutput`. A template that invoked the
run reads its return data with `return_data`, and a client can read it by simulating a transaction
that ends with the run. The plain `emit` method is different: it appends a raw instruction record.

```rust
// After `builder.require(within)` in the example above:
// log the tag "PAID" and the total, then return the total to the caller.
let tag = builder.blob(b"PAID");
builder.emit_data(&[Segment::Literal(tag), Segment::Register(DATA_REG_U64, total)]);
builder.set_return_data(&[Segment::Register(DATA_REG_U64, total)]);
```

### Introspection and byte reads

Introspection means reading the other instructions in the same transaction. The builder does it
through the Instructions sysvar. A sysvar is an account whose data the Solana runtime maintains,
and this one holds every instruction in the current transaction. Declare it as a fixed account
pinned to `INSTRUCTIONS_SYSVAR_ID`, and pass it read-only in the run, as
`AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR_ID, false)`. The verifier rejects `introspect`,
`read_instruction_data`, or `read_instruction_bytes` on any other account with
`InvalidIntrospection`.

| Call | Effect |
| --- | --- |
| `introspect(opcode, sysvar, index, position)` | One fact about the transaction, picked by `opcode` from the table below. `index` and `position` are `u64` registers; pass `NO_INDEX` for one the opcode does not take |
| `read_instruction_data(read_opcode, sysvar, index, offset)` | A value from instruction `index`'s data at the `u64` offset in `offset`, with the width and type of an `OP_READ_*` opcode |
| `read_instruction_bytes(sysvar, index, offset, len)` | Exactly `len` bytes, 1 to 1,024, of instruction `index`'s data from `offset` |
| `read_account_bytes(account, offset, len)` | Exactly `len` bytes, 1 to 1,024, of an account's data from the `u64` offset in `offset`, read in place without a copy |
| `bytes_len(value)` | The length of a `bytes` register, as a `u64` |

| `opcode` | Operands | Result |
| --- | --- | --- |
| `OP_INSTRUCTION_COUNT` | None | The number of instructions in the transaction |
| `OP_INSTRUCTION_INDEX` | None | The index of the instruction running the template |
| `OP_INSTRUCTION_PROGRAM` | `index` | The program that instruction `index` calls, as a `pubkey` |
| `OP_INSTRUCTION_ACCOUNT_COUNT` | `index` | How many accounts instruction `index` lists |
| `OP_INSTRUCTION_ACCOUNT` | `index`, `position` | The address of the account at `position` in instruction `index` |
| `OP_INSTRUCTION_ACCOUNT_FLAGS` | `index`, `position` | The flags of that account, as a `u64`: bit 0 is signer, bit 1 is writable |
| `OP_INSTRUCTION_DATA_LEN` | `index` | The length of instruction `index`'s data |

An index, position, or byte range beyond what exists fails the run with `InstructionOutOfRange`.
`read_account_bytes` also fails with `WritableAccountBytesRead` if the account is writable in the
run instruction, because a CPI could change the bytes while the run holds them. Declare the account
without `ACCOUNT_WRITABLE` and pass it with `AccountMeta::new_readonly`: neither the builder nor the
verifier checks this.

```rust
use ballista_sdk::{
    ballista_common::template::{
        NO_INDEX, OP_EQ, OP_INSTRUCTION_INDEX, OP_INSTRUCTION_PROGRAM, OP_SUB,
    },
    ProgramBuilder, ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID,
};

let mut builder = ProgramBuilder::new();
let sysvar = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID.to_bytes()), None, 0);
let one = builder.const_u64(1);
let ed25519 = builder.const_pubkey(ED25519_PROGRAM_ID.to_bytes());

// Require the instruction just before this run to call the Ed25519 program.
let current = builder.introspect(OP_INSTRUCTION_INDEX, sysvar, NO_INDEX, NO_INDEX);
let previous = builder.binary(OP_SUB, current, one);
let program = builder.introspect(OP_INSTRUCTION_PROGRAM, sysvar, previous, NO_INDEX);
let is_ed25519 = builder.binary(OP_EQ, program, ed25519);
builder.require(is_ed25519);
```

The Ed25519 program is a precompile: a program built into Solana that checks signatures as part of
the transaction, so a transaction with an invalid signature fails. The Rust SDK has no counterpart
to TypeScript's `ed25519Signature`, which ties such a signature to a template.
`signed_quote_settlement` in the `protocol_templates` example writes the same checks with the calls
above: the instruction before the run is the Ed25519 program and holds one signature, by the maker,
over a message of the expected length, with the key, the signature, and the message all in its own
data. Check the signed key against one the transaction's builder cannot choose, such as a pinned
address or the key of an account that must sign; otherwise the builder can sign with a key of
their own. `protocol_templates_run` builds the Ed25519 instruction, and
[Settle at a signed quote](/examples/protocols/signed-quote) walks through both.

## Addresses and hashing

```rust
use ballista_sdk::{find_template_pda, find_template_pda_for_program, template_hash};

let (template, bump) = find_template_pda(&creator, template_id);
let (other_template, _) = find_template_pda_for_program(&creator, template_id, &other_program_id);
let hash = template_hash(&payload);
```

A template lives at a program-derived address (PDA) of the Ballista program, computed from the
seed `"template"`, the creator's address, and a 16-bit template ID. `find_template_pda` uses the
program ID built into the crate, `ballista_sdk::ID`. Each program version is deployed at its own
address, so `find_template_pda_for_program` takes the program ID of another deployment.
`template_hash` returns the payload's SHA-256 hash, which the upload instructions carry.

## Lifecycle instructions

```rust
let create = ballista_sdk::create_template_instruction(creator, id, &payload);

let begin = ballista_sdk::begin_template_instruction(
    creator,
    id,
    payload.len() as u32,
    ballista_sdk::template_hash(&payload),
);
let write = ballista_sdk::write_template_chunk_instruction(creator, template, offset, chunk);
let finalize = ballista_sdk::finalize_template_instruction(creator, template);
let cancel = ballista_sdk::cancel_template_instruction(creator, template);
```

| Function | Instruction |
| --- | --- |
| `create_template_instruction` | Creates, verifies, and finalizes a template in one instruction, when the payload fits in one transaction |
| `begin_template_instruction` | Starts a chunked upload by creating the template account with the payload's length and hash |
| `write_template_chunk_instruction` | Writes the next chunk; chunks must be written in order |
| `finalize_template_instruction` | Checks the hash, verifies the bytecode, and makes the template permanent and runnable |
| `cancel_template_instruction` | Closes an unfinished upload and returns its lamports to the creator |

The creator signs each of these. Create and begin also pass the System Program, which creates the
template account.

## Inputs and the run instruction

```rust
use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
use solana_program::instruction::AccountMeta;

// template, treasury and recipients are addresses you supply; amount and budget are u64 values.
let inputs = RunInputs::new().u64(amount).u64(budget).finish();
let mut accounts = vec![
    AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    AccountMeta::new(treasury, true),
];
accounts.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
let run = run_instruction(template, accounts, &inputs);
```

`RunInputs` encodes values in the order the template declares them: `bool`, `u64`, `i64`, `u128`,
`pubkey`, and `bytes`, which gets a length prefix. If the template declares account groups, call
`groups(&[..])` first with one length per group. Row inputs follow the fixed inputs, one set per
row.

`run_instruction` puts the template account first, read-only, followed by your runtime accounts in
the template's order: fixed accounts, then each batch row's accounts, then account group members,
group by group.

## Decoding failures

```rust
use ballista_sdk::{decode_ballista_error, ErrorSource};

// `code` is the custom error code (u32) from a failed transaction.
if let Some(error) = decode_ballista_error(code) {
    let context = match (error.source, error.name) {
        (ErrorSource::Verifier, _) => "verifier context",
        (_, "AccountConstraintFailed") => "runtime account index",
        (_, "InvalidRunInputs") => "input index",
        (_, "InvalidAccountRange") => "accounts or rows supplied",
        (_, "CpiAccountLimitExceeded") => "accounts in the call",
        _ => "program counter",
    };
    println!("{} ({context} {})", error.name, error.context);
}
```

`None` means the code is outside Ballista's ranges and came from an invoked program. The low 16
bits of a code name the error, and the high 16 bits, `context`, say where it happened: for most
runtime errors the program counter (the index of the failing bytecode instruction), but an account
index, an input index, or a count for the kinds matched above. The `run_template` example does the
same. [Errors and events](/guide/errors-and-events) lists the context of every kind.

## Decoding a template account {#borrowed-decoding}

```rust
use ballista_sdk::ballista_common::template::TemplateAccount;

let account = TemplateAccount::parse(&account_data)?;
let program = account.finalized_program()?;
let stats = program.verify()?;
```

`TemplateAccount::parse` reads a template account's header and payload, and `finalized_program`
returns a `ProgramView` of the bytecode, failing if the template is not finalized. Both point into
the account bytes instead of copying them. `verify` runs the program's verifier and returns counts
such as the worst-case number of CPIs.

## Using TypeScript-compiled templates {#sharing-compiler-artifacts}

A build step can save `compiled.bytes` from the TypeScript compiler to a file; the guides use a
`.bvm` extension. A Rust service can then hash, upload, inspect, and run those exact bytes. The
example payloads in `fixtures/` come from the TypeScript compiler, and the Rust tests, including
the Mollusk tests that run the program in a simulated Solana runtime, execute them against the
program.
