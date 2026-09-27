# TypeScript SDK

The `@jac0xb/ballista` package defines, compiles, and inspects templates, and encodes the
instructions that upload and run them. The core package has no RPC dependency: it works on
bytes and plain objects. The Solana Kit adapter, `@jac0xb/ballista/kit`, turns those into Solana
Kit instructions, derives addresses, and talks to an RPC node. Import it only when you need it.

## Authoring

| API | Purpose |
| --- | --- |
| `defineTemplate(document)` | Validate a template document with Zod (a TypeScript schema library) and fill in defaults |
| `account.fixed(name)` | Refer to a fixed account, passed once per run |
| `account.iteration(name)` | Refer to an account in the current batch row, inside `forEach` only |
| `expression.*` | Build literals, reads, arithmetic, comparisons, casts, selects, and program-derived addresses (PDAs); see [Template language](/reference/language) |
| `expression.accountData(account, offset, type)` | Read account data at a fixed offset or at a `u64` expression offset |
| `expression.returnData(type, offset?)` | Read the return data of the invoke just before; `offset` defaults to 0 |
| `expression.rowInput(name)` | Read a row input of the current batch row |
| `step.require(condition, label?)` | Fail the run unless the condition is true |
| `step.let(name, value, label?)` | Evaluate a value once and bind it to a name |
| `step.snapshot(name, value, label?)` | Same as `let`; the name suits before-and-after checks |
| `step.assign(name, value, label?)` | Reassign a carried variable inside `forEach` |
| `step.invoke(descriptor)` | Perform a CPI (a call to another program); `when` makes it conditional, `programAddress` names the intended program, and `accountGroup` forwards an account group |
| `step.forEach(steps, { carry?, label? })` | Run the steps once per batch row; at most one per template, at the top level |
| `data.literal(bytes)`, `data.encode(encoding, value)` | Build a CPI's instruction data from literal bytes and encoded values |

A template document has these fields:

| Field | Meaning |
| --- | --- |
| `inputs` | Named inputs and their types; a `bytes` input also takes `maxLength` (1 to 1,024) |
| `accounts` | Named fixed accounts and their constraints |
| `batch` | Optional: `maxIterations` (1 to 60), `minIterations` (default 0), `row` (1 to 8 named accounts with constraints), and `rowInputs` (up to 8 named inputs) |
| `accountGroups` | Up to 8 names of caller-sized account lists; see [Account groups](/guide/account-groups) |
| `emitEvent` | Log a run event after each successful run; default `false` |
| `steps` | 1 to 128 steps |
| `version` | Optional; must be `1` |

An account constraint accepts `signer`, `writable`, and `executable` (default `false`); `address`
and `owner` (32-byte `Uint8Array`s that pin the account's address or its owner program);
`minDataLength` (default 0); and `unsafeUnpinned` (default `false`), which waives the pinning
rules below.

## Compile-time checks

`compileTemplate` fails with an error that describes the problem when it finds:

- an invoked program, or a program used to derive a PDA, that is not marked `executable`, or that
  does not pin its `address` and is not `unsafeUnpinned`;
- a data read from an account that pins neither `owner` nor `address` and is not
  `unsafeUnpinned`;
- an invoke whose `programAddress` differs from the address its program account pins (the CPI
  helpers below set `programAddress`);
- an invoke that asks for signer or writable on an account whose constraint does not allow it;
- `assign` outside a loop, to a variable not listed in `carry`, or with a different type or size;
- `returnData` anywhere except as the value of a `let` directly after an invoke with no guard;
- more than 64 registers (the numbered slots that hold values during a run), 128 bytecode
  instructions, 64 CPIs per run (counting each loop iteration), or 10,240 bytes of bytecode;
- CPI instruction data that could exceed 4,096 bytes, or a PDA seed that could exceed 32 bytes.

`defineTemplate` and `compileTemplate` also reject anything the document schema forbids, such as
more than 120 runtime accounts or a `forEach` inside another. Reads at a fixed offset raise the
account's `minDataLength` to cover the read.

## Compilation and inspection

```ts
const compiled = compileTemplate(template);

compiled.bytes;             // bytecode to store on chain
compiled.hash;              // SHA-256
compiled.inputOrder;
compiled.fixedAccountOrder;
compiled.batchAccountOrder;
compiled.stats;             // counts and sizes
compiled.sourceMap;         // { pc, path, label? } per instruction

inspectTemplate(compiled.bytes);
decodeTemplateAccount(accountBytes);
```

`inputOrder`, `fixedAccountOrder`, and `batchAccountOrder` give the order in which callers pass
inputs and accounts. `rowInputOrder` and `accountGroupOrder` do the same for row inputs and
account groups. `sourceMap` links each bytecode instruction's index (`pc`, the program counter
reported in run errors) to the step that produced it, as a path such as `steps[1].steps[0]`, with
the step's label if it has one.

