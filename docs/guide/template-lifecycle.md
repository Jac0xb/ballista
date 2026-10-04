# Template lifecycle

How to upload a template in one instruction or in several, resume an upload that stopped, and what
you can and cannot do with a template afterwards. The code continues
[Getting started](/guide/getting-started), and uses its `send`, `creator` and imports: in
TypeScript also `emptyMessage` and a `compiled` template, and in Rust a `payload`.

A template is stored in its own account, at a [PDA](/reference/glossary#pda) derived from the seeds
`['template', creator, templateId]`. `creator` is the uploader's address, and `templateId` is a
number from 0 to 65,535 that the creator picks. Once a template is
[finalized](/reference/glossary#finalize), checked and locked, its account can never change.

## Upload

`CreateTemplate` uploads a template, checks it, and finalizes it in one instruction, when the
template fits in one transaction. A larger template is uploaded in pieces:

1. `BeginTemplate` creates the account and records the template's total length and its SHA-256
   hash.
2. `WriteTemplateChunk` instructions append the bytes. Each write must start exactly where the last
   one ended, at the byte count the account has recorded (`written_len`).
3. `FinalizeTemplate` checks that every byte arrived, that the bytes match the recorded hash, and
   that the template passes the program's checks. Then it locks the account.

Each instruction goes in a transaction of its own, which holds at most 1,232 bytes. In TypeScript,
pass `buildKitTemplateUploadPlan` the transaction you send them in; it picks one instruction or
pieces, and sizes each write to fit. In Rust you choose: a legacy transaction fits a
`CreateTemplate` for a template of up to 960 bytes, and a write of up to 1,023 bytes.
[Limits](/reference/limits#transaction-ceilings) has the sizes.

::: code-group

<<< @/../clients/js/examples/start/template-lifecycle.ts#upload [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#chunked [Rust]

:::

## Resume an interrupted upload

If an upload stops partway, the account keeps the bytes written so far and records how many. Send
only the missing writes, then the finalize:

::: code-group

<<< @/../clients/js/examples/start/template-lifecycle.ts#resume [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#resume [Rust]

:::

## Cancel or replace

An upload that has not been finalized can be cancelled with `CancelTemplate`
(`buildKitCancelTemplateInstruction` in TypeScript, `cancel_template_instruction` in Rust). That
closes the account and returns its rent deposit to the creator. A finalized template can never be
changed or closed, and its rent deposit stays locked. To publish a revision, upload it under a new
template ID; see [Failure modes and recovery](/guide/failure-modes).
