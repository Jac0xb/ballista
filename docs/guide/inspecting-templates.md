# Inspecting a template

Before you run a template someone else uploaded, read what it will do. A finalized template cannot
change, so what you read is what Ballista runs. That covers Ballista's part only: the programs the
template calls can be upgraded by their own authorities, and so can the pre-release devnet build of
Ballista itself.

## 1. Find and fetch it

A template's address is a [PDA](/reference/glossary#pda) derived from
`['template', creator, templateId]`. Derive it with `getTemplateAddress(creator, templateId)` in
TypeScript (`@jac0xb/ballista/kit`) or `find_template_pda(&creator, template_id)` in Rust, then
fetch the account over RPC.

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

<<< @/../clients/rust/examples/docs_security.rs#inspect-template [Rust]

:::

`inspectTemplate` returns counts only. `ProgramView` in Rust gives every table: account
declarations, inputs, compiled instructions, calls, and their data. The Rust tab prints each
account's requirements and the signers each call passes. The byte layout is on
[Wire format](/reference/wire-format).

## What to look for

- **Signers and writable accounts.** A declared account's flags are the most any call can ask of
  it, but one signer declaration lets every call pass it as a signer, so check each call that
  does. [Account group](/guide/account-groups) members have no declaration: a call passes each one
  as writable whenever the transaction marks it writable.
- **Called programs.** Each call names a program account. Check that the account pins an address
  you recognise, and that you trust whoever can upgrade that program. The Ballista program does
  not require the pin; only the TypeScript compiler does.
- **CPI data built from inputs.** A call whose data comes from an input sends whatever the caller
  encodes. With a signer passed to it, that can be any instruction the called program accepts from
  that signer.
- **Types and identities** of accounts whose data is read: an owner pin alone doesn't fix either.
  See [Pins](/guide/trust-model#pins).
- **Distinct accounts.** Where two slots must hold different accounts, look for a `require` that
  their keys differ. See [Aliased accounts](/guide/trust-model#aliased-accounts).
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
