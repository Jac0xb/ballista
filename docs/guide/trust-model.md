# Trust model

Who controls what when a template runs, what Ballista checks, and where those checks stop. A
template calls other programs with accounts the caller supplies, so its checks protect you only if
you know which parts the caller controls.

## Who controls what

| Party | Controls | What limits them |
| --- | --- | --- |
| Template author | The declarations, inputs and steps | The [finalization checks](#finalization-checks); the template is fixed after them |
| Caller | Every account, input value and signature in the run | The template's declarations, pins and checks |
| Transaction builder | The caller's choices, when someone else builds the transaction, and its other instructions | The same, and what the signers check before signing |
| Other instructions | Any account the signers allow, before and after the run | A template sees them only through [introspection](/reference/language#introspection) |
| Called programs | What each call does with the accounts it gets | Their own checks, and the template's checks after the call |
| Called programs' upgrade authorities | What those programs do after an upgrade | Nothing in the template. Call programs whose authority you trust |
| Anyone, later (replay) | A second run, with the same inputs or the same signed message, given the signatures it needs | Solana refuses only an identical transaction. Use a deadline, or record each use in a [registry entry](/guide/registries) |
| Ballista | Its checks, and the registry entries templates declare | It holds no authority of its own: see [Signing](#signing) and [State](#state) |
| Ballista's upgrade authority | What every template on the pre-release devnet build does | Nothing yet. Releases will have none: see [Deployments](#deployments) |

Treat the caller and the builder as hostile unless the template binds them: a value from an input,
or an account nothing pins, is theirs to choose.

## Finalization checks

Finalization is the one-time check that locks a template on chain, at the end of its upload. The
Ballista program finalizes a template only if:

- **The bytes are sound.** The upload is complete and matches the hash recorded when it began, and
  the bytes are well formed: a known format and version, no unknown instructions, and no reserved
  bits or fields set.
- **Every value is typed.** Each value is set before it is read and has the type each instruction
  expects. A call's return data is read only straight after that call, and only if the call
  always runs.
- **Every account is declared.** Each account reference points at a declared account, and row
  accounts and row inputs appear only inside a row loop.
- **Calls stay within the declarations.** No call passes a declared account as a signer or as
  writable unless its declaration requires that privilege (see [Privileges](#privileges)), and
  every program the template calls, or derives a [PDA](/reference/glossary#pda) with, is declared
  `executable`.
- **Reads stay in bounds.** Each fixed-offset read stays within the account's declared minimum
  length, and introspection reads only the Instructions sysvar, pinned to its address.
- **The work is bounded.** Loops are never nested, and each has a fixed maximum, so even the worst
  case stays within the [limits](/reference/limits) on accounts, calls, call data, seeds and
  output.
- **Registries and output follow their rules.** A
  [registry entry](/reference/language#registries) is opened at the top level before it is used,
  read and written only through its declared fields, and never passed writable to a call. Return
  data is set once, after the last call, and every `emit` starts with its tag
  ([output rules](/reference/language#output)).

Finalization does not check that a called program's address is pinned (only the TypeScript
compiler requires that; see [Pins](#pins)), what the called programs do, or who may run the
template.

Each run then checks every account against its declaration (signer, writable, executable,
address, owner and minimum length), decodes the inputs exactly, and checks the account and row
counts. Arithmetic and casts are checked, a failed `require` stops the run, and the run never
writes to the template.

## Privileges

A declaration is a ceiling. A call can pass a declared account as a signer or as writable only if
its declaration requires that privilege and the transaction granted it. The ceiling has two gaps:

- **It bounds accounts, not calls.** A declared signer can be passed as a signer to every call in
  the template, with whatever data each call builds. A call whose data comes from an input can be
  any instruction the called program accepts from that signer.
- **Group members have no declaration.** A call passes each
  [account group](/guide/account-groups) member as writable whenever the transaction marked it
  writable, though never as a signer.

A template has no authority of its own: every call it makes, its signers could have made directly.
What the template adds is that its steps and checks run together, in one transaction, exactly as
written.

## Signing

Ballista signs with the seeds of its own PDAs in two places only:

- **At upload**, to create the template's account at its address.
- **When a run first opens a registry entry**, to create the entry's account. A payer the template
  names pays its rent.

A template's own calls never carry seeds, so Ballista never signs them, and no template can sign
as a PDA. [`assertPda`](/guide/pda-assertions) checks how an address was derived; it does not let
Ballista sign for it.

## State

Ballista keeps run-to-run state only in the [registry entries](/guide/registries) a template
declares. Each entry is an account Ballista owns, at an address derived from the template, the
registry and a key the template computes, and only that template's runs can change it. A run
checks each entry against the template and the key before using it. Entries are never closed, and
anyone can read them.

Templates and registry entries are the only accounts Ballista owns. Each holds its rent plus
anything someone sends it, and nothing withdraws those lamports from a finalized template or an
entry.

## Aliased accounts

A run accepts the same account in two slots and checks each slot against its own declaration. So
one account can fill two roles: one signer can count as two approvers, and one deposit can satisfy
two "received at least" checks. Where a template's checks assume two accounts differ, require it
with `notEqual(accountKey(a), accountKey(b))`, as
[Exact lamport delta](/guide/assertions#exact-lamport-delta) does. Registry entries are the
exception: opening an entry that is already open in the run fails with `InvalidRegistryEntry`.

## Pins

To pin a fact about an account is to fix it in the template, so that a run fails if the caller
passes an account that does not match. An account declaration can pin two facts:

- **`address`** fixes the account to one exact address. Pin every program a template calls, or
  derives a PDA with, this way. Otherwise the caller could substitute any program, and the
  template's calls would mean nothing.
- **`owner`** fixes the program that owns the account, and so which program wrote its data.

An owner pin is not a type pin: it accepts every account that program owns. A 355-byte Token
multisig passes an owner pin to the Token Program and a `minDataLength` of 165, and its bytes 64 to
72, where a token account keeps its balance, fall inside its list of signer addresses. An Anchor
program tells its account types apart only by their first eight bytes, the discriminator. Before a
template relies on an account's data, it should also check:

- **the type**: the discriminator, or the exact data length, such as 165 bytes for a token account;
- **the identity**: the address, a derivation ([`assertPda`](/guide/pda-assertions)), or fields
  that tie the account to the run, such as a token account's mint and owner.

Only the TypeScript compiler requires pins, unless you opt out as described below. It refuses a
template that calls a program without a pinned address, or that reads the data of an account that
pins neither its owner nor its address; neither rule checks a type. The Ballista program checks no
pins, so a template built with the Rust `ProgramBuilder`, or by hand, can leave a program unpinned
and still be finalized. Check the pins of any template you did not compile yourself.

## Opting out

`unsafeUnpinned: true` on an account declaration turns off both compiler requirements for that
account, so the template accepts whatever the caller passes. That suits a template only its caller
relies on, such as a wallet's own maintenance template, and not one that someone else relies on as
a safeguard: the caller could point a call at any program. The long name is meant to stand out in
code review. The flag exists only in the TypeScript template document and is not stored on chain.

## Deployments

Each release of the Ballista program will be deployed under its own address with no upgrade
authority, so that nobody can change what a stored template means. No release exists yet. The
pre-release build on devnet still has an upgrade authority: whoever holds it can change what every
template on that build does, so while you use it, you trust that key. It also predates this
repository's template format and rejects its templates. See
[Audit status](/guide/security#audit-status) and [Devnet workflow](/guide/devnet).

A template's address is derived from the program that finalized it, so each template belongs to
that one deployment. A new deployment is a different program, and templates must be uploaded again
to use it. For this reason the TypeScript SDK takes the program address as a parameter, and the
Rust SDK's `_for_program` functions, such as `find_template_pda_for_program`, take any deployment
other than `ballista_sdk::ID`.
