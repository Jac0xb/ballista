# Getting started

In this walkthrough you install the repository, write a template that transfers SOL, plan its
upload, build the instruction that runs it, and decode a failed run. Ballista has not been released
yet: until its packages are published to npm and crates.io, use the ones in this repository.

## Install the workspace

```bash
git clone https://github.com/Jac0xb/ballista.git
cd ballista
corepack enable
pnpm install
pnpm test
```

You need Node.js 22 or later, pnpm 11.24, and Rust. The local tests for version 1 transactions also
need Solana CLI 4.2 or later.

## 1. Define and compile

A template declares its inputs, the accounts it expects, and its steps. This one takes an amount
and transfers that many lamports (the smallest unit of SOL: one SOL is 1,000,000,000 lamports) from
a sender to a recipient.

```ts
import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  systemTransfer,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
} from '@jac0xb/ballista';

const template = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    sender: { signer: true, writable: true },
    recipient: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('sender'),
      to: account.fixed('recipient'),
      lamports: expression.input('amount'),
    }),
  ],
});

const compiled = compileTemplate(template);
console.log(compiled.hash, compiled.stats);
```

Each account entry says what the caller's account must be. `signer: true` means it must sign the
transaction, `writable: true` means the transaction must allow it to change, `executable: true`
means it must be a program, and `address` fixes it to one exact address.

Compilation is deterministic: the same template always produces the same bytes and the same hash.
Account and input names exist only in your code; the compiled template refers to accounts and
inputs by position.

## 2. Plan an upload

A template is stored in its own account, at a PDA (program-derived address: an address owned by a
program, with no private key). The address is computed from the creator's address and a template
ID the creator picks, 7 in this example.

::: code-group

```ts [TypeScript]
const upload = planTemplateUpload(compiled, 7);

for (const instruction of upload.instructions) {
  console.log(instruction.kind, instruction.data.length);
}
```

```rust [Rust]
use ballista_sdk::{create_template_instruction, find_template_pda};

let payload = std::fs::read("artifacts/sol-transfer.bvm")?;
let (template, _) = find_template_pda(&creator, 7);
let create = create_template_instruction(creator, 7, &payload);
```

:::

A template small enough to fit in one instruction is uploaded, checked, and finalized by a single
`CreateTemplate` instruction. A larger template is uploaded in steps: one instruction creates the
account, write instructions append the bytes in order, and a finalize instruction checks the whole
template and locks it. If an upload is interrupted, the SDK resumes from the number of bytes the
account has recorded (`written_len`). [Template lifecycle](/guide/template-lifecycle) covers both
paths.

## 3. Build a run

::: code-group

```ts [TypeScript]
const run = buildRunInstruction({
  compiled,
  programAddress: BALLISTA_BYTES,
  templateAddress: TEMPLATE_BYTES,
  inputs: { amount: 50_000_000n },
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    sender: { address: SENDER_BYTES },
    recipient: { address: RECIPIENT_BYTES },
  },
});
```

```rust [Rust]
let mut inputs = Vec::new();
inputs.extend_from_slice(&50_000_000u64.to_le_bytes());

let run = ballista_sdk::run_instruction(
    template,
    runtime_accounts,
    &inputs,
);
```

:::

Anyone can run a finalized template; the creator does not need to sign. Every account the template
declares as a signer must sign the outer transaction.

After the template's own account, which both SDKs add for you, the run instruction lists accounts
in the order the template declares them. The template's fixed accounts (those declared in
`accounts`) come first, in declaration order. If the template has a [batch](/guide/batching), each
row's accounts follow, and then the members of any [account groups](/guide/account-groups). Inputs
are encoded in declaration order with no padding.

The TypeScript builder takes accounts and inputs by name, puts them in order, and checks them
against the compiled template. In Rust you pass them already in order, so a template's author
should publish that order; `compiled.fixedAccountOrder` and `compiled.inputOrder` list it.
`RunInputs::new().u64(50_000_000).finish()` builds the same bytes as the `to_le_bytes` call above.

## 4. Read a failed run

When a run fails, Ballista returns a custom error code that names the failure and where it happened.
Both SDKs decode it:

::: code-group

```ts [TypeScript]
import { explainRunError } from '@jac0xb/ballista';

// After a failed simulation:
explainRunError(customErrorCode, compiled)?.message;
// 'RequirementFailed at steps[0] (paySender)'
```

```rust [Rust]
use ballista_sdk::decode_ballista_error;

if let Some(error) = decode_ballista_error(custom_error_code) {
    println!("{} at instruction {}", error.name, error.context);
}
```

:::

[Errors and events](/guide/errors-and-events) explains how the code is laid out.

## Author it in Rust

You can also build a template in Rust with `ProgramBuilder`. It works at a lower level than the
TypeScript compiler: you declare accounts and inputs in order and pass values between steps through
registers, numbered slots that hold values during a run. For the transfer template above, the
builder's output is byte-identical to the compiler's, and the repository tests this against
`fixtures/system-transfer.hex`.

The TypeScript compiler also checks that every program the template calls has a fixed address and
that every account whose data it reads has a fixed owner or address. It works out each account's
minimum data length, and it records which step produced each instruction so that errors can name
the step. With the Rust builder, those decisions are yours. A template built either way can be
uploaded and run from either language.

```rust
use ballista_sdk::{
    ballista_common::template::{
        ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, VALUE_U64,
    },
    ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
};

let mut builder = ProgramBuilder::new();
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let sender = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let recipient = builder.account(ACCOUNT_WRITABLE, None, None, 0);
let amount_input = builder.input(VALUE_U64, 0);

let amount = builder.load_input(amount_input);
let discriminator = builder.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
let transfer = builder.cpi(
    system,
    &[
        (sender, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
        (recipient, ACCOUNT_WRITABLE),
    ],
    &[
        Segment::Literal(discriminator),
        Segment::Register(DATA_REG_U64, amount),
    ],
);
builder.invoke(transfer, None);

let payload = builder.build()?;
let create = ballista_sdk::create_template_instruction(creator, 7, &payload);
```

## Run the shipped examples

```bash
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
