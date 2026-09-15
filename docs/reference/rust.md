# Rust SDK

The Rust client intentionally provides shared parsing plus small lifecycle and execution codecs. It
does not currently duplicate the Zod authoring compiler.

```toml
[dependencies]
ballista-sdk = { path = "clients/rust" }
```

## Addresses and hashing

```rust
use ballista_sdk::{find_template_pda, template_hash, ID};

let (template, bump) = find_template_pda(&creator, template_id);
let hash = template_hash(&compiled_payload);
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
let write = ballista_sdk::write_template_chunk_instruction(
    creator,
    template,
    offset,
    chunk,
);
let finalize = ballista_sdk::finalize_template_instruction(creator, template);
let cancel = ballista_sdk::cancel_template_instruction(creator, template);
```

## Run instruction

```rust
use solana_program::instruction::AccountMeta;

let run = ballista_sdk::run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(program, false),
        AccountMeta::new(source, false),
        AccountMeta::new(destination, false),
        AccountMeta::new_readonly(authority, true),
    ],
    &input_bytes,
);
```

Runtime accounts must follow the compiled schema order: fixed accounts first, then each repeated row
in field order.

## Borrowed decoding

```rust
use ballista_sdk::ballista_common::template::TemplateAccount;
use zerocopy::IntoBytes;

let account = TemplateAccount::parse(&account_data)?;
let program = account.finalized_program()?;
let stats = program.verify()?;

assert!(core::ptr::eq(
    program.header.as_bytes().as_ptr(),
    account.payload().as_ptr(),
));
```

The returned views borrow the original account bytes; they do not allocate an owned AST.

## Sharing compiler artifacts

A build pipeline can write `compiled.bytes` from TypeScript to a `.bvm` artifact. Rust services can
hash, upload, inspect, and run those exact bytes. The cross-language fixtures in the repository
prove the wire representation is identical.
