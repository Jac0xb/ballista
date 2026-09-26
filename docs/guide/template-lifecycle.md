# Template lifecycle

Templates are immutable PDA accounts derived from `['template', creator, templateId]`.

## One-shot creation

::: code-group

```ts [TypeScript]
const compiled = compileTemplate(template);
const plan = planTemplateUpload(compiled, 42);

if (plan.mode === 'oneShot') {
  await send(plan.instructions[0]);
}
```

```rust [Rust]
let payload = std::fs::read("artifacts/template.bvm")?;
let instruction = ballista_sdk::create_template_instruction(
    creator,
    42,
    &payload,
);
send(instruction).await?;
```

:::

## Chunked and resumable upload

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

Writes must append at exactly `written_len`. Finalization checks the declared length, SHA-256 hash,
and VM structure. An uploading account can be cancelled to recover rent; a finalized template can
never be changed or closed. Publish a revision under a new template ID.
