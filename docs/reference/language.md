# Template language

Everything a template can contain: the types of values it works with, the inputs
and accounts it declares, the expressions it can compute, and the steps it runs. Function names are
from the TypeScript SDK. The Rust `ProgramBuilder` produces the same bytecode at a lower level; see
[Rust SDK](/reference/rust).

A template declares typed inputs, the accounts it expects, an optional batch of repeated rows, and
an ordered list of steps. The compiler turns it into bytecode: a list of fixed-size instructions,
each identified by a number called its opcode. Instructions keep intermediate values in registers,
numbered slots that each hold one value for the length of a run. The names you give inputs,
accounts, and bindings are replaced by numbers and are not stored on chain.

When a template is finalized (made permanent and runnable), the Ballista program's verifier checks
its bytecode once. It confirms that the template always finishes, never reads a register before
writing it, only refers to accounts it declared, and stays within the limits on CPIs
(cross-program invocations, meaning calls from the template to other programs) and CPI instruction
data even in the worst case. The language leaves out anything that would stop those checks from
working.

## Values

A register holds one of six types. Five have a fixed width. A `bytes` value has a maximum length
that is declared when the template is written.

| Type | Width | Typical use |
| --- | ---: | --- |
| `bool` | 1 byte | Conditions and flags |
| `u64` | 8 bytes | Lamports (1 SOL is 1,000,000,000 lamports), token amounts, slot numbers, byte offsets |
| `i64` | 8 bytes | Signed values, such as the clock's Unix timestamp |
| `u128` | 16 bytes | Intermediate products that would overflow 64 bits |
| `pubkey` | 32 bytes | Account addresses, compared for equality or used as seeds of a program-derived address (PDA) |
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

Callers send inputs as one byte string, with the values in declaration order. Numbers are
little-endian, a `bool` is one byte, and a `bytes` value is a little-endian `u16` length followed
by the bytes. Declaration order is therefore part of a template's interface. Renaming an input
leaves the compiled template unchanged, but reordering inputs changes the encoding every caller
must produce.

A batch can also declare up to eight row inputs: values the caller supplies once per row, read
inside the loop with `expression.rowInput(name)`. The run data (the bytes sent with each run) then
holds one length byte per [account group](/guide/account-groups) (a list of accounts whose size the
caller chooses), the fixed input values, and one set of row input values per row. Fixed and row
inputs share the limit of 32. The number of fixed inputs, plus the number of row inputs times the
maximum row count, may not exceed 256.

## Accounts

The account schema lists, by name, the accounts a template uses. Each entry states the most a run
may do with that account, and the program rejects a run whose account does not satisfy it. An
entry can require the account to be a signer (the transaction carries its signature), writable
(the transaction allows programs to modify it), or executable (a program). It can also pin the
account's address or its owner program, and require a minimum data length. To pin a value is to
fix it in the template, so that a run fails if the caller passes anything else.

A CPI in the template may ask for the same privileges as the account's schema entry or fewer,
never more. Reading a finalized template's schema therefore tells you the most any run of it can
do with the accounts it declares. Ballista passes on signatures the transaction already carries
and never signs as its own PDA, so a template cannot gain authority the transaction did not
already have.

The compiler enforces two pinning rules:

- A program that the template invokes, or derives a PDA from, must be marked `executable` and must
  pin its `address`. Otherwise the caller could substitute any program.
- An account whose data the template reads must pin its `owner` or its `address`. A byte offset
  only has a meaning when you know which program wrote the data.

Setting `unsafeUnpinned: true` on an account waives both rules for that account. Use it when a
template deliberately accepts a program or data that the caller chooses.

A read at a fixed offset also raises the account's minimum data length. If a template reads a
`u64` at offset 64, the compiler records that the account must hold at least 72 bytes, and every
run checks it.

## Reading state

Five account fields can be read without knowing the account's layout: its address (`key`), its
`owner`, its `lamports` balance, its `dataLength`, and `isEmpty`, which is true when the account
holds no data. These work on any account in the schema, including a row account inside the loop.

Account data is read at a byte offset as one of eight types: `bool`, `u8`, `u16`, `u32`, `u64`,
`i64`, `u128`, or `pubkey`. The narrow unsigned types `u8`, `u16`, and `u32` are widened to `u64`
when read, so a one-byte flag and an eight-byte amount can be combined without a cast. A `bool`
read fails the run unless the byte is 0 or 1. The offset is usually a constant. It can instead be a
`u64` expression evaluated during the run, so the position can depend on an input or on an earlier
read. Such a read does not raise the account's minimum data length, and it fails the run if it
extends past the end of the data.

A CPI's return data (bytes the invoked program hands back) can be read at a byte offset as the
same eight types, with one restriction: the read must be the value of a `let` step placed directly
after an invoke that has no guard. A guarded invoke might be skipped, which would leave no return
data and a register with no defined value. The run also fails if the return data was not set by the
program just invoked, or is too short for the read.

