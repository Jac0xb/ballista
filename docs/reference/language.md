# Template language

This page describes the complete language a template can be written in: every value it can hold,
every source it can read, every computation it can perform, and every effect it can cause. The
surface is deliberately small. It is sized so that finalization can prove a template terminates,
never reads an uninitialized register, never addresses an account outside its schema, and never
exceeds its declared worst-case CPI count or generated data length. Anything that would defeat one
of those proofs is absent by construction rather than by convention.

A template is a declaration of typed inputs, an account schema, an optional batch range, and an
ordered list of steps. Compilation turns names into indices and expressions into register-based
bytecode. Nothing in the authoring document survives into the stored payload except its meaning:
every identifier you write is a compiler-side convenience.

## Values

Six types can occupy a register. The first five are fixed width; `bytes` carries a length bounded
at authoring time.

| Type | Width | Notes |
| --- | ---: | --- |
| `bool` | 1 byte | Guards, logical operators, and the condition of a select |
| `u64` | 8 bytes | Lamports, token amounts, slots, and offsets |
| `i64` | 8 bytes | Signed quantities, notably the clock's Unix timestamp |
| `u128` | 16 bytes | Intermediate products that would overflow 64 bits |
| `pubkey` | 32 bytes | Addresses, compared for equality and used as PDA seeds |
| `bytes` | declared maximum, up to 1024 | Opaque client-supplied payloads |

Numeric types never coerce implicitly. An addition, a comparison, or a select whose two branches
disagree on type is rejected at compile time rather than at run time, so a mismatch is a build
error and never a failed transaction. Conversions are explicit through a cast, which accepts a
numeric expression and produces `u64`, `i64`, or `u128`. A cast that cannot represent its input
fails the run.

There is no string type, no floating point, no map or struct, and no dynamic deserialization of
protocol account layouts. Values that look like structured data are read as fixed-width fields at
known offsets, and everything else stays opaque `bytes` that the template forwards without
interpretation.

## Inputs

A template declares up to thirty-two named inputs, each with one of the six types. A `bytes` input
additionally declares a maximum length between one and 1024, which becomes part of the worst-case
data accounting proven at finalization.

Inputs are encoded positionally in declaration order. A caller concatenates each value little
endian, with a `bool` as a single byte and a `bytes` value preceded by its length as a
little-endian `u16`. This is why the declaration order of a template is part of its interface:
renaming an input is free, but reordering the declarations changes the encoding that every existing
caller produces.

A batch may also declare up to eight row inputs, which are carried once per iteration and read
inside the loop with `expression.rowInput`. The run data is then the account-group length prefix
(one byte per declared group), the fixed values, and one row of values per iteration. Fixed and row
descriptors share the budget of thirty-two, and fixed values plus row values times the maximum
iteration count may not exceed 256.

## Accounts

An account schema is a capability declaration. Each named account states the maximum privilege a
run may use for that slot, and the program rejects any account the caller supplies that does not
satisfy it. A schema can require that the account be a signer, be writable, or be executable; it
can pin an exact address; it can pin the owning program; and it can demand a minimum data length.

The privileges flow one direction only. A CPI inside the template may request the same privilege as
its schema or less, never more, so reading a finalized template's schema tells you the upper bound
on what any invocation of it can do. Ballista forwards signer status that the outer transaction
already carries and never signs as its own PDA, which means a template cannot manufacture authority
that the sender did not already hold.

Two pinning rules are enforced at compile time because their absence would make the surrounding
logic meaningless. A program that is invoked, or that a PDA is derived against, must be marked
executable and must pin an address, since an unpinned program turns every downstream guarantee into
a guess. An account whose data is read must pin either an owner or an address, since a byte offset
into an account of unknown provenance is not a field, only a number. Both rules can be waived per
account with an explicit opt-out flag, which exists so that a template can deliberately accept
caller-chosen programs or untrusted data. The flag is named to be conspicuous in review.

Fixed-offset reads also raise a floor on the account's required data length automatically. If a
template reads a `u64` at offset 64, the compiler records that the account must hold at least
seventy-two bytes, and the run enforces it.

## Reading state

Five account fields are available without knowing anything about an account's layout: its key, its
owner, its lamport balance, its data length, and whether it is empty. These work on any account in
the schema, including an iteration account inside a loop.

Account data is read at a byte offset as one of eight widths: `bool`, `u8`, `u16`, `u32`, `u64`,
`i64`, `u128`, or `pubkey`. The narrow unsigned widths widen into `u64` in the register file, so a
`u8` flag and a `u64` amount compose in the same arithmetic without an explicit cast. The offset is
normally a constant fixed in the template. It can instead be a `u64` expression evaluated during
the run, which allows a read whose position depends on an input or on a value read earlier, at the
cost of the automatic data-length floor that a constant offset provides.