`inspectTemplate(bytes)` reads any compiled payload and returns the same stats.
`decodeTemplateAccount(accountBytes)` decodes a template account: its state (0 uploading,
1 finalized), creator, template ID, lengths, hash, and payload.

## Errors

```ts
decodeBallistaError(code);            // { kind, name, context, source } or undefined
explainRunError(code, compiled);      // adds the step, account, or input the context points at
```

The decoded error also carries the original `code`, and `source` is `'runtime'` or `'verifier'`.
`decodeBallistaError` returns `undefined` for a code outside Ballista's ranges, which comes from an
invoked program. See [Errors and events](/guide/errors-and-events).

## Lifecycle

A template lives at a PDA of the Ballista program, derived from its creator's address and a 16-bit
template ID. A small template is created in one instruction. A larger one is uploaded in chunks and
then finalized, which makes it permanent and runnable. These functions return instruction data
bytes; the Kit adapter below wraps them into instructions with the right accounts.

```ts
planTemplateUpload(compiled, templateId);
resumeTemplateUpload(compiled, decodedAccount);
encodeCreateTemplate(compiled, templateId);
encodeBeginTemplate(compiled, templateId);
encodeWriteTemplateChunk(offset, bytes);
encodeFinalizeTemplate();
encodeCancelTemplate();
```

| Function | Produces |
| --- | --- |
| `planTemplateUpload` | A complete upload: one create instruction if its data fits in 3,500 bytes (change this with `maxInstructionDataBytes`), otherwise begin, write chunks, and finalize |
| `resumeTemplateUpload` | The remaining chunks and the finalize step for an upload that stopped partway, after checking that the bytes already written match |
| `encodeCreateTemplate` | Create, verify, and finalize a template in one instruction |
| `encodeBeginTemplate` | Start a chunked upload: create the account with the payload's length and SHA-256 hash |
| `encodeWriteTemplateChunk` | Write the next chunk; chunks must be written in order |
| `encodeFinalizeTemplate` | Check the hash, verify the bytecode, and make the template permanent |
| `encodeCancelTemplate` | Close an unfinished upload and return the SOL it holds to the creator |

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

`accounts` maps each fixed account's name to `{ address }`, and `batchRows` holds one such record
per row. Signer and writable flags come from the template's constraints. When the template declares
row inputs, also pass `batchInputs`, one record of row input values per row. When it declares
account groups, pass `accountGroups`: a list of `{ address, writable? }` members for each group
name. The result holds the program address, the account list (the template account first, then
the runtime accounts), and the instruction data.

`buildRunInstruction` rejects missing or unknown account names, an address that differs from the
account's pinned address, too many or too few rows, input values that do not fit their types, more
than 120 runtime accounts, and run data over 1,024 bytes.

## Protocol helpers

| Helper | Step it returns |
| --- | --- |
| `systemTransfer` | A System Program transfer of `lamports` (the smallest unit of SOL) from `from` to `to` |
| `tokenTransfer` | An SPL Token transfer of `amount` from `source` to `destination`, signed by `authority` |
| `createAssociatedTokenAccount` | A call to the Associated Token Account program that creates `associatedTokenAccount` |
| `ensureAssociatedTokenAccount` | The same create, run only when the account holds no data |
| `assertPda` | A requirement that an account is the PDA for the given seeds and program |
| `assertAta` / `assertAssociatedTokenAccount` | A requirement that an account is the associated token account (ATA) for an owner, mint, and token program |

Each helper takes one object naming the accounts and values involved, for example
`systemTransfer({ systemProgram, from, to, lamports })`, and returns an ordinary step. The CPI
helpers also accept `when` and `label`; the assertions accept `bump` and `label`. The CPI helpers
set `programAddress`, so compilation fails if the program account pins a different program.

The byte constants `SYSTEM_PROGRAM_ADDRESS_BYTES`, `TOKEN_PROGRAM_ADDRESS_BYTES`, and
`ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES` are exported for pinning those programs.

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

| Function | Purpose |
| --- | --- |
| `getTemplateAddress` | The template's address and bump |
| `findFreeTemplateId` | The lowest template ID at or above `start` whose address holds no account. An address that already holds a template cannot be reused |
| `buildKitRunInstruction` | `buildRunInstruction` with Kit addresses, returning a Kit `Instruction` |
| `buildKitTemplateUploadPlan` | An upload plan made of Kit instructions; given a `transactionMessage`, it sizes the chunks to fit that message |
| `buildKitResumeTemplateUploadPlan` | The same, for resuming an upload |
| `measureTransactionMessage` | A message's size, the size limit for its version, and whether it fits |
| `createComputeUnitProvider` | A provider whose `estimateAndSet(message)` simulates the message and sets its compute-unit limit (the execution budget the transaction requests) to the measured amount plus a margin in basis points (1,000 is 10%, the default). For version 1 messages it also sets the loaded-account-data limit |
| `getComputeUnitsConsumed` | The compute units a confirmed transaction used, read from its metadata |

`buildKitCancelTemplateInstruction({ creator, templateId })` builds the instruction that cancels an
unfinished upload.
