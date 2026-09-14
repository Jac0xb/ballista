# TypeScript SDK

The core package has no RPC dependency. Import the Solana Kit adapter only when constructing native
instructions or transaction messages.

## Authoring

| API | Purpose |
| --- | --- |
| `defineTemplate(document)` | Zod-validate named inputs, accounts, batch schema, and steps |
| `account.fixed(name)` | Reference a fixed runtime account |
| `account.iteration(name)` | Reference a field in the current batch row |
| `expression.*` | Build typed literals, reads, math, booleans, casts, selects, and PDA derivations |
| `step.require(condition)` | Abort unless the boolean condition is true |
| `step.let(name, value)` | Evaluate once and bind the result register lexically |
| `step.snapshot(name, value)` | Semantic alias for `let`, intended for pre/post invariants |
| `step.invoke(descriptor)` | Emit a guarded generic CPI |
| `step.forEach(steps)` | Emit the one bounded account-row body |

## Compilation and inspection

```ts
const compiled = compileTemplate(template);

compiled.bytes;             // canonical payload
compiled.hash;              // SHA-256
compiled.inputOrder;
compiled.fixedAccountOrder;
compiled.batchAccountOrder;
compiled.stats;

inspectTemplate(compiled.bytes);
decodeTemplateAccount(accountBytes);
```

## Lifecycle

```ts
planTemplateUpload(compiled, templateId);
resumeTemplateUpload(compiled, decodedAccount);
encodeCreateTemplate(compiled, templateId);
encodeBeginTemplate(compiled, templateId);
encodeWriteTemplateChunk(offset, bytes);
encodeFinalizeTemplate();
encodeCancelTemplate();
```

## Run construction

```ts
const instruction = buildRunInstruction({
  compiled,
  programAddress,
  templateAddress,
  inputs,
  accounts,
  batchRows,
});
```

The builder rejects missing or extra bindings, fixed-address mismatches, excessive rows, and input
values that do not fit their declared types.

## Protocol helpers

- `systemTransfer`
- `tokenTransfer`
- `createAssociatedTokenAccount`
- `ensureAssociatedTokenAccount`
- `assertPda`
- `assertAta` / `assertAssociatedTokenAccount`

Every helper compiles to generic VM records. None adds a protocol-specific runtime evaluator.

## Solana Kit adapter

Import from `@jac0xb/ballista/kit`:

```ts
getTemplateAddress(creator, templateId);
buildKitRunInstruction(input);
buildKitTemplateUploadPlan(input);
buildKitResumeTemplateUploadPlan(input);
measureTransactionMessage(message);
createComputeUnitProvider({ rpc, marginBps: 1_000 });
getComputeUnitsConsumed(confirmedTransaction);
```