Return data from a CPI is readable at a byte offset and width, under one structural restriction:
the read must be the value of a binding that immediately follows an unconditional invocation. A
guarded invocation cannot be the source, because a skipped CPI produces no return data and the
resulting register would have no defined value. This is the same initialization proof that governs
every other register, applied to a value that comes from outside the VM.

The clock supplies the current slot as a `u64` and the Unix timestamp as an `i64`. Inside a loop,
the current iteration index is available as a `u64`.

A canonical program-derived address is computed from a pinned program and between one and fifteen
seeds. Each seed is a value of any type, contributing its own encoding, and no single seed may
exceed thirty-two bytes. The derivation is the canonical one, so it finds the bump rather than
accepting one, and it produces a `pubkey` that is typically compared against an account the caller
supplied. This is how a template checks that the account it was handed really is the associated
token account or vault it was supposed to be, rather than trusting the caller's word.

### Every source, enumerated

| Constructor | Result | Availability |
| --- | --- | --- |
| `expression.input(name)` | the input's declared type | Anywhere |
| `expression.variable(name)` | the binding's type | After the binding, in scope |
| `expression.snapshot(name)` | the binding's type | Identical to `variable` |
| `expression.bool(v)` | `bool` | Literal |
| `expression.u64(v)` | `u64` | Literal |
| `expression.i64(v)` | `i64` | Literal |
| `expression.u128(v)` | `u128` | Literal |
| `expression.pubkey(v)` | `pubkey` | Literal, 32 bytes |
| `expression.bytes(v)` | `bytes` | Literal, up to 1024 bytes |
| `expression.accountField(account, field)` | see below | Any schema account |
| `expression.accountData(account, offset, type)` | see below | Account must pin owner or address |
| `expression.returnData(type, offset?)` | see below | Only as a binding directly after an unguarded invoke |
| `expression.clockSlot()` | `u64` | Anywhere |
| `expression.clockUnixTimestamp()` | `i64` | Anywhere |
| `expression.loopIndex()` | `u64` | Inside a loop only |
| `expression.pda(program, seeds)` | `pubkey` | Program must pin an address; 1 to 15 seeds |

The `field` argument selects one of five layout-independent properties:

| Field | Result |
| --- | --- |
| `key` | `pubkey` |
| `owner` | `pubkey` |
| `lamports` | `u64` |
| `dataLength` | `u64` |
| `isEmpty` | `bool` |

The `type` argument of a data or return-data read selects one of eight widths. The three narrow
unsigned widths widen on the way into the register file, which is why a one-byte flag and an
eight-byte amount compose without an explicit cast:

| Read width | Result |
| --- | --- |
| `bool` | `bool` |
| `u8`, `u16`, `u32`, `u64` | `u64` |
| `i64` | `i64` |
| `u128` | `u128` |
| `pubkey` | `pubkey` |

## Computation

Arithmetic is checked. Addition, subtraction, multiplication, and division operate on two operands
of the same numeric type and fail the transaction on overflow, on underflow, and on division by
zero. There is no wrapping, no saturation, and no silent truncation anywhere in the language, which
means an arithmetic result that exists at all is a result that was representable.

Both operands of `min` and `max` must share a numeric type, and the result takes that type.
Equality and inequality apply to any two values of matching type, including pubkeys and bytes.
Ordered comparison is restricted to numerics. The three boolean connectives combine boolean
operands, and a select chooses between two same-typed branches on a boolean condition. A select
evaluates both branches before choosing, since the VM has no branch instruction; the choice picks a
register rather than skipping work.

### Every operator, enumerated

Arithmetic takes two operands of the same numeric type and returns that type. Each failure below
aborts the transaction.

| Constructor | Operands | Result | Fails when |
| --- | --- | --- | --- |
| `expression.add(a, b)` | matching numeric | same | The sum exceeds the type |
| `expression.subtract(a, b)` | matching numeric | same | The result goes below the type's minimum |
| `expression.multiply(a, b)` | matching numeric | same | The product exceeds the type |
| `expression.divide(a, b)` | matching numeric | same | The divisor is zero |
| `expression.min(a, b)` | matching numeric | same | Never |
| `expression.max(a, b)` | matching numeric | same | Never |
| `expression.cast(to, value)` | any numeric | `u64`, `i64`, or `u128` | The value does not fit the target |

