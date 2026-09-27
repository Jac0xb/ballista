# Rust SDK

The `ballista-sdk` crate, in `clients/rust`, lets Rust code author templates, derive template
addresses, build the instructions that upload and run a template, encode run inputs, and decode
Ballista error codes. Its `ProgramBuilder` writes the same bytecode as the TypeScript compiler:
declaring the same records in the same order produces identical bytes, and a test checks this
against the TypeScript-compiled `fixtures/system-transfer.hex`.

```toml
[dependencies]
ballista-sdk = { path = "clients/rust" }
```

Two runnable examples cover the basics: one authors templates, the other builds a run and decodes
failures.

```bash
cargo run -p ballista-sdk --example author_template
cargo run -p ballista-sdk --example run_template
```

A third example, `protocol_runs`, builds run instructions for the protocol templates in the
[examples](/examples/protocols/).

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
| `for_each(carry_mask, body)` | Emits the loop. Bit *n* of `carry_mask` keeps register *n*'s value from one row to the next and after the loop |
| `invoke(cpi, guard)` | Performs a declared CPI. If `guard` names a `bool` register, the CPI runs only when that register is true |
| `binary(opcode, a, b)` | Emits a two-operand instruction such as `OP_ADD` or `OP_LTE` |
| `mov(dst, src)` | Copies one register into another; used to update a carried register |
| `require(register)` | Fails the run unless the `bool` register is true |

Other methods cover the rest of the instruction set, such as `read` and `read_dynamic` for account
data, `derive_pda` and `create_pda`, `row_input`, and `account_groups` with `cpi_with_group`.
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
use ballista_sdk::{run_instruction, RunInputs};
use solana_program::instruction::AccountMeta;

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

if let Some(error) = decode_ballista_error(code) {
    match error.source {
        ErrorSource::Runtime => println!("{} at instruction {}", error.name, error.context),
        ErrorSource::Verifier => println!("{} (context {})", error.name, error.context),
    }
}
```

`None` means the code is outside Ballista's ranges and came from an invoked program. For most
runtime errors the context is the index of the failing bytecode instruction.
[Errors and events](/guide/errors-and-events) lists the kinds whose context is an account or
input index instead.

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
