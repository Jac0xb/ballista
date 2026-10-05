# Trust model

Who controls what when a template runs, what Ballista checks, and where those checks stop. A
template calls other programs with accounts the caller supplies, so its checks protect you only if
you know which parts the caller controls.

## Who controls what

Several parties can shape what a run does. Here is what each one decides, and what keeps them in
check.

- **The template's author** writes its accounts, inputs and steps. Once the template is
  [finalized](#finalization-checks) it can never change, so the author has no further say.
- **Whoever runs it** chooses every input and every account, and supplies the signatures. The
  template's rules are the only limit: an input it doesn't check, or an account it doesn't fix by
  address or owner, is the caller's to choose. Treat the caller as hostile unless the template
  rules something out.
- **Whoever builds the transaction**, if that isn't the signer (a wallet, an app or a bot), makes
  the same choices, and can add other instructions before or after the run. The signer's review
  before signing is the only check on those. A template can inspect the other instructions only
  through [introspection](/reference/language#introspection).
- **The programs a template calls** decide what each call does. Their own checks apply, and the
  template can check the result after the call returns.
- **Whoever can upgrade those programs** can change what they do later, and nothing in the template
  can stop that. Call programs whose upgrade authority you trust, or that have none.
- **Ballista** enforces the template and keeps its registry entries, but holds no authority of its
  own; see [Signing](#signing) and [State](#state).
- **Ballista's upgrade authority** can change the pre-release devnet build, and every template on
  it. Releases will have no upgrade authority; see [Deployments](#deployments).

## Template finalization checks {#finalization-checks}

Finalization is the one-time check, at the end of an upload, that makes a template immutable. Every
template passes through these gates first:

<FinalizationGates />

Finalization does not check that a called program's address is pinned (only the SDK compilers
require that; see [Pins](#pins)), what the called programs do, or who may run the
template.

Each run then checks every account against its declaration (signer, writable, executable,
address, owner and minimum length), decodes the inputs exactly, and checks the account and row
counts. Arithmetic and casts are checked, a failed `require` stops the run, and the run never
writes to the template.

## Privileges

A call can pass an account as a signer or writable only if its declaration asks for that and the
transaction granted it. Three gaps remain:

- **It bounds accounts, not calls.** A declared signer can sign for every call in the template,
  including a call whose data comes from an input.
- **It bounds slots, not addresses.** The caller fills every slot, so one address can sit in a
  read-only slot, even a pinned one, and in a writable slot that a call passes writable. Where it
  matters, require the two keys to differ, as in [Aliased accounts](#aliased-accounts).
- **Group members have no declaration.** [Account group](/guide/accounts-and-cpis#account-groups)
  members are passed writable whenever the transaction marked them so (never as signers).

A template has no authority of its own: every call it makes, its signers could have made directly.
It only guarantees that its steps and checks run together, in one transaction, exactly as written.

## Signing

Ballista signs only to create its own accounts: a template's account at upload, and a registry
entry the first time a run opens it (the template's named payer pays the rent). It never signs a
template's calls, so no template can sign as a PDA; [`assertPda`](/guide/pda-assertions) only
checks how an address was derived.

## State

Ballista keeps run-to-run state only in the [registry entries](/guide/registries) a template
declares. Each entry is an account Ballista owns, at an address derived from the template, the
registry and a key the template computes, and only that template's runs can change it. A run
checks each entry against the template and the key before using it. Entries are never closed, and
anyone can read them.

Ballista creates only templates and registry entries. Each holds its rent plus anything someone
sends it, and nothing withdraws those lamports from a finalized template or an entry. Anyone can
also make an account Ballista-owned through the System program, but it holds only zeros: every
Ballista instruction refuses it, and nothing moves its lamports.

## Aliased accounts

A run accepts the same account in two slots and checks each slot against its own declaration. So
one account can fill two roles: one signer can count as two approvers, and one deposit can satisfy
two "received at least" checks. Where a template's checks assume two accounts differ, require it
with `notEqual(accountKey(a), accountKey(b))`, as
[Exact lamport delta](/guide/assertions#exact-lamport-delta) does. Registry entries are the
exception: opening an entry that is already open in the run fails with `InvalidRegistryEntry`
(6025).

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

Only the SDK compilers require pins, unless you opt out as described below. TypeScript
`compileTemplate` and Rust `Template::compile` both refuse a template that calls a program without a
pinned address, or that reads the data of an account that pins neither its owner nor its address;
neither rule checks a type. The Ballista program checks no pins, so a template built by hand or with
other tools can leave a program unpinned and still be finalized. Check the pins of any template you did not compile yourself.

## Opting out

`unsafeUnpinned: true` on an account declaration, or `.unsafe_unpinned()` in Rust, turns off both
compiler requirements for that account, so the template accepts whatever the caller passes. That
suits a template only its caller relies on, such as a wallet's own maintenance template, and not one
that someone else relies on as a safeguard: the caller could point a call at any program. The long
name is meant to stand out in code review. The flag exists only in the template you compile and is
not stored on chain.

## Deployments

Each release of the Ballista program will be deployed under its own address with no upgrade
authority, so that nobody can change what a stored template means. No release exists yet. The
pre-release build on devnet, built from this repository, still has an upgrade authority: whoever
holds it can change what every template on that build does, so while you use it, you trust that
key. See [Audit status](/guide/security#audit-status).

- **Devnet program:** [`BLSTAmUBA29tcRUvoq5DBYxRhGptrnWPtfQW65RszRWR`](https://explorer.solana.com/address/BLSTAmUBA29tcRUvoq5DBYxRhGptrnWPtfQW65RszRWR?cluster=devnet)
- **Its upgrade authority:** `A9TciQEkWp1uh8ee4DpPyXgi4twSfUuNFe9sGEvjxfsQ`

A template's address is derived from the program that finalized it, so each template belongs to
that one deployment. A new deployment is a different program, and templates must be uploaded again
to use it. For this reason the TypeScript SDK takes the program address as a parameter, and the
Rust SDK's `_for_program` functions, such as `find_template_pda_for_program`, take any deployment
other than `ballista_sdk::ID`.