Comparisons return a boolean. Equality accepts any two values of matching type, including addresses
and opaque bytes. The four ordered comparisons are numeric only.

| Constructor | Operands | Result |
| --- | --- | --- |
| `expression.equal(a, b)` | any matching type | `bool` |
| `expression.notEqual(a, b)` | any matching type | `bool` |
| `expression.lessThan(a, b)` | matching numeric | `bool` |
| `expression.lessThanOrEqual(a, b)` | matching numeric | `bool` |
| `expression.greaterThan(a, b)` | matching numeric | `bool` |
| `expression.greaterThanOrEqual(a, b)` | matching numeric | `bool` |

Four logical forms complete the surface. There is no exclusive-or, no implication, and no n-ary
form: wider conjunctions are built by nesting.

| Constructor | Operands | Result |
| --- | --- | --- |
| `expression.and(a, b)` | `bool`, `bool` | `bool` |
| `expression.or(a, b)` | `bool`, `bool` | `bool` |
| `expression.not(value)` | `bool` | `bool` |
| `expression.select(condition, ifTrue, ifFalse)` | `bool` plus two matching | the branch type |

Neither connective short-circuits. Both operands are evaluated before the combining instruction
runs, because the machine has no branch. This matters only for cost, never for correctness, since
no expression has a side effect.

## Bindings

A binding evaluates an expression once, at its position in the step list, and holds the result in a
register for the remainder of the run. This is what makes a before-and-after comparison possible.
Every expression is otherwise re-evaluated wherever it appears, so a balance read on either side of
an invocation yields two independent reads, and without a binding the earlier value is
unrecoverable. The operation is spelled two ways for readability: one name reads naturally in
pre/post checks, the other everywhere else, and they compile identically.

Bindings are immutable and lexically scoped, and a name cannot be redefined in the scope that
already holds it. They allocate no account, cost no rent, and do not survive the transaction. One
exception to immutability exists for loops, described below.

## Assertions

There is exactly one assertion primitive. `step.require(condition, label?)` evaluates a boolean and
aborts the whole transaction unless it is true, rolling back every effect the run has already
caused, including CPIs that already succeeded. The optional label is carried in the source map so a
failure can be attributed to a specific check rather than to the template as a whole.

That single primitive covers every assertion in the language, because the interesting part is the
expression, not the statement. Anything in the two tables above that yields a boolean is a valid
condition. In practice the useful shapes are a small set. An account can be checked for identity by
comparing its key against a literal or a derived address, or for provenance by comparing its owner.
A balance or token amount can be bounded by comparing a read against an input. A deadline is a
comparison against the clock. A delta is a comparison against a binding captured before an
invocation, which is the one shape that cannot be expressed without a binding. A compound policy is
those parts combined with the logical connectives, and an override is the whole thing disjoined
with an input flag.

Two helpers wrap the derivation shape, since it is verbose and easy to get subtly wrong. Both
return an ordinary requirement step and introduce no new capability.

| Helper | Asserts |
| --- | --- |
| `assertPda({ account, program, seeds, label? })` | The account's key equals the canonical derivation of those seeds under that program |
| `assertAta({ associatedTokenAccount, owner, mint, tokenProgram, associatedTokenProgram, label? })` | The account is the canonical associated token account for that owner and mint |

`assertAssociatedTokenAccount` is an alias of the second. Both exist because checking that a caller
handed you the account it claimed is the most common assertion in a template, and the most
consequential one to omit.

## Steps and control flow

A template holds up to 128 top-level steps, executed in order. There are five kinds.

A requirement evaluates a boolean and aborts the entire Solana transaction unless it is true, which
rolls back every effect the run has already caused, including CPIs that already succeeded.

A binding step introduces a name, and an assignment step updates one, subject to the loop rules
below.

An invocation performs a CPI against a pinned program. It lists up to sixty-four accounts with the
privileges it wants for each, may name one [account group](/guide/account-groups) whose caller-supplied
members follow those accounts without signer status, and builds its instruction data from up to
sixty-four parts. A part
is either a literal byte string fixed in the template, typically a discriminator, or a register
encoded at a chosen width. The available encodings are `u8`, `u16`, `u32`, `u64`, `i64`, `u128`,
`pubkey`, `bool`, and `bytes`. The three narrow unsigned encodings accept a `u64` or `u128`
register and check the value fits at run time, which is how a template writes a one-byte field
without giving up checked arithmetic. A `bytes` part is inserted raw, with no length prefix, so a
protocol that expects a length must be given one explicitly as a preceding part. Total generated
data is capped at 4096 bytes, and the worst case is proven at finalization rather than discovered
during a run.

