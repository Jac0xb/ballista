# Template language

Everything a template can contain. A template declares typed inputs, the accounts it expects, an
optional batch of repeated rows, optional registries that keep state between runs, and an ordered
list of steps. Function names are from the TypeScript SDK. The Rust `ProgramBuilder` produces the
same bytecode at a lower level; see [Rust SDK](/reference/rust). Every maximum is on
[Limits](/reference/limits).

The compiler turns a template into bytecode: a list of fixed-size instructions, each identified by
a number called its opcode. Instructions keep intermediate values in
[registers](/reference/glossary#register). The names you give inputs, accounts, bindings,
registries, and fields are replaced by numbers and are not stored on chain.

When a template is [finalized](/reference/glossary#finalize), the Ballista program's
[verifier](/reference/glossary#verifier) checks its bytecode once. It confirms that the template
always finishes, never reads a register before writing it, only refers to accounts it declared, and
stays within the limits on [CPIs](/reference/glossary#cpi), on CPI instruction data, and on the
bytes it logs or returns, even in the worst case. The language leaves out anything that would stop
those checks from working.

## Values

A register holds one of six types. Five have a fixed width. A `bytes` value has a maximum length
that is declared when the template is written.

| Type | Width | Typical use |
| --- | ---: | --- |
| `bool` | 1 byte | Conditions and flags |
| `u64` | 8 bytes | [Lamports](/reference/glossary#lamports), token amounts, slot numbers, byte offsets |
| `i64` | 8 bytes | Signed values, such as the clock's Unix timestamp |
| `u128` | 16 bytes | Intermediate products that would overflow 64 bits |
| `pubkey` | 32 bytes | Account addresses, compared for equality or used as [PDA](/reference/glossary#pda) seeds |
| `bytes` | declared maximum, up to 1,024 | Raw data the template passes along without interpreting it |

Numeric types are never converted automatically. The compiler rejects an addition, a comparison,
or a `select` whose operands have different types, so a type mismatch is a build error rather than
a failed transaction. To convert, use `expression.cast`, which accepts any numeric value and
produces a `u64`, `i64`, or `u128`. A cast fails the run if the value does not fit the target type.

There is no string type, no floating point, no map or struct, and no way to decode another
program's account layout. Structured data is read as fixed-width fields at known byte offsets.
Anything else stays as `bytes`.

## Inputs

A template declares up to 32 named inputs, each with one of the six types. A `bytes` input also
declares a maximum length from 1 to 1,024. That maximum counts toward the worst-case CPI data size
the verifier checks.

Callers send the input values as one byte string, in declaration order; the encoding is on
[Wire format](/reference/wire-format#run-data). Declaration order is therefore part of a template's
interface. Renaming an input leaves the compiled template unchanged, but reordering inputs changes
the encoding every caller must produce.

A batch can also declare up to eight row inputs: values the caller supplies once per row, read
inside a `forEach` loop with `expression.rowInput(name)`. Fixed and row inputs share the limit of
32. The number of fixed inputs, plus the number of row inputs times the maximum row count, may not
exceed 256.

## Accounts

The account schema lists, by name, the accounts a template uses. Each declaration states the most a
run may do with that account, and the program rejects a run whose account does not satisfy it. A
declaration can require the account to be a signer (the transaction carries its signature), writable
(the transaction allows programs to modify it), or executable (a program). It can also pin the
account's address or its owner program, and require a minimum data length. To pin a value is to
fix it in the template, so that a run fails if the caller passes anything else.

A CPI in the template may ask for the same privileges as the account's declaration or fewer, never
more. Reading a finalized template's schema therefore tells you the most any run of it can do
through each slot it declares. The caller fills the slots, so one address can still reach a call
writable through another slot or an account group; see [Privileges](/guide/trust-model#privileges).
Ballista only passes on signatures the transaction already carries, so a template cannot gain
authority the transaction did not already have; see
[When Ballista signs](/guide/trust-model#signing).

A run checks each account against its own declaration, so the caller can pass one account in two
slots; where two slots must hold different accounts, require their keys to differ, as the
[trust model](/guide/trust-model#aliased-accounts) shows.

The compiler enforces two pinning rules:

- A program that the template invokes, or derives a PDA from, must be marked `executable` and must
  pin its `address`. Otherwise the caller could substitute any program.
- An account whose data the template reads must pin its `owner` or its `address`. A byte offset
  only has a meaning when you know which program wrote the data.

Setting `unsafeUnpinned: true` on an account waives both rules for that account. Use it when a
template deliberately accepts a program or data that the caller chooses. Reading the transaction's
other instructions has a stricter rule that nothing waives; see [Introspection](#introspection).

Accounts are named in steps with `account.fixed(name)` for an account in the schema and
`account.iteration(name)` for an account in the current batch row, inside a `forEach` loop. Two
more `account` functions build declarations for the schema rather than names:
`account.registry(registry, { key, payer })` declares a registry entry, and
`account.systemProgram()` declares the System program, pinned by address. See
[Registries](#registries).

## Reading state

Every source of a value, and where it can be used:

| Constructor | Result | Where it can be used |
| --- | --- | --- |
| `expression.input(name)` | The input's declared type | Anywhere |
| `expression.rowInput(name)` | The row input's declared type | Inside `forEach` only |
| `expression.variable(name)` | The binding's type | After the binding, while it is in scope |
| `expression.snapshot(name)` | The binding's type | Same as `variable` |
| `expression.bool(v)` | `bool` | Literal |
| `expression.u64(v)` | `u64` | Literal |
| `expression.i64(v)` | `i64` | Literal |
| `expression.u128(v)` | `u128` | Literal |
| `expression.pubkey(v)` | `pubkey` | Literal, 32 bytes |
| `expression.bytes(v)` | `bytes` | Literal, up to 1,024 bytes |
| `expression.accountField(account, field)` | See [Account data](#account-data) | Any schema account, including a row account inside a `forEach` loop |
| `expression.accountKey(name)` | `pubkey` | A fixed account; shorthand for `accountField(account.fixed(name), 'key')` |
| `expression.registry(entry, field)` | The field's declared type | `entry` names a fixed account declared with `account.registry`; see [Registries](#registries) |
| `expression.accountData(account, offset, type)` | See [Account data](#account-data) | An account that pins its owner or address |
| `expression.accountDataBytes(account, offset, length)` | `bytes`, exactly `length` long | A read-only account that pins its owner or address |
| `expression.returnData(type, offset?)` | See [Return data](#return-data) | Only as the value of a `let` directly after an unguarded invoke |
| `expression.clockSlot()` | `u64`, the current slot (Solana's block-by-block time counter) | Anywhere |
| `expression.clockUnixTimestamp()` | `i64`, the Unix timestamp | Anywhere |
| `expression.loopIndex()` | `u64`, the zero-based index of the current row or pass | Inside a loop only |
| `expression.pda(program, seeds, bump?)` | `pubkey` | A program that pins its address; see [PDAs](#pdas) |

The sources that read the transaction's other instructions are under
[Introspection](#introspection).

### Account data

The `field` argument of `accountField` selects one of five properties that do not depend on the
account's layout:

| Field | Result |
| --- | --- |
| `key` | `pubkey` |
| `owner` | `pubkey` |
| `lamports` | `u64` |
| `dataLength` | `u64` |
| `isEmpty` | `bool`, true when the account holds no data |

`accountData` reads the account's data at a byte offset. Its `type` argument selects one of nine
types, which return-data and instruction-data reads share:

| Read type | Result |
| --- | --- |
| `bool` | `bool`; the run fails unless the byte is 0 or 1 |
| `u8`, `u16`, `u32`, `u64` | `u64`, so a one-byte flag and an eight-byte amount combine without a cast |
| `i32`, `i64` | `i64`; an `i32` is sign-extended, so a negative value stays negative |
| `u128` | `u128` |
| `pubkey` | `pubkey` |

- **A constant offset** raises the account's minimum data length. If a template reads a `u64` at
  offset 64, the compiler records that the account must hold at least 72 bytes, and every run
  checks it.
- **An offset computed during the run**, a `u64` expression, lets the position depend on an input
  or an earlier read. Such a read does not raise the minimum data length, and it fails the run if
  it extends past the end of the data.
- **`accountDataBytes`** reads exactly `length` bytes (1 to 1,024) as a `bytes` value, from a `u64`
  offset. The bytes are used in place rather than copied, which is safe only because the account
  cannot change during the run. So the compiler rejects an account declared `writable`, and a run
  fails with `WritableAccountBytesRead` if the account is passed as writable. This read never
  raises the minimum data length. A range past the end of the data fails the run with
  `InstructionOutOfRange`.

### Return data

A CPI's return data is bytes the invoked program hands back. `expression.returnData(type, offset?)`
reads it at a byte offset, `0` by default, as one of the read types. The read must be the value of a
`let` step placed directly after an invoke that has no guard: a guarded invoke might be skipped,
which would leave no return data and a register with no defined value. The run also fails if the
return data was not set by the program just invoked, or is too short for the read.

### PDAs

A template derives a [PDA](/reference/glossary#pda) from a pinned program and 1 to 15 seeds. Each
seed can be a value of any type and contributes that type's encoding: a `u64` adds 8 little-endian
bytes, a `pubkey` its 32 bytes. No seed may exceed 32 bytes. The result is a `pubkey`, usually
compared with an account the caller supplied. This is how a template checks that it was given the
right vault or [ATA](/reference/glossary#ata) instead of trusting the caller.

Deriving a PDA needs a bump: one extra seed byte, tried from 255 downward until the result is a
valid program address. The first value that works is the canonical bump. By default the template
searches for it, and each attempt costs 1,500 [compute units](/reference/glossary#compute-units).
Passing a `u64` bump as the third argument derives the address once instead. A bump above 255, or
one that does not produce a valid program address, fails the run.

The two forms prove slightly different things. Without a bump, a match proves the account is the
canonical PDA for those seeds. With a bump, a match proves the account is the PDA for those seeds
and that bump. Several bumps can produce valid addresses, so when the caller chooses the bump, a
match does not prove the address is the canonical one. If the account must be the canonical PDA,
do not take the bump from the caller: leave it out, or, when every seed is fixed in the template,
write the canonical bump in as a constant.

### Introspection {#introspection}

A template can read every instruction in its transaction, before and after its own: the program
each one calls, the accounts it passes with their signer and writable flags, and its data. It reads
them through the [Instructions sysvar](/reference/glossary#instructions-sysvar), a read-only account
that Solana fills with the transaction's instructions.

- **Declare the sysvar** as a fixed account, not a row account, that pins its `address` to
  `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES`. The verifier checks this too, and `unsafeUnpinned` does not
  waive it. Each source below takes that account as its first argument, `sysvar`.
- **`index`, `position`, and `offset`** are `u64` values, and a number becomes a constant. An index,
  position, or byte range that the transaction does not hold fails the run with
  `InstructionOutOfRange`.
- **`currentInstructionIndex` is a top-level index.** When the template runs inside a CPI, it is
  the index of the transaction's instruction that made the outer call, so `instructionProgram` at
  that index is the outer program, not Ballista. "The instruction before this one" counts from
  there.
- **An account read this way is an address to compare**, not access to the account.

| Constructor | Result |
| --- | --- |
| `expression.instructionCount(sysvar)` | `u64`, how many instructions the transaction holds |
| `expression.currentInstructionIndex(sysvar)` | `u64`, the index of the top-level instruction this run is part of |
| `expression.instructionProgram(sysvar, index)` | `pubkey`, the program that instruction `index` calls |
| `expression.instructionAccountCount(sysvar, index)` | `u64`, how many accounts it passes |
| `expression.instructionAccount(sysvar, index, position)` | `pubkey`, the account at `position` |
| `expression.instructionAccountFlags(sysvar, index, position)` | `u64`: bit 0 signer, bit 1 writable |
| `expression.instructionAccountIsSigner(...)`, `expression.instructionAccountIsWritable(...)` | `bool`, one of those flags; same arguments |
| `expression.instructionDataLength(sysvar, index)` | `u64`, the length of its data |
| `expression.instructionData(sysvar, index, offset, type)` | One of the [read types](#account-data) |
| `expression.instructionDataBytes(sysvar, index, offset, length)` | `bytes`, exactly `length` long (1 to 1,024), used in place rather than copied |

The [`ed25519Signature`](#assertions) helper uses these reads to check a signed message.

## Computation

Arithmetic is checked. Nothing wraps around, saturates, or is silently truncated: each failure in
the tables below fails the transaction. Division discards the remainder, and `remainder` returns
it. `multiplyDivide` suits an amount times a price, divided by the price's scale: the product is
held exactly (in 256 bits for `u128`), so the run fails only when the final result does not fit,
not when `a × b` alone would. `powerOfTen(n)`, 10 to the power `n`, scales between token decimals.

Arithmetic returns the type of its operands unless the table says otherwise.

| Constructor | Operands | Result | Fails when |
| --- | --- | --- | --- |
| `expression.add(a, b)` | matching numeric | same | The result does not fit the type |
| `expression.subtract(a, b)` | matching numeric | same | The result does not fit the type (for `u64` and `u128`, it would be below zero) |
| `expression.multiply(a, b)` | matching numeric | same | The result does not fit the type |
| `expression.divide(a, b)` | matching numeric | same | The divisor is zero, or the result does not fit (the `i64` minimum divided by −1) |
| `expression.remainder(a, b)` | matching numeric | same, with the sign of `a` | The divisor is zero, or `a` is the `i64` minimum and `b` is −1 |
| `expression.multiplyDivide(a, b, c, rounding?)` | three matching `u64` or `u128` | same | `c` is zero, or the result does not fit; `rounding` is `'down'` (default) or `'up'` |
| `expression.powerOfTen(n)` | `u64` | `u128` | `n` is above 38 |
| `expression.min(a, b)` | matching numeric | same | Never |
| `expression.max(a, b)` | matching numeric | same | Never |
| `expression.cast(to, value)` | any numeric | `u64`, `i64`, or `u128` | The value does not fit the target |

Bit operations work on unsigned integers. A shift amount is always a `u64`.

| Constructor | Operands | Result | Fails when |
| --- | --- | --- | --- |
| `expression.shiftLeft(a, n)` | `a`: `u64` or `u128`; `n`: `u64` | the type of `a` | A set bit would be shifted out |
| `expression.shiftRight(a, n)` | as `shiftLeft` | the type of `a`, rounded down | Never; a shift of the full width or more gives 0 |
| `expression.bitAnd(a, b)`, `expression.bitOr(a, b)`, `expression.bitXor(a, b)` | matching `u64` or `u128` | same | Never |

`expression.bytesLength(value)` takes a `bytes` value and returns its length as a `u64`.

Comparisons return a `bool`. Equality accepts any two values of the same type, including addresses
and bytes. The four ordered comparisons accept numeric types only.

| Constructor | Operands | Result |
| --- | --- | --- |
| `expression.equal(a, b)` | any matching type | `bool` |
| `expression.notEqual(a, b)` | any matching type | `bool` |
| `expression.lessThan(a, b)` | matching numeric | `bool` |
| `expression.lessThanOrEqual(a, b)` | matching numeric | `bool` |
| `expression.greaterThan(a, b)` | matching numeric | `bool` |
| `expression.greaterThanOrEqual(a, b)` | matching numeric | `bool` |

There is no boolean exclusive-or (`notEqual` on two booleans gives the same result), and `and` and
`or` take exactly two operands; nest them to combine more.

| Constructor | Operands | Result |
| --- | --- | --- |
| `expression.and(a, b)` | `bool`, `bool` | `bool` |
| `expression.or(a, b)` | `bool`, `bool` | `bool` |
| `expression.not(value)` | `bool` | `bool` |
| `expression.select(condition, ifTrue, ifFalse)` | `bool` plus two of the same type | The type of the two values |

The bytecode has no jump instruction, so nothing inside an expression is skipped. `select`
evaluates both branches before it chooses, and `and` and `or` evaluate both operands. If any of
those fails (an overflow, a division by zero, a bad read), the run fails, even when the other side
alone would have decided the result. For example, a `select` that divides by a value only when the
value is non-zero still fails when it is zero, because the division runs either way.

## Bindings

A binding evaluates an expression once, at its place in the step list, and keeps the result in a
[register](/reference/limits#registers) for every later read. Write one with
`step.let(name, value)` or `step.snapshot(name, value)`. The two compile identically; `snapshot`
reads better in before-and-after checks. Read the value back with `expression.variable(name)` or
`expression.snapshot(name)`.

Bindings are what make before-and-after comparisons possible. An expression written inline is
evaluated where it appears, so a balance read before a CPI and the same read after it are two
separate reads. Without a binding, the earlier value is lost.

A binding cannot be changed, and its name cannot be reused while the binding is in scope. A binding
is visible to the steps after it; one made inside a loop body is visible only inside that body.
Bindings create no account and end with the transaction. The one exception to "cannot be changed"
is a carried binding in a loop, described under [Carried values](#carried-values).

## Assertions

`step.require(condition, label?)` is the only assertion. It evaluates a boolean and, if it is
false, fails the whole transaction. Solana then undoes everything the run did, including CPIs that
already succeeded. The label names the check, so a failure can be traced to it; see
[Steps](#steps).

Any expression that produces a boolean can be a condition. Common patterns:

- **Identity:** compare an account's `key` with a literal or a derived address.
- **Owner:** compare an account's `owner` with the program that should own it.
- **Bounds:** compare a balance or token amount with an input.
- **Deadline:** compare the clock with an input.
- **Change across a CPI:** compare a value read after the CPI with a binding captured before it.
  This is the one pattern that needs a binding.
- **Combined policy:** join the checks above with `and` and `or`. Never `or` a check with an
  input: the caller sets every input, so the caller could switch the check off.

Two helpers write the common address checks for you. Each returns an ordinary `require` step.

| Helper | Checks that |
| --- | --- |
| `assertPda({ account, program, seeds, bump?, label? })` | The account's address is the PDA derived from those seeds under that program |
| `assertAta({ associatedTokenAccount, owner, mint, tokenProgram, associatedTokenProgram, bump?, label? })` | The account is the associated token account for that owner, mint, and token program |

`assertAssociatedTokenAccount` is another name for `assertAta`. Checking that the caller passed
the account it claims is the most common check in a template.

`ed25519Signature({ sysvar, index, signer, messageLength, name? })` checks a signed message.
Solana's Ed25519 program verifies the signatures in its instruction as part of the transaction, so
a transaction with a bad signature fails, but that instruction does not say whose signature it was
or over which bytes. The helper returns `steps`, which require instruction `index` to be an Ed25519
instruction with exactly one signature, by `signer`, over `messageLength` bytes of its own data, and
`field(offset, type)`, which reads a value from that message. Place the steps before any step that
uses `field`. `signer` must be a key the transaction's builder cannot choose, such as a pinned key
or an account that must sign.

## Steps {#steps}

A template's steps run in order. Each one is one of these:

| Constructor | Effect |
| --- | --- |
| `step.require(condition, label?)` | Fail the transaction unless the condition is true; see [Assertions](#assertions) |
| `step.let(name, value, label?)` | Bind a value to a name for the rest of the run; see [Bindings](#bindings) |
| `step.snapshot(name, value, label?)` | Same as `let`; the name suits before-and-after checks |
| `step.assign(name, value, label?)` | Reassign a carried binding; loop body only; see [Carried values](#carried-values) |
| `step.invoke({ program, accounts, data, when?, accountGroup?, programAddress?, label? })` | Perform a CPI, optionally guarded by `when`; see [Invocations](#invocations) |
| `step.emit(parts, label?)` | Log the encoded parts as one `Program data:` line; see [Output](#output) |
| `step.setReturnData(parts, label?)` | Set the encoded parts as the run's return data; see [Output](#output) |
| `step.setRegistry(entry, field, value, label?)` | Write a value of the field's type into a field of a registry entry; see [Registries](#registries) |
| `step.forEach(steps, { carry?, label? })` | Run the steps once per batch row; top level only; see [Loops](#loops) |
| `step.repeat(count, steps, { max, carry?, label? })` | Run the steps `count` times; a `count` above `max` (1 to 255) fails the run with `LoopCountExceeded`; top level only; see [Count loops](#count-loops) |

- **Only an invocation can be skipped,** by its guard. Every other step runs each time the run
  reaches it.
- **A label** (up to 64 characters) is kept in the compiled template's source map, which links each
  bytecode instruction to the step that produced it, so a failure can be traced to a specific
  step. Labels are not stored on chain.

### Invocations

An invocation calls a program whose address the template pins.

- **`accounts`** lists up to 64 accounts, each `{ account, signer?, writable? }`, with the flags the
  call needs, no more than each account's declaration allows.
- **`accountGroup`** names one [account group](/guide/account-groups): a list of accounts the
  caller supplies at run time, passed after the listed accounts and never as signers.
- **`data`** builds the instruction data from up to 64 parts, at most 4,096 bytes. The verifier
  checks the worst case when the template is finalized.
- **`programAddress`** states which program the step is written for. Compilation fails if the
  `program` account pins a different address.
- **`when`** is a guard that makes the CPI optional. It is evaluated in place; if it is false, the
  CPI is skipped and the run continues with the next step. Use a guard when the work is sometimes
  unnecessary, such as creating an account that may already exist, and a requirement when a false
  condition means something is wrong. If the template sets `emitEvent`, the
  [run event](/guide/errors-and-events#run-events) records which invocations ran.

Instruction data, logs, and return data are built from two part constructors.

| Constructor | Produces |
| --- | --- |
| `data.literal(bytes)` | Fixed bytes, typically a [discriminator](/reference/glossary#discriminator) |
| `data.encode(encoding, value)` | A value encoded as `u8`, `u16`, `u32`, `u64`, `i64`, `u128`, `pubkey`, `bool`, or `bytes` |

The unsigned encodings `u8`, `u16`, `u32`, and `u64` accept a `u64` or `u128` value and fail the run
if it does not fit, so a template can write a one-byte field without giving up checked arithmetic.
A `bytes` part is inserted as is, with no length prefix; if the program expects a length, add it as
a separate part, such as `data.encode('u32', expression.bytesLength(value))`.

### Output {#output}

Two steps hand values out of a run. Both build their bytes from the same parts as invocation data,
at most 1,024 bytes, counting a `bytes` value at its maximum length. The compiler refuses an output
that breaks the size, tag, or placement rules below; the verifier refuses one built another way,
with `InvalidOutput` (6130).

`step.emit(parts)` writes one base64 `Program data:` line to the transaction's logs, for indexers
and clients to read.

- **Anywhere, every time.** An `emit` can go anywhere, loops included, and runs every time the run
  reaches it.
- **A tag first.** Its first part must be a literal tag of at least 4 bytes that does not start
  with `BEV`, so the line cannot pass for Ballista's own
  [run event](/guide/errors-and-events#run-events), which starts with `BEV1`.
- **The tag doesn't identify the template.** The line names no program; the `invoke` lines around
  it do, and `parseProgramData` (TypeScript) and `program_data` (Rust) follow them. Any template can
  log the same tag, so before trusting a line, check which template ran: the run instruction's
  template account, or the run event's template address.
- **Logs can be cut short.** Solana keeps 10,000 bytes of a transaction's logs by default, counting
  every program's lines, then writes `Log truncated` and drops the rest. Base64 makes an `emit`'s
  line a third longer than its bytes, so many emits, or a transaction whose other programs log a
  lot, can lose lines while the run itself succeeds.
- **A failed transaction keeps its logs.** Lines logged before a later failure still appear, so
  check that the transaction succeeded before you trust them.

`step.setReturnData(parts)` sets the run's return data: bytes a program hands back to whoever
invoked it.

- **Once, at the end.** It may appear once, outside every loop, with no invocation after it,
  because invoking any program clears return data.
- **Read by the caller.** A program that runs the template through a CPI can read it as soon as the
  call returns; another template does so with [`expression.returnData`](#return-data).
- **It names the program, not the template.** It proves only that Ballista set it. A template that
  reads a nested run's return data must pin the inner template's address, or a run of any template
  could supply it.
- **A transaction's return data is its last instruction's.** Each instruction starts with none, so
  any instruction after the run replaces it. Put the run last, or read the run's `Program return:`
  log line. That line names the program whose call ended, not the one that set the bytes, so after
  a run that sets none, it can show a called program's bytes under Ballista's name.

## Loops

Loops are the only way to repeat steps. There are two kinds:

- `step.forEach` runs its steps once for each batch row the caller supplies.
- `step.repeat` runs its steps a counted number of times.

A template can hold up to eight loops of either kind. They sit at the top level and run one after
another, and a loop cannot contain another loop. Each body holds 1 to 64 steps. Every loop has a
declared maximum, the batch's maximum rows or a count loop's `max`, so the worst-case number of CPIs
is known before the template ever runs: for each loop, the invocations in its body times its
maximum, plus the invocations outside loops, plus 3 for each [registry entry](#registries). That
total may not exceed 64.

### Batches

A batch declares a maximum number of rows (1 to 60), an optional minimum (default 0), and a row of
1 to 8 named accounts. The caller supplies the rows at run time, and steps inside a `forEach` refer
to the current row's accounts with `account.iteration(name)`. Each row can also carry
[row inputs](#inputs), so a payroll can pay each recipient a different amount. A run with
fewer rows than the minimum fails instead of succeeding without doing anything. Every `forEach` in
a template runs over the same rows, from the first. A template with a batch needs at least one
`forEach`, and a `forEach` needs a batch.

A row cannot change which accounts a CPI forwards: an invocation inside a `forEach` forwards the
same account group on every row.

### Count loops

`step.repeat(count, steps, { max })` runs its steps `count` times, for work repeated a number of
times chosen at run time, such as one transfer per round. `count` is a `u64` expression, such as an
input or a value read from an account. It is evaluated once, before the first pass, so nothing in
the body can change how many passes run. `max` is a fixed number from 1 to 255, and the worst case
counts every pass. A run whose count is above `max` fails with `LoopCountExceeded`, and a count of 0
skips the body.

A count loop has no rows, so its body cannot use `account.iteration` or `expression.rowInput`.

### Carried values

Values pass from one row or pass to the next only through a carry. A binding created before the
loop and listed in the loop's `carry` option can be reassigned inside the body with `step.assign`.
It must keep the same type (and, for `bytes`, the same maximum length), and it can still be read
after the loop. This is how a template keeps a running total, which a requirement after the loop
can then check. A binding created inside the body without being carried is recreated on every pass
and cannot be read after the loop.

## Registries

A registry keeps state between runs, such as a spending cap, a counter, or an allowlist. The
template declares each registry's fields. The values live in entries: accounts that Ballista owns,
one for each template, registry, and key. Only runs of the template can write its entries, and
anyone can read them. [Remember state between runs](/guide/registries) works through examples.

### Declaring registries

- **`registries`** maps each registry's name to its fields, in order. A field is a `bool`, `u64`,
  `i64`, `u128`, or `pubkey` (1, 8, 8, 16, or 32 bytes). Fields are packed in declaration order
  with no padding, and take 1 to 512 bytes in all. A template declares up to 8 registries, and a
  registry's position in `registries` is its index, part of each entry's address.
- **`account.registry(registry, { key, payer })`** declares a fixed account, not a batch-row
  account, that holds one entry. It declares the account writable and nothing else: a signer or
  executable flag, a pinned address or owner, or a minimum data length is refused.
- **`key`** is a `pubkey` expression that picks the entry, computed before the first step. It can
  read the fields of entries whose accounts come earlier in `accounts`, but not its own entry's or
  later ones.
  - A signer's address, such as `expression.accountKey('caller')`, gives one entry per caller.
  - Leaving `key` out gives the one template-wide entry, whose key is 32 zero bytes.
  - A key the caller chooses, such as an input, lets the caller pick any entry, including a fresh
    one.
- **`payer`** names a fixed account declared signer and writable. The first time a run opens the
  entry, the payer pays its [rent](/reference/glossary#rent).
- **`account.systemProgram()`** must also be declared by a template with registry accounts, since
  creating an entry calls the System program.

### Opening an entry

Every run opens each entry before the first step, in the order the accounts are declared. Each open
counts as 3 of the run's CPIs, since creating an entry can take three calls to the System program.

- **An existing entry** must be an account Ballista owns, of the registry's size, whose header
  names this template, this registry, and this key. Anything else fails the run with
  `InvalidRegistryEntry` (6025). This stops a caller from passing another caller's entry, or
  another template's.
- **A missing entry** is created. The account must hold no data, be owned by the System program,
  and sit at the entry's address, or the run fails with `InvalidRegistryEntry`. The payer pays the
  rent, or only the part still missing if the address already holds lamports. Ballista creates the
  account ([when Ballista signs](/guide/trust-model#signing)) and writes its header. The fields
  start at zero.
- **An entry already open in this run** fails with `InvalidRegistryEntry`. Two entries of one
  registry are one account when their keys come out equal, such as a sender who names themselves
  as the receiver. Entries open before the first step, so no `require` can catch this first; a
  client that compares the keys before sending can report it more clearly.
- **An entry account passed read-only** fails before the first step, with
  `AccountConstraintFailed` (6020).

A client derives an entry's address with `findRegistryEntryAddress(template, registryIndex, key)`
from `@jac0xb/ballista/kit`, or `find_registry_entry_address` in the Rust SDK, and passes it as the
account. `registryIndex(compiled, name)` gives a registry's index. The address is a PDA of the
Ballista program; its seeds are on [Wire format](/reference/wire-format#registry-entries).

### Reading and writing fields

- **`expression.registry(entry, field)`** reads a field, typed as declared.
  **`step.setRegistry(entry, field, value)`** writes one. Both work anywhere in the steps, loops
  included.
- **`entry` names the account, not the registry,** so two entries of one registry, such as a
  sender's and a receiver's, stay separate.
- **A write lands at once,** so later steps read the new value. If the run fails later, Solana
  undoes it with the rest of the transaction.
- **A write cannot be skipped.** To leave a field unchanged in some runs, write back its current
  value with `select`.
- **Fields are the only way in.** `accountData` and `accountDataBytes` of an entry are refused. The
  entry account's `key`, `owner`, `lamports`, `dataLength`, and `isEmpty` stay readable, as for any
  account.
- **A CPI may pass an entry read-only, never writable.** The compiler refuses it, and the verifier
  refuses the template with `InvalidRegistry` (6132). If another fixed account, a batch-row
  account or an account group member that a CPI passes writable turns out to be an open entry,
  the CPI fails with `RegistryReentry` (6026). The verifier also refuses an open after any CPI, so
  every CPI meets this check, and no other run can change an entry between this run's read and its
  write.

### Spending limits

`rateLimit({ registry, cap, refillPerSecond, amount })` returns the steps for a spending limit that
refills over time. It uses the entry's `u64` field `spent` and `i64` field `lastSpend`, or the
fields its `spent` and `lastSpend` options name. A run that would spend past `cap` fails at the
requirement `withinRateLimit`.

- **`cap` and `refillPerSecond` must be template constants,** built from literals and arithmetic.
  The helper refuses an input, a variable, and reads of accounts or of the transaction, since the
  caller may control them.
- **A registry field is the one exception,** but the helper cannot tell who wrote it. If any
  caller's run can write that field, every caller can set the cap, so write it only in a branch
  that only the author's runs take.
- **It cannot see the entry's key.** Key the entry by a signer's address, or leave the key out for
  one template-wide limit. A key from an input lets a caller open a fresh entry on every run.
- **`name`** names the requirement, `within<Name>`, and prefixes the variables the steps bind, so a
  template that uses two limits can tell their failures apart.

### What an entry cannot do

- **Belong to another template.** Each entry's header names its template, so a template cannot
  write another's entries, and the same template published at a new address starts with fresh
  entries.
- **Open per row.** Registry accounts are fixed accounts, opened once before the first step, so a
  template opens at most 8 entries and never one per batch row.
- **Close, resize, or change layout.** Its size and layout are fixed when the template is
  finalized, and its rent stays locked for good.

## What the language excludes

The language leaves these out on purpose:

- There are no jumps, recursion, nested loops, or unbounded loops. Every template therefore
  finishes, and its worst case in steps and calls is known before it runs. Its compute is not
  bounded that way: a loop of costly steps, such as PDA searches, can exhaust the transaction's
  budget and fail.
- A template cannot discover accounts at run time. The only accounts outside the schema are
  account group members, which a template can forward to a CPI but never read, check, or sign
  with. Everything a template checks is fixed in the stored template, where anyone can review it.
- There is no hidden state. A template keeps state of its own only in its
  [registry entries](#registries), accounts anyone can read. What a run does depends only on its
  inputs, its accounts, the chain state it reads, and, if it reads them, the transaction's other
  instructions.

Ballista also leaves out several things it could do in principle. It never takes custody of funds,
never schedules its own runs, never signs a template's CPIs
([When Ballista signs](/guide/trust-model#signing)), and does not pay keepers (bots that submit
transactions for a fee). A workflow that needs any of these needs its own program. Outside the
template's own CPIs, the only lamports a run moves are a new entry's rent, from the payer.

Ballista does not stop a template from being run again with the same inputs (replay protection).
A template that must refuse repeats can keep a counter or nonce in a registry entry.
[Why Ballista?](/guide/why-ballista) explains where that line falls.