The clock gives the current slot (Solana's block-by-block time counter) as a `u64` and the Unix
timestamp as an `i64`. Inside the loop, `expression.loopIndex()` gives the current row's zero-based
index as a `u64`.

A program-derived address (PDA) is an address computed from a program ID and a list of seeds. No
private key exists for it. A template derives one from a pinned program and 1 to 15 seeds. Each
seed can be a value of any type and contributes that type's encoding: a `u64` adds 8 little-endian
bytes, a `pubkey` its 32 bytes. No seed may exceed 32 bytes. The result is a `pubkey`, usually
compared with an account the caller supplied. This is how a template checks that it was given the
right vault or associated token account (ATA: the standard token account for a wallet and a mint)
instead of trusting the caller.

Deriving a PDA needs a bump: one extra seed byte, tried from 255 downward until the result is a
valid program address. The first value that works is the canonical bump. By default the template
searches for it, and each attempt costs 1,500 compute units (Solana's measure of execution cost).
Passing a `u64` bump as the third argument derives the address once instead. A bump above 255, or
one that does not produce a valid program address, fails the run.

The two forms prove slightly different things. Without a bump, a match proves the account is the
canonical PDA for those seeds. With a bump, a match proves the account is the PDA for those seeds
and that bump. Several bumps can produce valid addresses, so when the caller chooses the bump, a
match does not prove the address is the canonical one. If the account must be the canonical PDA,
do not take the bump from the caller: leave it out, or, when every seed is fixed in the template,
write the canonical bump in as a constant.

### Every source, enumerated

| Constructor | Result | Where it can be used |
| --- | --- | --- |
| `expression.input(name)` | The input's declared type | Anywhere |
| `expression.rowInput(name)` | The row input's declared type | Inside the loop only |
| `expression.variable(name)` | The binding's type | After the binding, while it is in scope |
| `expression.snapshot(name)` | The binding's type | Same as `variable` |
| `expression.bool(v)` | `bool` | Literal |
| `expression.u64(v)` | `u64` | Literal |
| `expression.i64(v)` | `i64` | Literal |
| `expression.u128(v)` | `u128` | Literal |
| `expression.pubkey(v)` | `pubkey` | Literal, 32 bytes |
| `expression.bytes(v)` | `bytes` | Literal, up to 1,024 bytes |
| `expression.accountField(account, field)` | See the field table below | Any schema account |
| `expression.accountData(account, offset, type)` | See the read-type table below | The account must pin its owner or address |
| `expression.returnData(type, offset?)` | See the read-type table below | Only as the value of a `let` directly after an unguarded invoke; `offset` defaults to 0 |
| `expression.clockSlot()` | `u64` | Anywhere |
| `expression.clockUnixTimestamp()` | `i64` | Anywhere |
| `expression.loopIndex()` | `u64` | Inside the loop only |
| `expression.pda(program, seeds, bump?)` | `pubkey` | The program must pin an address; 1 to 15 seeds; `bump` is a `u64` |

The `field` argument selects one of five properties that do not depend on the account's layout:

| Field | Result |
| --- | --- |
| `key` | `pubkey` |
| `owner` | `pubkey` |
| `lamports` | `u64` |
| `dataLength` | `u64` |
| `isEmpty` | `bool`, true when the account holds no data |

The `type` argument of an account-data or return-data read selects one of eight types. The narrow
unsigned types become `u64`:

| Read type | Result |
| --- | --- |
| `bool` | `bool` |
| `u8`, `u16`, `u32`, `u64` | `u64` |
| `i64` | `i64` |
| `u128` | `u128` |
| `pubkey` | `pubkey` |

## Computation

Arithmetic is checked. `add`, `subtract`, `multiply`, and `divide` take two operands of the same
numeric type and fail the run if the result does not fit the type or the divisor is zero. Nothing
wraps around, saturates, or is silently truncated. Division discards the remainder.

`min` and `max` take two operands of the same numeric type and return that type. `equal` and
`notEqual` accept any two values of the same type, including pubkeys and bytes. The ordered
comparisons accept numeric types only. `and`, `or`, and `not` combine booleans, and `select` picks
one of two values of the same type depending on a boolean condition.

The bytecode has no jump instruction, so nothing inside an expression is skipped. `select`
evaluates both branches before it chooses, and `and` and `or` evaluate both operands. If any of
those fails (an overflow, a division by zero, a bad read), the run fails, even when the other side
alone would have decided the result. For example, a `select` that divides by a value only when the
value is non-zero still fails when it is zero, because the division runs either way.

### Every operator, enumerated

Arithmetic takes two operands of the same numeric type and returns that type. Each failure below
fails the transaction.

| Constructor | Operands | Result | Fails when |
| --- | --- | --- | --- |
| `expression.add(a, b)` | matching numeric | same | The result does not fit the type |
| `expression.subtract(a, b)` | matching numeric | same | The result does not fit the type (for `u64` and `u128`, it would be below zero) |
| `expression.multiply(a, b)` | matching numeric | same | The result does not fit the type |
| `expression.divide(a, b)` | matching numeric | same | The divisor is zero, or the result does not fit (the `i64` minimum divided by −1) |
| `expression.min(a, b)` | matching numeric | same | Never |
| `expression.max(a, b)` | matching numeric | same | Never |
| `expression.cast(to, value)` | any numeric | `u64`, `i64`, or `u128` | The value does not fit the target |

Comparisons return a boolean. Equality accepts any two values of the same type, including
addresses and bytes. The four ordered comparisons accept numeric types only.

| Constructor | Operands | Result |
| --- | --- | --- |
| `expression.equal(a, b)` | any matching type | `bool` |
| `expression.notEqual(a, b)` | any matching type | `bool` |
| `expression.lessThan(a, b)` | matching numeric | `bool` |
| `expression.lessThanOrEqual(a, b)` | matching numeric | `bool` |
| `expression.greaterThan(a, b)` | matching numeric | `bool` |
| `expression.greaterThanOrEqual(a, b)` | matching numeric | `bool` |

There are four logical forms. There is no exclusive-or, and `and` and `or` take exactly two
operands; nest them to combine more.

| Constructor | Operands | Result |
| --- | --- | --- |
| `expression.and(a, b)` | `bool`, `bool` | `bool` |
| `expression.or(a, b)` | `bool`, `bool` | `bool` |
| `expression.not(value)` | `bool` | `bool` |
| `expression.select(condition, ifTrue, ifFalse)` | `bool` plus two of the same type | The type of the two values |

## Bindings

A binding evaluates an expression once, at its place in the step list, and keeps the result in a
register for the rest of the run. Write one with `step.let(name, value)` or
`step.snapshot(name, value)`. The two compile identically; `snapshot` reads better in
before-and-after checks. Read the value back with `expression.variable(name)` or
`expression.snapshot(name)`.

Bindings are what make before-and-after comparisons possible. An expression written inline is
evaluated where it appears, so a balance read before a CPI and the same read after it are two
separate reads. Without a binding, the earlier value is lost.

A binding cannot be changed, and its name cannot be reused while the binding is in scope. A binding
is visible to the steps after it; one made inside the loop body is visible only inside the body.
Bindings create no account and end with the transaction. The one exception to "cannot be changed"
is a carried binding in the loop, described under [Batches](#batches).

## Assertions

`step.require(condition, label?)` is the only assertion. It evaluates a boolean and, if it is
false, fails the whole transaction. Solana then undoes everything the run did, including CPIs that
already succeeded. The optional label (up to 64 characters) is kept in the compiled template's
source map, which links each bytecode instruction to the step that produced it, so a failure can
be traced to a specific check. Labels are not stored on chain.

Any expression that produces a boolean can be a condition. Common patterns:

- **Identity:** compare an account's `key` with a literal or a derived address.
- **Owner:** compare an account's `owner` with the program that should own it.
- **Bounds:** compare a balance or token amount with an input.
- **Deadline:** compare the clock with an input.
- **Change across a CPI:** compare a value read after the CPI with a binding captured before it.
  This is the one pattern that needs a binding.
- **Combined policy:** join the checks above with `and` and `or`. To let the caller override a
  check, combine it with a `bool` input using `or`.

Two helpers write the common address checks for you. Each returns an ordinary `require` step.

| Helper | Checks that |
| --- | --- |
| `assertPda({ account, program, seeds, bump?, label? })` | The account's address is the PDA derived from those seeds under that program |
| `assertAta({ associatedTokenAccount, owner, mint, tokenProgram, associatedTokenProgram, bump?, label? })` | The account is the associated token account for that owner, mint, and token program |

`assertAssociatedTokenAccount` is another name for `assertAta`. Checking that the caller passed
the account it claims is the most common check in a template.

## Steps and control flow

A template has up to 128 top-level steps, run in order. There are five kinds:

- A **requirement** (`step.require`) fails the whole transaction if its condition is false.
- A **binding** (`step.let` or `step.snapshot`) names a value.
- An **assignment** (`step.assign`) updates a carried binding inside the loop.
- An **invocation** (`step.invoke`) performs a CPI.
- A **loop** (`step.forEach`) runs its steps once per batch row.

An invocation calls a program whose address the template pins. It lists up to 64 accounts, each
with the signer and writable flags it needs, and builds its instruction data from up to 64 parts.
It may also name one [account group](/guide/account-groups): a list of accounts the caller
supplies at run time, passed after the listed accounts and never as signers.

Each data part is either literal bytes fixed in the template, such as an instruction discriminator
(the leading bytes that tell a program which instruction to run), or a value encoded at a chosen
width: `u8`, `u16`, `u32`, `u64`, `i64`, `u128`, `pubkey`, `bool`, or `bytes`. The unsigned
encodings `u8`, `u16`, `u32`, and `u64` accept a `u64` or `u128` value and fail the run if it does
not fit, so a template can write a one-byte field without giving up checked arithmetic. A `bytes`
part is inserted as is, with no length prefix; if the program expects a length, add it as a
separate part. An invocation's data can be at most 4,096 bytes, and the verifier checks the worst
case when the template is finalized.

An invocation can have a guard (`when`), which makes that one CPI optional. The guard is evaluated
in place. If it is false, the CPI is skipped and the run continues with the next step. Use a guard
when the work is sometimes unnecessary, such as creating an account that may already exist. Use a
requirement when a false condition means something is wrong. If the template sets `emitEvent`, the
run event records which invocations ran.

The loop is the only way to repeat steps. A template can have at most one, at the top level, and
loops cannot be nested. The loop runs once for each batch row the caller supplies, and its body
holds 1 to 64 steps. Because the template declares the maximum number of rows, the worst-case
number of CPIs is known before the template ever runs: the invocations in the body times the
maximum rows, plus the top-level invocations. That total may not exceed 64.

### Every step and data part, enumerated

| Constructor | Effect |
| --- | --- |
| `step.require(condition, label?)` | Fail the transaction unless the condition is true |
| `step.let(name, value, label?)` | Bind a value to a name for the rest of the run |
| `step.snapshot(name, value, label?)` | Same as `let`; the name suits before-and-after checks |
| `step.assign(name, value, label?)` | Reassign a carried binding; loop body only |
| `step.invoke({ program, accounts, data, when?, accountGroup?, programAddress?, label? })` | Perform a CPI, optionally guarded by `when` |
| `step.forEach(steps, { carry?, label? })` | Run the steps once per batch row; one per template, top level only |

Each entry in an invocation's `accounts` is `{ account, signer?, writable? }`. `programAddress`
states which program the step is written for; compilation fails if the `program` account pins a
different address.

Instruction data is built from two part constructors.

| Constructor | Produces |
| --- | --- |
| `data.literal(bytes)` | Fixed bytes, typically a discriminator |
| `data.encode(encoding, value)` | A value encoded as `u8`, `u16`, `u32`, `u64`, `i64`, `u128`, `pubkey`, `bool`, or `bytes` |

Accounts are named with `account.fixed(name)` for an account in the schema and
`account.iteration(name)` for an account in the current batch row, inside the loop.

## Batches

A batch declares a maximum number of rows (1 to 60), an optional minimum (default 0), and a row of
1 to 8 named accounts. The caller supplies the rows at run time, and steps inside the loop refer to
the current row's accounts with `account.iteration(name)`. A run with fewer rows than the minimum
fails instead of succeeding without doing anything.

When the batch declares row inputs, each row also carries one value per row input, read with
`expression.rowInput(name)`. A payroll can therefore pay each recipient a different amount. A row
cannot change which accounts a CPI forwards: an invocation inside the loop forwards the same
account group on every row.

Values pass from one row to the next only through a carry. A binding created before the loop and
listed in the loop's `carry` option can be reassigned inside the body with `step.assign`. It must
keep the same type (and, for `bytes`, the same maximum length), and it can still be read after the
loop. This is how a template keeps a running total, which a requirement after the loop can then
check. A binding created inside the body without being carried is recreated on every row and
cannot be read after the loop.

## Bounds

Every maximum, such as 64 registers, 64 CPIs per run and 120 runtime accounts, is on
[Limits](/reference/limits). The TypeScript SDK adds a few of its own, such as 60 batch rows.

## What the language excludes

The language leaves these out on purpose:

- There are no jumps, recursion, nested loops, or unbounded loops. Every template is therefore
  known to finish, and its worst case is known before it runs, rather than being cut off by the
  transaction's compute budget.
- A template cannot discover accounts at run time. The only accounts outside the schema are
  account group members, which a template can forward to a CPI but never read, check, or sign
  with. Everything a template checks is fixed in the stored template, where anyone can review it.
- No state survives a transaction. What a run does depends only on its inputs, its accounts, and
  the chain state it reads.

Ballista also leaves out several things it could do in principle. It never signs as its own PDA,
never takes custody of funds, never schedules its own runs, does not stop a template from being run
again with the same inputs (replay protection), and does not pay keepers (bots that submit
transactions for a fee). A workflow that needs any of these needs its own program.
[Why Ballista?](/guide/why-ballista) explains where that line falls.