An invocation may carry a guard, which makes that single CPI optional. The guard is evaluated in
place, and if it is false the invocation is skipped and execution continues with the next step. A
guard is the right tool when the work is legitimately unnecessary, such as creating an account that
may already exist; a requirement is the right tool when a false condition means something is wrong.
The run event records which invocations actually fired.

A loop is the only control flow the language has. A template may contain exactly one, it must be at
the top level, it cannot nest, and it runs forward over the batch rows the caller supplied. Its
body holds between one and sixty-four steps. The loop's trip count is bounded by the batch range
declared in the template, so worst-case CPI expansion is the body's invocation count multiplied by
the maximum iteration count, and that product is known before the template is ever run.

### Every step and data part, enumerated

| Constructor | Effect |
| --- | --- |
| `step.require(condition, label?)` | Abort the transaction unless the boolean holds |
| `step.let(name, value, label?)` | Bind an expression to a name for the rest of the run |
| `step.snapshot(name, value, label?)` | Identical to `let`, named for pre/post checks |
| `step.assign(name, value, label?)` | Reassign a carried binding; loop body only |
| `step.invoke({ program, accounts, data, when?, programAddress?, label? })` | Perform a CPI, optionally guarded |
| `step.forEach(steps, { carry?, label? })` | Iterate the batch rows; one per template, top level only |

Instruction data is assembled from two part constructors.

| Constructor | Produces |
| --- | --- |
| `data.literal(bytes)` | Fixed bytes, typically a discriminator |
| `data.encode(encoding, value)` | A register encoded as `u8`, `u16`, `u32`, `u64`, `i64`, `u128`, `pubkey`, `bool`, or `bytes` |

Accounts are named with `account.fixed(name)` for a schema account and `account.iteration(name)`
for the current row's account inside a loop.

## Batches

A batch declares a maximum iteration count, an optional minimum, and a row shape of between one and
eight named accounts. The caller supplies rows at run time, and steps inside the loop reach that
iteration's accounts by name. The minimum exists so that a run supplying too few rows fails rather
than succeeding vacuously.

A row carries accounts and, when the batch declares row inputs, one value per row input. Steps
inside the loop read the current row's values with `expression.rowInput`, so a payroll that pays each
recipient a different caller-supplied amount is expressible. What a row cannot carry is a CPI shape:
an invocation inside the loop forwards the same account group on every iteration.

Values cross iteration boundaries only through an explicit carry. A binding created before the loop
and named in the loop's carry list may be reassigned inside the body, must keep its exact type and
size across the assignment, and remains readable after the loop ends. This is what makes running
totals possible, and a requirement placed after the loop can then assert something about the batch
as a whole. Any binding created inside the body without being carried is rebuilt each iteration and
cannot be read afterward.

## Bounds

Every limit below is fixed in the wire format and checked at finalization.

| Bound | Limit |
| --- | ---: |
| Registers | 64 |
| Inputs | 32 |
| `bytes` input or literal length | 1024 |
| Top-level steps | 128 |
| Steps in a loop body | 64 |
| Runtime accounts, fixed plus batch range plus groups | 120 |
| Row inputs per batch row | 8 |
| Input values per run, fixed plus rows | 256 |
| Account groups | 8 |
| Accounts per CPI, declared plus group | 64 |
| Data parts per CPI | 64 |
| Generated CPI data | 4096 bytes |
| Batch iterations | 60 |
| Accounts per batch row | 8 |
| PDA seeds | 15 |
| Bytes per PDA seed | 32 |

The runtime account budget is the one that binds soonest in practice. Fixed accounts plus the row
width multiplied by the maximum iteration count must fit within 120, so a full eight-account row
leaves room for fourteen iterations and eight fixed accounts. Account groups are sized at run time
within the same total, and the 1,232-byte transaction limit usually binds before it does.

## What the language excludes

The absences are load-bearing, and each one buys a specific proof.

There are no backward jumps, no recursion, no nested loops, and no unbounded loop form, which is
what makes termination decidable at finalization instead of being a runtime compute-budget gamble.
There is no dynamic account discovery. The only accounts outside the schema are the members of an
account group, which a template can forward to a CPI but never read, constrain, or sign with, so
everything a template checks is still a fixed, auditable property of the stored template. There is no
mutable template state and no variable that outlives a transaction, so a finalized template is a
pure function of its inputs, its accounts, and the chain state it reads.

Ballista also declines several things it could technically do. It never signs as its own PDA, holds
custody, schedules its own execution, enforces replay policy, or pays keepers. A workflow needing
any of those needs a program, and the guide's opening page draws that line in more detail.
