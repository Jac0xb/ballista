# Inspecting a template

Before you run a template someone else uploaded, read what it will do. A finalized template cannot
change, so what you read is what runs.

## 1. Find and fetch it

A template's address is a PDA derived from `['template', creator, templateId]`. Derive it with
`getTemplateAddress(creator, templateId)` in TypeScript (`@jac0xb/ballista/kit`) or
`find_template_pda(&creator, template_id)` in Rust, then fetch the account over RPC.

Check that the account is owned by the Ballista program you expect. Templates belong to the
deployment that finalized them.

## 2. Compare it with the source

The strongest check: get the template's source, compile it yourself, and compare the result with
what is stored. The account records the SHA-256 hash of its payload.

```ts
import { compileTemplate, decodeTemplateAccount } from '@jac0xb/ballista';

const stored = decodeTemplateAccount(accountBytes);
const compiled = compileTemplate(sourceTemplate);

const same =
  stored.state === 1 && // finalized
  stored.payload.length === compiled.bytes.length &&
  stored.payload.every((byte, i) => byte === compiled.bytes[i]);
```

In Rust, `ballista_sdk::template_hash(&payload)` gives the same hash as `compiled.hash`.

## 3. Read it without the source

Without the source, you can still decode the stored bytes.

::: code-group

```ts [TypeScript]
import { decodeTemplateAccount, inspectTemplate } from '@jac0xb/ballista';

const stored = decodeTemplateAccount(accountBytes);
console.log(stored.creator, stored.templateId, stored.state);
console.log(inspectTemplate(stored.payload));
// { fixedAccounts, batchMaxIterations, inputs, cpis, maxExpandedCpis, ... }
```

```rust [Rust]
use ballista_sdk::ballista_common::template::{
    TemplateAccount, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, NO_INDEX,
};

let account = TemplateAccount::parse(&account_data).expect("not a template account");
let program = account.finalized_program().expect("not finalized");
let stats = program.verify()?; // the same checks the program ran at finalize

for (index, constraint) in program.accounts.iter().enumerate() {
    let pinned = |i: u8| (i != NO_INDEX).then(|| program.pubkeys[i as usize].bytes);
    println!(
        "account {index}: signer={} writable={} address={:?} owner={:?}",
        constraint.flags & ACCOUNT_SIGNER != 0,
        constraint.flags & ACCOUNT_WRITABLE != 0,
        pinned(constraint.address_index),
        pinned(constraint.owner_index),
    );
}
for cpi in program.cpis {
    println!("calls the program in account {}", cpi.program_account);
}
```

:::

`inspectTemplate` returns counts only. `ProgramView` in Rust gives every table: account
declarations, inputs, compiled instructions, calls, and their data. The byte layout is on
[Wire format](/reference/wire-format).

## What to look for

- **Signers and writable accounts.** A template can never do more with an account than its
  declaration allows. These flags are the most it can ask of your wallet.
- **Called programs.** Each call names a program account. Check that account pins an address you
  recognise. The program does not require this pin; only the TypeScript compiler does.
- **Pinned owners** on accounts whose data is read, so byte offsets mean what the template assumes.
- **Worst case.** `maxExpandedCpis` and the maximum batch rows say how much one run can do.

## What is missing

- **No disassembler.** Nothing turns the compiled instructions back into readable steps. Reading
  them means following the opcodes on [Wire format](/reference/wire-format).
- **No names on chain.** Input, account and step names, and `require` labels, are not stored. You
  see positions, not names, unless you have the source.
- **No source registry.** Nothing links a template address to its source or proves a build. Ask
  the creator for the source and compare as in step 2.
- **No one-call fetch.** Neither SDK fetches and decodes a template in one call; fetch the account
  yourself.
