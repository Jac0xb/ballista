# Rust SDK

The Rust client provides template addresses, lifecycle and run instruction codecs, typed run input
encoding, error decoding, and the shared authoring builder. Templates authored in Rust compile to
the same bytes as templates authored in TypeScript.

```toml
[dependencies]
ballista-sdk = { path = "clients/rust" }
```

Two runnable examples cover both halves:

```bash
cargo run -p ballista-sdk --example author_template
cargo run -p ballista-sdk --example run_template
```

## Authoring with the builder

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

Declaration order matters: fixed accounts, then row accounts, then inputs, in the order callers
will pass them. `builder.build()` writes the header from the final counts; verify the result with
`ProgramView::parse(..).verify()` before uploading, exactly as the program will at finalize.

The builder does not apply the TypeScript compiler's pin lints. Pin program addresses and data
owners deliberately.

## Addresses and hashing

```rust
use ballista_sdk::{find_template_pda, find_template_pda_for_program, template_hash};

let (template, bump) = find_template_pda(&creator, template_id);
let (v3_template, _) = find_template_pda_for_program(&creator, template_id, &v3_program_id);
let hash = template_hash(&payload);
```

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

`RunInputs` encodes `bool`, `u64`, `i64`, `u128`, `pubkey`, and length-prefixed `bytes` in
declaration order. Runtime accounts follow the compiled schema order: fixed accounts first, then
each repeated row in field order.

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

`None` means the code came from an invoked program. See
[Errors and events](/guide/errors-and-events) for which kinds carry an account or input index
instead of a program counter.

## Borrowed decoding

```rust
use ballista_sdk::ballista_common::template::TemplateAccount;

let account = TemplateAccount::parse(&account_data)?;
let program = account.finalized_program()?;
let stats = program.verify()?;
```

The returned views borrow the original account bytes; they do not allocate an owned AST.

## Sharing compiler artifacts

A build pipeline can write `compiled.bytes` from TypeScript to a `.bvm` artifact. Rust services can
hash, upload, inspect, and run those exact bytes. The fixtures under `fixtures/` prove the wire
representation is identical, and the Mollusk suite runs the TypeScript-compiled fixtures against the
program.
