# Failure modes and recovery

What can go wrong with a template, and what you can do about it.

## A run fails part way through

**What happens:** nothing. A run is all or nothing. If any check or call fails, Solana undoes the
whole transaction, including calls that had already succeeded. The caller still pays the
transaction fee.

**Recover:** read the error code to find the failing step, fix the inputs or accounts, and run
again. See [Errors and events](/guide/errors-and-events).

## The transaction runs too many instructions

**What happens:** a transaction runs at most 64 instructions, counting every CPI at every depth.
The call that would be the 65th fails the whole transaction with Solana's
`MaxInstructionTraceLengthExceeded`, not a Ballista code. Ballista counts only the template's own
calls, so a loop within its `max` can still overflow.

**Recover:** count one pass's entries in a simulation (each `invoke` line in the logs is one), then
lower the loop's count or split the work across transactions. See
[Instruction trace](/reference/limits#instruction-trace).

## A call nests too deep

**What happens:** Solana nests calls at most 5 frames deep, and a run called by the transaction is
the first. A call into frame 6 fails the whole transaction with Solana's `CallDepth` error, not a
Ballista code. Each template that runs another template adds a frame.

**Recover:** call the deep program from fewer frames: send the inner template's run as its own
instruction instead of nesting it, or pick a route that nests less. See
[Call depth](/reference/limits#call-depth).

## The template has a bug

**What happens:** a finalized template can never be changed or closed. The bug stays, and so does
the rent deposit paid to store it: those lamports are locked for good.

**Recover:**

1. Upload a fixed version under a new template ID. `findFreeTemplateId` finds one.
2. Point your callers at the new template address.
3. Tell anyone else who runs the old one. There is no way to disable it.

Before you finalize, test the template locally and keep it small, so the cost of a mistake is a
small locked deposit. An upload that is not yet finalized can still be cancelled with
`CancelTemplate`, which returns the deposit.

## The creator key is compromised

**What happens:** the key cannot touch templates that are already finalized. Their bytes are
locked. With the key, an attacker can:

- publish new templates under your creator address, at any free template ID;
- cancel or finish uploads you have started but not finalized.

**Recover:** stop using the key, upload under a new creator, and tell callers to use the new
addresses. Callers should trust a template by its exact address, or by reading its contents, not by
its creator alone.

## A called protocol changes its layout

**What happens:** a template stores the programs it calls, their instruction bytes, and the byte
offsets it reads. If a protocol upgrades its program and moves a field or changes an instruction,
the template still uses the old layout. Usually the run then fails: a `require`, an owner or
length check, or the protocol itself rejects it. A template that pins less, or reads a field whose
offset now holds something else, might instead run with the wrong value.

**Recover:** upload a new template for the new layout under a new ID. To fail safely in the
meantime, pin each account's `owner`, check the values you read with `require`, and set a minimum
data length that matches the layout you expect.

## The Ballista program changes

Each Ballista release is a new deployment at a new address, and templates belong to the deployment
that finalized them. An old template keeps working on its old deployment. To move to a new
release, upload the template again under it.
