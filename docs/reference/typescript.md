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
| `expression.accountData(account, offset, type)` | Read account data at a fixed or `u64` expression offset |
| `expression.returnData(type, offset)` | Read the previous unconditional invoke's return data |
| `step.require(condition, label?)` | Abort unless the boolean condition is true |
| `step.let(name, value, label?)` | Evaluate once and bind the result register lexically |
| `step.snapshot(name, value, label?)` | Semantic alias for `let`, intended for pre/post invariants |
| `step.assign(name, value, label?)` | Reassign a carried variable inside `forEach` |
| `step.invoke(descriptor)` | Emit a guarded generic CPI; `programAddress` names the intended program |
| `step.forEach(steps, { carry?, label? })` | Emit the one bounded account-row body |

Account constraints accept `signer`, `writable`, `executable`, `address`, `owner`,
`minDataLength`, and `unsafeUnpinned`. Templates accept `emitEvent` and a batch `minIterations`.

## Compile-time checks

The compiler rejects, with a message naming the account or step:

- an invoked or PDA program account without `address`, unless `unsafeUnpinned`;
- a data read from an account with neither `owner` nor `address`, unless `unsafeUnpinned`;
- a helper whose target program differs from the account's pinned address;
- `assign` outside a loop, to a variable not listed in `carry`, or with a different type;
- `returnData` anywhere except as the value of a `let` directly after an unconditional `invoke`;
- more than 64 registers, 128 instructions, or 64 expanded CPIs.

Static reads raise the account's `minDataLength` to cover the read.

## Compilation and inspection

```ts
const compiled = compileTemplate(template);

compiled.bytes;             // canonical payload
compiled.hash;              // SHA-256
compiled.inputOrder;
compiled.fixedAccountOrder;
compiled.batchAccountOrder;
compiled.stats;             // includes batchMinIterations and emitEvent
compiled.sourceMap;         // { pc, path, label? } per instruction

inspectTemplate(compiled.bytes);
decodeTemplateAccount(accountBytes);
```

## Errors

```ts
decodeBallistaError(code);            // { kind, name, context, source } or undefined
explainRunError(code, compiled);      // adds the step, account, or input the context points at
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

The builder rejects missing or extra bindings, fixed-address mismatches, too many or too few rows,
and input values that do not fit their declared types.

## Protocol helpers

- `systemTransfer`
- `tokenTransfer`
- `createAssociatedTokenAccount`
- `ensureAssociatedTokenAccount`
- `assertPda`
- `assertAta` / `assertAssociatedTokenAccount`

Every helper compiles to generic VM records and declares the program it targets. The byte constants
`SYSTEM_PROGRAM_ADDRESS_BYTES`, `TOKEN_PROGRAM_ADDRESS_BYTES`, and
`ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES` are exported for account pins.

## Solana Kit adapter

Import from `@jac0xb/ballista/kit`:

```ts
getTemplateAddress(creator, templateId, programAddress?);
findFreeTemplateId({ rpc, creator, start? });
buildKitRunInstruction(input);
buildKitTemplateUploadPlan(input);
buildKitResumeTemplateUploadPlan(input);
measureTransactionMessage(message);
createComputeUnitProvider({ rpc, marginBps: 1_000 });
getComputeUnitsConsumed(confirmedTransaction);
```

`findFreeTemplateId` probes template addresses in batches and returns the first ID with no account,
since an address that already holds a template cannot be reused.
