# Trust model

This page explains who controls what when a template runs, what an account declaration can
guarantee, and where those guarantees stop. A template calls other programs with accounts the
caller supplies, so its checks protect you only if you know which parts the caller controls.

## Who supplies what

| Party | Supplies | Controls |
| --- | --- | --- |
| Template author | The template, which cannot change once finalized (checked and locked on chain): account declarations, inputs, steps | What can happen, in what order, and under which conditions |
| Caller | Every account, every input value, every signature | Which actual accounts and values the template works with |
| Called programs | Their own instruction behavior and errors | What each call does with the accounts it receives |
| Ballista | Checks at finalization, execution at run time, passing on signatures and write access | Nothing else: it never signs for a PDA (program-derived address), holds no funds, and keeps no state between runs |

Ballista never signs. A CPI (a call from the template to another program) can pass an account as a
signer (an account that signed the transaction) or as writable (allowed to change) only if the
account's declaration requires that privilege and the outer transaction actually granted it. A
template has no authority of its own: every call it makes, the caller could have made directly with
the same signatures. What the template adds is that its steps and checks run together, in one
transaction, exactly as written.

## Pins

To pin a fact about an account is to fix it in the template, so that a run fails if the caller
passes an account that does not match. An account declaration can pin two facts:

- **`address`** fixes the account to one exact address. Every program a template calls, or derives
  a PDA with, must be pinned this way. Otherwise the caller could substitute any program, and the
  template's calls would mean nothing.
- **`owner`** fixes the program that owns the account, so the template knows the layout of its
  data. A `u64` at byte offset 64 is a token balance only if the Token Program owns the account.

The TypeScript compiler enforces both unless you opt out, as described below. It refuses a template
that calls a program without a pinned address, or that reads the data of an account that pins
neither its owner nor its address. Helpers such as `systemTransfer` also declare which program they
target, so pinning the wrong address fails when you compile, not when someone runs the template.

A read at a fixed offset also raises the account's minimum data length automatically, and
finalization rejects any fixed-offset read that extends past the declared minimum. So a read that
could run off the end of an account fails when you build the template instead of during a run.

## Opting out

`unsafeUnpinned: true` on an account declaration turns off both requirements for that account, and
the template then accepts whatever the caller passes. That is reasonable when the caller is the only
party affected, such as a wallet's own maintenance template. It is not reasonable when someone else
relies on the template as a safeguard, because the caller can then point a call at any program they
choose.

The flag's name is long on purpose, so that it stands out in code review.

## Error attribution

Errors raised by a called program pass through a run unchanged, including custom codes that happen
to fall in Ballista's own ranges. Inside the program, Ballista keeps its own failures separate from
a callee's errors and only turns them into codes at the end, so it never relabels a callee's error
or adds its own context to it.

A code on its own does not say which program raised it, though: a callee's `6001` is the same number
as Ballista's `InvalidTemplateAccount`. The transaction logs show which program failed. See
[Errors and events](/guide/errors-and-events) for how codes are laid out.

## Immutable deployments

Each version of the Ballista program is deployed under its own address with no upgrade authority,
so nobody can change what a stored template means. A template's address is derived from the program
that finalized it, so each template belongs to that one deployment for good. A new deployment is a
different program, and templates must be uploaded again to use it.

For this reason the TypeScript SDK takes the program address as a parameter. The Rust SDK's
instruction builders use the constant `ballista_sdk::ID`, and `find_template_pda_for_program`
derives a template address under any other deployment. The program deployed on devnet today is an
earlier pre-release build that rejects templates compiled from this repository; see
[Devnet workflow](/guide/devnet).
