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
| `account.registry(registry, { key?, payer })` | Declare a fixed account that holds a registry entry; see [Registries](#registries) |
| `account.systemProgram()` | Declare the System Program, pinned to its address |
| `expression.*` | Build literals, reads, arithmetic, comparisons, casts, selects, and program-derived addresses (PDAs); see [Template language](/reference/language) and the sections below |
| `expression.accountData(account, offset, type)` | Read account data at a fixed offset or at a `u64` expression offset |
| `expression.accountKey(name)` | Read a fixed account's address; short for `accountField(account.fixed(name), 'key')` |
| `expression.registry(entry, field)` | Read a field of the registry entry in fixed account `entry` |
| `expression.returnData(type, offset?)` | Read the return data (bytes a called program hands back) of the invoke just before; `offset` defaults to 0 |
| `expression.rowInput(name)` | Read a row input of the current batch row, inside `forEach` only |
| `step.require(condition, label?)` | Fail the run unless the condition is true |
| `step.let(name, value, label?)` | Evaluate a value once and bind it to a name |
| `step.snapshot(name, value, label?)` | Same as `let`; the name suits before-and-after checks |
| `step.assign(name, value, label?)` | Reassign a carried variable inside a loop |
| `step.invoke(descriptor)` | Perform a CPI (a call to another program); `when` makes it conditional, `programAddress` names the intended program, and `accountGroup` forwards an account group |
| `step.forEach(steps, { carry?, label? })` | Run the steps once per batch row; see [Loops](#loops) |
| `step.repeat(count, steps, { max, carry?, label? })` | Run the steps `count` times, at most `max`; see [Loops](#loops) |
| `step.emit(parts, label?)` | Log the parts for indexers; see [Output](#output) |
| `step.setReturnData(parts, label?)` | Set the parts as the run's return data; see [Output](#output) |
| `step.setRegistry(entry, field, value, label?)` | Write a field of the registry entry in fixed account `entry` |
| `data.literal(bytes)`, `data.encode(encoding, value)` | Build a CPI's instruction data, or an output's bytes, from literal bytes and encoded values |

A read takes one of these types: `bool`, `u8`, `u16`, `u32`, `i32`, `u64`, `i64`, `u128`, or
`pubkey`. `u8`, `u16`, and `u32` give a `u64`, and `i32` gives an `i64` with its sign kept.
`readWidth` maps each type to its width in bytes.

A template document has these fields:

| Field | Meaning |
| --- | --- |
| `inputs` | Named inputs and their types; a `bytes` input also takes `maxLength` (1 to 1,024) |
| `accounts` | Named fixed accounts and their constraints |
| `registries` | Optional: up to 8 named registries and their fields; see [Registries](#registries) |
| `batch` | Optional: `maxIterations` (1 to 60), `minIterations` (default 0), `row` (1 to 8 named accounts with constraints), and `rowInputs` (up to 8 named inputs) |
| `accountGroups` | Up to 8 names of caller-sized account lists; see [Account groups](/guide/account-groups) |
| `emitEvent` | Log a run event after each successful run; default `false` |
| `steps` | 1 to 128 steps |
| `version` | Optional; must be `1` |

An account constraint accepts `signer`, `writable`, and `executable` (default `false`); `address`
and `owner` (32-byte `Uint8Array`s that pin the account's address or its owner program);
`minDataLength` (default 0); `unsafeUnpinned` (default `false`), which waives the pinning rules
below; and `registry`, which `account.registry` sets to make the account a registry entry.

### Math

These sit on `expression` next to `add` and `divide`. None of them wraps.

| Expression | Result |
| --- | --- |
| `multiplyDivide(a, b, divisor, rounding?)` | `a × b ÷ divisor` for three `u64`s or three `u128`s. The product is exact, up to 256 bits, so only the result has to fit. `rounding` is `'down'` (the default) or `'up'` |
| `remainder(a, b)` | `a mod b` for two `u64`s, `i64`s, or `u128`s. The result takes the sign of `a` |
| `shiftLeft(value, bits)`, `shiftRight(value, bits)` | A `u64` or `u128` shifted by a `u64` number of bits. `shiftRight` rounds down |
| `bitAnd(a, b)`, `bitOr(a, b)`, `bitXor(a, b)` | Bitwise operations on two `u64`s or two `u128`s. The boolean `and` and `or` are separate |
| `powerOfTen(exponent)` | `10^exponent` as a `u128`, for a `u64` exponent from 0 to 38 |

A zero divisor fails the run with `DivisionByZero`. A result too large for its type, a left shift
that would drop a set bit, an exponent above 38, or the smallest `i64` modulo -1 fails it with
`ArithmeticOverflow`. Operands of the wrong type fail compilation.

### Loops

A template holds up to 8 loops at the top level. They run one after another, and a loop cannot
hold another.

- `step.forEach(steps, { carry?, label? })` runs once per batch row. A template with a `batch`
  needs at least one, and a template without one can have none.
- `step.repeat(count, steps, { max, carry?, label? })` runs `count` times. `count` is a `u64`
  expression, evaluated once before the first pass. `max`, from 1 to 255, is the most passes
  allowed: a run whose count is higher fails with `LoopCountExceeded`. A `repeat` body has no
  rows, so it cannot use `account.iteration` or `expression.rowInput`.

Each body holds 1 to 64 steps, and `expression.loopIndex()` is the current pass, counting from 0.
`carry` lists variables defined before the loop that keep their value from one pass to the next and
after the loop; `step.assign` updates them. The limit of 64 CPIs per run counts every loop at its
maximum: `maxIterations` passes for `forEach` and `max` for `repeat`.

### Output

`step.emit` and `step.setReturnData` build bytes the way `step.invoke` builds instruction data:
from 1 to 64 `data.literal` and `data.encode` parts, at most 1,024 bytes in all, counting a `bytes`
value at its maximum length.

- `step.emit(parts, label?)` logs the bytes as one base64 `Program data:` line in the
  transaction's logs, where indexers can read them. The first part must be a `data.literal` tag of
  at least 4 bytes (`MIN_EMIT_TAG_LENGTH`) that does not start with `BEV`
  (`RUN_EVENT_TAG_FAMILY`), so the log cannot pass for Ballista's
  [run event](/guide/errors-and-events#run-events). It can appear anywhere, loops included.
- `step.setReturnData(parts, label?)` sets the bytes as the run's return data. The run's caller
  reads them: a template that invoked the run, with `expression.returnData`, or a client simulating
  a transaction that ends with the run. It can appear once, outside every loop, with no invoke
  after it, because invoking a program clears return data.

Compilation fails when a step breaks these rules.

```ts
// systemProgram, payer and recipient are account.fixed(...) references.
const steps = [
  step.let('paid', expression.u64(0)),
  // Pay `amount` once per round, for at most 4 rounds.
  step.repeat(
    expression.input('rounds'),
    [
      systemTransfer({ systemProgram, from: payer, to: recipient, lamports: expression.input('amount') }),
      step.assign('paid', expression.add(expression.variable('paid'), expression.input('amount'))),
    ],
    { max: 4, carry: ['paid'] },
  ),
  // Log the tag "PAID" and the total, then return the total to the caller.
  step.emit([
    data.literal(new TextEncoder().encode('PAID')),
    data.encode('u64', expression.variable('paid')),
  ]),
  step.setReturnData([data.encode('u64', expression.variable('paid'))]),
];
```

### Introspection and byte reads

Introspection means reading the other instructions in the same transaction. These expressions do
it through the Instructions sysvar. A sysvar is an account whose data the Solana runtime maintains,
and this one holds every instruction in the current transaction. Declare it as a fixed account
pinned to its address, `{ address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES }`, and pass that account as
`sysvar`; compilation fails for any other account. At run time, bind it to
`Sysvar1nstructions1111111111111111111111111`.

| Expression | Result |
| --- | --- |
| `instructionCount(sysvar)` | The number of instructions in the transaction, as a `u64` |
| `currentInstructionIndex(sysvar)` | The index of the instruction running the template |
| `instructionProgram(sysvar, index)` | The program that instruction `index` calls, as a `pubkey` |
| `instructionAccountCount(sysvar, index)` | How many accounts instruction `index` lists |
| `instructionAccount(sysvar, index, position)` | The address of the account at `position` in instruction `index` |
| `instructionAccountFlags(sysvar, index, position)` | The flags of that account, as a `u64`: bit 0 is signer, bit 1 is writable |
| `instructionAccountIsSigner(sysvar, index, position)`, `instructionAccountIsWritable(...)` | One of those flags, as a `bool` |
| `instructionDataLength(sysvar, index)` | The length of instruction `index`'s data |
| `instructionData(sysvar, index, offset, type)` | A value of one of the read types, from instruction `index`'s data at `offset` |
| `instructionDataBytes(sysvar, index, offset, length)` | Exactly `length` bytes, 1 to 1,024, of instruction `index`'s data from `offset` |
| `accountDataBytes(account, offset, length)` | Exactly `length` bytes, 1 to 1,024, of an account's data from `offset`, read in place without a copy |
| `bytesLength(value)` | The length of a `bytes` value, as a `u64` |

`index`, `position`, and `offset` take a number or a `u64` expression. An index, position, or byte
range beyond what exists fails the run with `InstructionOutOfRange`.

`accountDataBytes` needs an account that pins `owner` or `address` (or is `unsafeUnpinned`) and is
not declared `writable`. If the account is writable in the run instruction anyway, the run fails
with `WritableAccountBytesRead`, because a CPI could change the bytes while the run holds them.

Compare `bytes` values with `expression.equal`, or pass one on with `data.encode('bytes', value)`,
which adds no length prefix. When the receiving program expects one, encode `bytesLength(value)`
before it, at the width that program reads.

### Registries

[Remember state between runs](/guide/registries) walks through examples, and the
[language reference](/reference/language#registries) has the full rules.

A registry gives a template state that outlives a run, such as a running total or a spending
limit. It declares named fields, and each entry holds one copy of them in its own account, picked
by a 32-byte key the template computes, such as the caller's address. Ballista owns every entry.
Only the template's own runs can write it, and anyone can read it. Entries belong to the
template's address, so a new version of a template starts with new entries.

```ts
// Each caller can send at most 1 SOL in all, over every run.
// systemProgram, caller and recipient are account.fixed(...) references.
const template = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  registries: { totals: { sent: 'u64' } },
  accounts: {
    caller: { signer: true, writable: true },
    recipient: { writable: true },
    // One entry per caller, keyed by the caller's address. The caller pays for it once.
    callerTotal: account.registry('totals', {
      key: expression.accountKey('caller'),
      payer: 'caller',
    }),
    systemProgram: account.systemProgram(),
  },
  steps: [
    step.let(
      'sent',
      expression.add(expression.registry('callerTotal', 'sent'), expression.input('amount')),
    ),
    step.require(
      expression.lessThanOrEqual(expression.variable('sent'), expression.u64(1_000_000_000)),
      'withinTotal',
    ),
    step.setRegistry('callerTotal', 'sent', expression.variable('sent')),
    systemTransfer({ systemProgram, from: caller, to: recipient, lamports: expression.input('amount') }),
  ],
});
```

- `registries` declares up to 8 registries (`MAX_REGISTRIES`). A field is a `bool`, `u64`, `i64`,
  `u128`, or `pubkey`, and a registry holds 1 to 512 bytes of fields (`MAX_REGISTRY_SIZE`), packed
  in declaration order; `registrySize(fields)` adds them up. A registry's index is its position in
  `registries`.
- Each `account.registry` account is one entry, and a template declares at most 8
  (`MAX_REGISTRY_OPENS`). `key` is a `pubkey` expression. Leave it out for one entry that every run
  shares, keyed by 32 zero bytes. `payer` names a fixed account declared signer and writable.
- `expression.registry` and `step.setRegistry` name the entry's account, not the registry, so two
  entries of one registry stay apart. A written value must have the field's type. The write lands
  at once, and a run that fails later undoes it with the rest of the transaction.
- Read an entry's data only through its fields: `accountData` and `accountDataBytes` do not compile
  on it. Its address, owner, lamports, and data length stay readable with `expression.accountField`.
  No invoke may pass an entry writable.

The compiler opens every entry before the first step, in the order the accounts are declared, so a
key can read the fields of entries declared before its own. Opening checks an existing entry, or
creates a missing one:

- An entry lives at a program-derived address (PDA): an address computed from a program's ID and
  a list of byte strings called seeds. Its seeds are `"registry"`, the template's address, the
  registry's index, and the key.
- Creating it calls the System Program, so a template with registry accounts must declare a fixed
  account pinned to it, such as `account.systemProgram()`. The payer pays the rent: the lamports
  (the smallest unit of SOL) that Solana requires an account to hold for its size, here a 72-byte
  header (`REGISTRY_ENTRY_HEADER_LENGTH`) plus the fields. Entries are never closed, so the rent
  stays in them.
- Each open counts as 3 of the 64 CPIs a run allows (`REGISTRY_OPEN_CPIS`), because an address
  that already holds lamports takes three calls to create.

At run time, pass each entry's address like any fixed account's.
[`findRegistryEntryAddress`](#solana-kit-adapter) derives it. A run fails with:

- `InvalidRegistryEntry` (6025) if an entry account is not this template's entry for its registry
  and key, or has another size. `explainRunError` names the account.
- `RegistryReentry` (6026) if a CPI passes an open entry writable, as a row account or an account
  group member, where the compiler cannot see it.
- `AccountConstraintFailed` (6020) if an entry is passed read-only.

The program's verifier checks the same rules when a template is created or finalized. It rejects a
template that breaks a registry rule with `InvalidRegistry` (6132), and one with more than 64 CPIs,
opens included, with `ExcessiveCpiExpansion` (6121). [Rate limits](#rate-limits) builds a limit
that refills over time on a registry.

## Compile-time checks

`compileTemplate` fails with an error that describes the problem when it finds:

- an invoked program, or a program used to derive a PDA, that is not marked `executable`, or that
  does not pin its `address` and is not `unsafeUnpinned`;
- a data read from an account that pins neither `owner` nor `address` and is not
  `unsafeUnpinned`, or an `accountDataBytes` read from an account declared `writable`;
- an introspection expression whose `sysvar` is not a fixed account pinned to
  `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES`;
- an invoke whose `programAddress` differs from the address its program account pins (the CPI
  helpers below set `programAddress`);
- an invoke that asks for signer or writable on an account whose constraint does not allow it;
- `assign` outside a loop, to a variable not listed in `carry`, or with a different type or size;
- a row account or row input outside `forEach`, including in a `repeat` body;
- `returnData` anywhere except as the value of a `let` directly after an invoke with no guard;
- an `emit` without a valid tag, or a `setReturnData` inside a loop, a second time, or before an
  invoke;
- a registry account in a batch row or declared with anything besides `writable`, more than 8 of
  them, a payer not declared signer and writable, or no fixed account pinned to the System
  Program;
- a registry key that is not a `pubkey` or reads its own entry or a later one, an unknown
  registry, registry account, or field, a write of another type, a data read of an entry, or an
  invoke that passes an entry writable;
- more than 64 registers (the numbered slots that hold values during a run), 128 bytecode
  instructions, 64 CPIs per run (counting every loop at its maximum and each registry open as 3),
  or 10,240 bytes of bytecode;
- CPI instruction data that could exceed 4,096 bytes, an output that could exceed 1,024 bytes, or
  a PDA seed that could exceed 32 bytes.

`defineTemplate` and `compileTemplate` also reject anything the document schema forbids, such as
more than 120 runtime accounts, more than 8 loops, a loop inside another, or a registry outside 1
to 512 bytes. Reads at a fixed offset raise the account's `minDataLength` to cover the read.

## Compilation and inspection

```ts
import { compileTemplate, decodeTemplateAccount, inspectTemplate } from '@jac0xb/ballista';

const compiled = compileTemplate(template);

compiled.bytes;             // bytecode to store on chain
compiled.hash;              // SHA-256
compiled.inputOrder;
compiled.fixedAccountOrder;
compiled.batchAccountOrder;
compiled.registryOrder;     // registry names, by index
compiled.stats;             // counts and sizes
compiled.sourceMap;         // { pc, path, label? } per instruction

inspectTemplate(compiled.bytes);
decodeTemplateAccount(accountBytes);
```

`inputOrder`, `fixedAccountOrder`, and `batchAccountOrder` give the order in which callers pass
inputs and accounts. `rowInputOrder` and `accountGroupOrder` do the same for row inputs and
account groups. `registryOrder` lists the registries in declaration order, and
`registryIndex(compiled, name)` gives one's index for `findRegistryEntryAddress`, throwing a
`TypeError` for an unknown name. `sourceMap` links each bytecode instruction's index (`pc`, the
program counter reported in run errors) to the step that produced it, as a path such as
`steps[1].steps[0]`, with the step's label if it has one. A registry open's path is the entry's
account, such as `accounts.callerTotal`.

`inspectTemplate(bytes)` reads any compiled payload and returns the same stats.
`decodeTemplateAccount(accountBytes)` decodes a template account: its state (0 uploading,
1 finalized), creator, template ID, lengths, hash, and payload.

## Errors

```ts
import { decodeBallistaError, explainRunError } from '@jac0xb/ballista';

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
import {
  encodeBeginTemplate,
  encodeCancelTemplate,
  encodeCreateTemplate,
  encodeFinalizeTemplate,
  encodeWriteTemplateChunk,
  planTemplateUpload,
  resumeTemplateUpload,
} from '@jac0xb/ballista';

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
import { buildRunInstruction } from '@jac0xb/ballista';

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

To encode only the instruction data, without accounts:

- `encodeRunInputs(compiled, values, { rows?, groupLengths? })` returns the run's input bytes:
  each account group's length, the inputs in declaration order, then each row's inputs. It
  rejects missing or unknown inputs and more than 1,024 bytes.
- `encodeRun(compiled, values, options?)` returns the same bytes behind the run instruction's
  tag, `INSTRUCTION_RUN`.

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
`ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES` are exported for pinning those programs, and
`INSTRUCTIONS_SYSVAR_ADDRESS_BYTES` and `ED25519_PROGRAM_ADDRESS_BYTES` for the Instructions
sysvar and the Ed25519 program.

The core package also exports:

- `BALLISTA_PROGRAM_ADDRESS`, the program's address as a base58 string.
- The limits as `MAX_*` constants, such as `MAX_RUNTIME_ACCOUNTS` and `MAX_EXPANDED_CPIS`; see
  [Limits](/reference/limits).
- `opcode`, the bytecode opcode numbers, and the `INSTRUCTION_*` instruction tags.
- `RUNTIME_ERROR_NAMES` and `VERIFIER_ERROR_NAMES`, the error names in code order from
  `RUNTIME_ERROR_BASE` (6000) and `VERIFIER_ERROR_BASE` (6100).
- The Zod schemas behind `defineTemplate`, such as `TemplateSchema` and `StepSchema`.

### Signed messages

`ed25519Signature` ties a signature that the transaction's Ed25519 instruction verified to the
template. The Ed25519 program is a precompile: a program built into Solana that checks signatures
as part of the transaction, so a transaction with an invalid signature fails. The precompile does
not say whose key it checked or over which bytes; the steps this helper returns check both.

```ts
const instructions = account.fixed('instructions');
const quote = ed25519Signature({
  sysvar: instructions,
  // The Ed25519 instruction directly before the run.
  index: expression.subtract(expression.currentInstructionIndex(instructions), expression.u64(1)),
  // maker is declared { signer: true }, so this key must also sign the transaction.
  signer: expression.accountField(account.fixed('maker'), 'key'),
  messageLength: 128,
  name: 'quote',
});

const steps = [
  ...quote.steps,
  // Bytes 16 to 23 of the signed message hold the most the maker sells.
  step.require(
    expression.lessThanOrEqual(expression.input('amount'), quote.field(16, 'u64')),
    'withinTheQuotedSize',
  ),
];
```

- `steps` require that instruction `index` is the Ed25519 program and holds exactly one signature,
  by `signer`, over exactly `messageLength` bytes (1 to 65,535), with the key, the signature, and
  the message all in its own data. Put them before any step that uses `field`: a template that
  reads `field` without them does not compile.
- `field(offset, type)` reads a value from the signed message. It throws a `RangeError` unless the
  read lies inside the message.
- `signer` must be a key the transaction's builder cannot choose, such as a pinned address or the
  key of an account that must sign. Otherwise the builder can sign with a key of their own. The
  helper throws a `TypeError` for an input or a row input, but it cannot see account constraints.
- `name`, `'signature'` by default, prefixes the step labels, such as `quoteIsBySigner`, so
  `explainRunError` names the check that failed. Different names let one template check several
  signatures.

The SDK does not build the Ed25519 instruction itself. The example
[Settle at a signed quote](/examples/protocols/signed-quote) shows a full template and the
transaction that runs it.

### Rate limits

`rateLimit` keeps a spending limit that refills over time in a [registry](#registries) entry. The
steps it returns read the entry's `spent` (`u64`) and `lastSpend` (`i64`, a Unix time) fields,
refill `spent` by the seconds since `lastSpend` times `refillPerSecond`, add `amount`, require the
total to be at most `cap`, and write the total and the time back.

```ts
const template = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    caller: { signer: true, writable: true },
    limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
    systemProgram: account.systemProgram(),
  },
  steps: [
    // 1 SOL a caller, refilling over a day: 1,000,000,000 lamports over 86,400 seconds.
    ...rateLimit({
      registry: 'limits',
      cap: expression.u64(1_000_000_000),
      refillPerSecond: expression.u64(11_574),
      amount: expression.input('amount'),
    }),
    // Then the steps that spend `amount`.
  ],
});
```

- `registry` names the registry account, declared with `account.registry`. `cap`,
  `refillPerSecond`, and `amount` are `u64` expressions.
- `cap` and `refillPerSecond` must be template constants: literals, or arithmetic and logic over
  them. The helper throws a `TypeError` for an input, a row input, a variable (whose origin it
  cannot trace), or a read of the transaction or of an account's data or fields, even inside
  arithmetic. A registry field is accepted, so an author can change a limit later. The helper
  cannot tell who wrote the field, so the template must write it only in an author-only branch,
  one that requires a pinned author to sign. A field any caller's run can write gives the caller
  the cap.
- Key the entry by a signer's address, such as `expression.accountKey('caller')`, for a limit per
  signer, or leave `key` out for one limit that every caller shares. A key taken from an input
  lets a caller open a fresh entry on every run and spend past the cap. The helper sees only the
  account's name, so it cannot check the key.
- A run over the cap fails with `RequirementFailed` at the step labeled `withinRateLimit`.
- A new entry starts with nothing spent. The refill is computed in `u128`, so no gap between runs
  can overflow it. If the clock steps back, a run refills nothing, rather than failing, and
  `lastSpend` stays where it was, so no later run refills the same seconds twice.
- `spent` and `lastSpend` rename the two fields. `name`, `'rateLimit'` by default, prefixes the
  variables the steps bind and names the requirement `within<Name>`, so one template can keep
  several limits.

## Solana Kit adapter

Import from `@jac0xb/ballista/kit`:

```ts
import {
  buildKitResumeTemplateUploadPlan,
  buildKitRunInstruction,
  buildKitTemplateUploadPlan,
  createComputeUnitProvider,
  findFreeTemplateId,
  findRegistryEntryAddress,
  getComputeUnitsConsumed,
  getTemplateAddress,
  measureTransactionMessage,
} from '@jac0xb/ballista/kit';

getTemplateAddress(creator, templateId, programAddress?);
findFreeTemplateId({ rpc, creator, programAddress?, start?, batchSize? });
findRegistryEntryAddress(template, registryIndex, key, programAddress?);
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
| `findFreeTemplateId` | The lowest template ID at or above `start` whose address holds no account, probing `batchSize` addresses per RPC call (default 16, at most 100). An address that already holds a template cannot be reused |
| `findRegistryEntryAddress` | A [registry](#registries) entry's address and bump. `key` is an address or 32 bytes, all zeros for an entry without a key. Throws a `RangeError` for an index outside 0 to 7 or a key that is not 32 bytes |
| `buildKitRunInstruction` | `buildRunInstruction` with Kit addresses, returning a Kit `Instruction` |
| `buildKitTemplateUploadPlan` | An upload plan made of Kit instructions; given a `transactionMessage`, it sizes the chunks to fit that message |
| `buildKitResumeTemplateUploadPlan` | The same, for resuming an upload |
| `measureTransactionMessage` | A message's size, the size limit for its version, and whether it fits |
| `createComputeUnitProvider` | A provider whose `estimateAndSet(message)` simulates the message and sets its compute-unit limit (the execution budget the transaction requests) to the measured amount plus a margin in basis points (1,000 is 10%, the default). For version 1 messages it also sets the loaded-account-data limit |
| `getComputeUnitsConsumed` | The compute units a confirmed transaction used, read from its metadata |

`buildKitCancelTemplateInstruction({ creator, templateId, programAddress? })` is async and resolves
to the instruction that cancels an unfinished upload.

The adapter also exports:

- `BALLISTA_ADDRESS` and `SYSTEM_PROGRAM_ADDRESS`, as Kit addresses.
- `toKitInstruction(descriptor)`, which turns any core instruction descriptor into a Kit
  `Instruction`.
- `measureInstructionInTransaction(message, instruction)`, which measures the message with the
  instruction appended.
- `getComputeUnitLimitWithMargin(simulatedUnits, { marginBps?, maxComputeUnitLimit? })`, the
  margin math `createComputeUnitProvider` uses, capped at `MAX_TRANSACTION_COMPUTE_UNITS`
  (1,400,000).
- `getLoadedAccountsDataSizeLimitWithHeadroom(bytes)`, which rounds a loaded-account-data size up
  to the next 32 KiB page.

To run the [Registries](#registries) example, derive the caller's entry and pass it as
`callerTotal`'s address:

```ts
import { registryIndex } from '@jac0xb/ballista';
import { findRegistryEntryAddress } from '@jac0xb/ballista/kit';

// templateAddress and caller are Kit addresses; compiled is the compiled template.
const [callerTotal] = await findRegistryEntryAddress(
  templateAddress,
  registryIndex(compiled, 'totals'),
  caller,
);
```
