---
layout: home
title: Ballista — execution, composed
titleTemplate: false
description: Ballista stores a sequence of Solana program calls, and the checks between them, as a template on chain. Anyone can run it later with new inputs.
---

## One example, start to finish

Send everything above a minimum balance to another account. The amount isn't known until the
transaction runs, so the template reads the balance, checks it, and sends the difference.

::: code-group

```ts [TypeScript · Template]
// Describe the template once: what it takes as input, which accounts it expects, and its steps.
// compileTemplate turns it into the bytes you upload to the chain.
import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  step,
  systemTransfer,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
} from '@jac0xb/ballista';

const sweep = defineTemplate({
  // The caller picks the minimum balance to leave behind.
  inputs: { reserve: { type: 'u64' } },
  // The accounts every run must pass, and what each one has to be.
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
    destination: { writable: true },
  },
  steps: [
    // Read the vault's balance while the transaction runs.
    step.let('balance', expression.accountField(account.fixed('vault'), 'lamports')),
    // Stop, and change nothing, unless the balance is above the minimum.
    step.require(
      expression.greaterThan(expression.variable('balance'), expression.input('reserve')),
      'aboveReserve',
    ),
    // Send everything above the minimum.
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('destination'),
      lamports: expression.subtract(expression.variable('balance'), expression.input('reserve')),
    }),
  ],
});

// Upload these bytes once. After that, anyone can run the template.
const compiled = compileTemplate(sweep);
```

```ts [TypeScript · Run]
// Build the instruction that runs the uploaded template. Add it to a transaction, sign, and send.
import {
  BALLISTA_ADDRESS,
  SYSTEM_PROGRAM_ADDRESS,
  buildKitRunInstruction,
  getTemplateAddress,
} from '@jac0xb/ballista/kit';

// creator, vault and destination are addresses you supply.
const [templateAddress] = await getTemplateAddress(creator, 7);

// The caller chooses the minimum, never the amount.
const run = buildKitRunInstruction({
  compiled,
  programAddress: BALLISTA_ADDRESS,
  templateAddress,
  inputs: { reserve: 2_000_000n },
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    vault: { address: vault },
    destination: { address: destination },
  },
});
```

```rust [Rust · Template]
// The same template, written with the Rust SDK. It is lower level: you declare accounts and inputs
// in order, and each step's result goes in a numbered register that later steps read.
use ballista_sdk::{
    ballista_common::template::{
        ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, OP_GT, OP_SUB,
        VALUE_U64,
    },
    ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
};

let mut builder = ProgramBuilder::new();
// The accounts every run must pass, in order, and the caller's minimum balance.
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let vault = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
let reserve_input = builder.input(VALUE_U64, 0);

// Read the balance while the transaction runs, and stop unless it is above the minimum.
let reserve = builder.load_input(reserve_input);
let balance = builder.account_lamports(vault);
let above = builder.binary(OP_GT, balance, reserve);
builder.require(above);
// Send everything above the minimum with a System Program transfer.
let amount = builder.binary(OP_SUB, balance, reserve);
let discriminator = builder.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
let transfer = builder.cpi(
    system,
    &[
        (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
        (destination, ACCOUNT_WRITABLE),
    ],
    &[
        Segment::Literal(discriminator),
        Segment::Register(DATA_REG_U64, amount),
    ],
);
builder.invoke(transfer, None);

// The same bytes the TypeScript compiler produces.
let payload = builder.build()?;
```

```rust [Rust · Run]
// Build the instruction that runs the uploaded template. Add it to a transaction, sign, and send.
use ballista_sdk::{find_template_pda, run_instruction, RunInputs};
use solana_program::instruction::AccountMeta;

// creator_pubkey, vault_pubkey and destination_pubkey are addresses you supply.
let (template, _) = find_template_pda(&creator_pubkey, 7);

// The caller chooses the minimum, never the amount.
let run = run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(ballista_sdk::SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(vault_pubkey, true),
        AccountMeta::new(destination_pubkey, false),
    ],
    &RunInputs::new().u64(2_000_000).finish(),
);
```

:::

A regular transaction can't do this, because it has to name a fixed amount when you sign it. The
[getting started guide](/guide/getting-started) walks through writing, uploading, and running a
template step by step.
