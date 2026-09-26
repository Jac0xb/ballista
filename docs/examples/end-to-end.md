# Author and run in both languages

The same SOL transfer template, four ways. The TypeScript compiler and the Rust builder produce
byte-identical bytecode for it, which the repository checks against `fixtures/system-transfer.hex`.

::: code-group

```ts [TypeScript · template]
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  defineTemplate,
  expression,
  systemTransfer,
} from '@jac0xb/ballista';

export const transfer = defineTemplate({
  inputs: { lamports: { type: 'u64' } },
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
      lamports: expression.input('lamports'),
      label: 'paySender',
    }),
  ],
});

export const compiled = compileTemplate(transfer);
// compiled.bytes, compiled.hash, compiled.sourceMap
```

```ts [TypeScript · run]
import { explainRunError } from '@jac0xb/ballista';
import {
  BALLISTA_ADDRESS,
  SYSTEM_PROGRAM_ADDRESS,
  buildKitRunInstruction,
  getTemplateAddress,
} from '@jac0xb/ballista/kit';

const [templateAddress] = await getTemplateAddress(creator, 0);
const instruction = buildKitRunInstruction({
  compiled,
  programAddress: BALLISTA_ADDRESS,
  templateAddress,
  inputs: { lamports: 55_000n },
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    sender: { address: sender },
    recipient: { address: recipient },
  },
});

// After a failed simulation:
explainRunError(customErrorCode, compiled)?.message;
// 'RequirementFailed at steps[0] (paySender)'
```

```rust [Rust · template]
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
let lamports_input = builder.input(VALUE_U64, 0);

let lamports = builder.load_input(lamports_input);
let discriminator = builder.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
let transfer = builder.cpi(
    system,
    &[
        (sender, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
        (recipient, ACCOUNT_WRITABLE),
    ],
    &[
        Segment::Literal(discriminator),
        Segment::Register(DATA_REG_U64, lamports),
    ],
);
builder.invoke(transfer, None);

let payload = builder.build()?;
let create = ballista_sdk::create_template_instruction(creator, 0, &payload);
```

```rust [Rust · inputs and run]
use ballista_sdk::{
    decode_ballista_error, find_template_pda, run_instruction, RunInputs, SYSTEM_PROGRAM_ID,
};
use solana_program::instruction::AccountMeta;

let (template, _) = find_template_pda(&creator, 0);
let inputs = RunInputs::new().u64(55_000).finish();
let run = run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(sender, true),
        AccountMeta::new(recipient, false),
    ],
    &inputs,
);

// After a failed simulation:
if let Some(error) = decode_ballista_error(custom_error_code) {
    println!("{} at instruction {}", error.name, error.context);
}
```

:::

## Run the shipped examples

```bash
pnpm --dir clients/js exec tsx examples/transfer.ts        # compile and print the payload
pnpm --dir clients/js exec tsx examples/run-transfer.ts    # build a Kit run instruction offline
cargo run -p ballista-sdk --example author_template        # build two templates in Rust
cargo run -p ballista-sdk --example run_template           # encode inputs and decode errors in Rust
```

The Rust `author_template` example also builds a budgeted payroll that carries a running total
across rows; see [Batch execution](/guide/batching) for the TypeScript equivalent.

## Account and input order

Both runners must pass accounts in schema order: fixed accounts first, in declaration order, then
every batch row in field order. Inputs are encoded in declaration order with no padding. The
TypeScript run builder checks names against the compiled template; the Rust side relies on the
template author documenting the order, which `compiled.fixedAccountOrder` and `compiled.inputOrder`
provide.
