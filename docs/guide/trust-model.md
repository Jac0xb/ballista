# Trust model

Ballista runs other people's programs with the caller's accounts. Knowing exactly who is protected
from whom is the difference between a template that is a guarantee and one that is a suggestion.

## Who supplies what

| Party | Supplies | Controls |
| --- | --- | --- |
| Template author | The immutable bytecode: account constraints, inputs, steps | What can happen, in what order, under which conditions |
| Caller | Every runtime account, every input value, every signature | Which concrete accounts the template sees |
| Invoked programs | Their own instruction semantics and errors | What each CPI does with the accounts it receives |
| Ballista | Verification at finalize, execution at run, privilege forwarding | Nothing else: no PDA signing, no custody, no state |

Ballista never signs. A CPI can only mark an account as a signer or writable if the account
constraint declared that privilege and the outer transaction actually granted it. Everything the
template does, the caller could have done in a hand-built transaction. What the template adds is
that the steps, guards, and invariants run atomically and exactly as written.

## Pins

An account constraint can pin two facts about the account the caller passes:

- **`address`** fixes the account to one key. Every program a template invokes or derives PDAs
  with must be pinned this way, otherwise the caller can substitute any executable and the
  template's CPIs mean nothing.
- **`owner`** fixes the program that owns the account's data, so fixed-offset reads have a known
  layout. A `u64` at offset 64 is a token balance only if the Token Program owns the account.

The compiler enforces both by default and refuses to compile a template that invokes an unpinned
program or reads data from an account with neither pin. Helpers such as `systemTransfer` also
declare which program they target, so pinning the wrong address is a compile error, not a runtime
surprise.

Static reads raise the account's minimum data length automatically, and the verifier rejects any
read that extends past the declared minimum. A read that could fail at run time therefore fails at
compile time instead.

## Opting out

`unsafeUnpinned: true` on an account constraint disables the pin requirements for that account.
The template then trusts whatever the caller supplies. This is reasonable when the caller is the
only party affected, for example a wallet's own maintenance template. It is not reasonable when a
third party relies on the template as a guard, because the caller can then point the CPI at a
program of their choosing.

The flag is deliberately verbose so it stands out in review.

## Error attribution

Errors raised by an invoked program pass through `Run` untouched, including custom codes that
happen to fall in Ballista's own numeric range. Ballista's failures are typed separately inside the
executor and only then mapped to codes, so a callee's `6001` is never mistaken for
`InvalidTemplateAccount`. See [Errors and events](/guide/errors-and-events) for the code layout.

## Immutable deployments

Each version of the Ballista program is deployed immutably under its own address; there is no
upgrade authority that can change what a stored template means. Template addresses are derived
under the program that finalized them, so a template is bound to one deployment forever. Moving to
a new bytecode version means uploading templates again under the new program address.

The SDKs take the program address as a parameter everywhere for this reason. The address in the
README is the version 2 devnet deployment; version 3 templates need the version 3 deployment.
