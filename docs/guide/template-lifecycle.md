# Template lifecycle

How to upload a template, in one instruction or in several, and what you can and cannot do with it
afterwards.

A template is stored in its own account. The account's address is a PDA (program-derived address:
an address owned by a program, with no private key) derived from the seeds
`['template', creator, templateId]`. `creator` is the uploader's address, and `templateId` is a
number from 0 to 65,535 that the creator picks. Once a template is finalized (checked and locked),
its account can never change.

## One-shot creation

When the whole template fits in one instruction, `CreateTemplate` uploads it, checks it, and
finalizes it in a single step. `planTemplateUpload` chooses this mode when the instruction's data is
at most 3,500 bytes; its `maxInstructionDataBytes` option changes that limit.

::: code-group

```ts [TypeScript]
const compiled = compileTemplate(template);
const plan = planTemplateUpload(compiled, 42);

if (plan.mode === 'oneShot') {
  await send(plan.instructions[0]);
}
```

```rust [Rust]
let payload = builder.build()?; // the template's bytes, from ProgramBuilder
let instruction = ballista_sdk::create_template_instruction(
    creator,
    42,
    &payload,
);
send(instruction).await?;
```

:::

## Chunked and resumable upload

A larger template is uploaded in pieces:

1. `BeginTemplate` creates the account and records the template's total length and its SHA-256
   hash.
2. `WriteTemplateChunk` instructions append the bytes. Each write must start exactly where the last
   one ended, at the byte count the account has recorded (`written_len`).
3. `FinalizeTemplate` checks that every byte arrived, that the bytes match the recorded hash, and
   that the template passes the program's checks. Then it locks the account.

`buildKitTemplateUploadPlan` sizes each write to fit the transaction message you pass it. If an
upload is interrupted, `resumeTemplateUpload` reads the account and plans only the remaining
writes.

::: code-group

```ts [TypeScript]
const plan = await buildKitTemplateUploadPlan({
  compiled,
  creator,
  templateId: 42,
  transactionMessage: baseV1Message,
});

for (const item of plan.instructions) await send(item.instruction);

// After an interrupted upload:
const resumed = resumeTemplateUpload(compiled, templateAccountBytes);
```

```rust [Rust]
let hash = ballista_sdk::template_hash(&payload);
send(ballista_sdk::begin_template_instruction(
    creator,
    42,
    payload.len() as u32,
    hash,
)).await?;

for (offset, chunk) in payload.chunks(3_000).enumerate() {
    send(ballista_sdk::write_template_chunk_instruction(
        creator,
        template,
        (offset * 3_000) as u32,
        chunk,
    )).await?;
}
send(ballista_sdk::finalize_template_instruction(creator, template)).await?;
```

:::

An upload that has not been finalized can be cancelled with `CancelTemplate`. That closes the
account and returns its rent deposit, in lamports (the smallest unit of SOL), to the creator. A
finalized template can never be changed or closed, and its rent deposit stays locked. To publish a
revision, upload it under a new template ID; see [Failure modes and recovery](/guide/failure-modes).
