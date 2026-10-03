# Accounts and CPIs

This page explains how a template declares the accounts it uses and how it calls other programs
through CPIs (cross-program invocations: one program calling another).

Each account a template uses is declared in its `accounts` section, with the requirements the
caller's account must meet. It can be required to be a signer (it signed the transaction), writable
(the transaction allows it to change), or executable (it is a program). A declaration can also fix
the account's exact `address`, the program that owns it (`owner`), and a minimum data length
(`minDataLength`). A run rejects any account that does not meet its declaration.

The declaration is also a ceiling. A CPI in the template may pass an account as a signer or as
writable only if the account's declaration requires that privilege. Finalization, the one-time
check that locks the template on chain, enforces this, so the declarations tell you the most a run
can ask of each declared account. They don't say which calls get it: a declared signer can be
passed as a signer to every CPI in the template. And [account group](#account-groups) members have
no declaration to bound them.

Ballista passes on only the signatures the transaction already carries and never signs a
template's calls, so a template cannot create authority the transaction did not already have. See
[Privileges](/guide/trust-model#privileges) and [Signing](/guide/trust-model#signing).

## Protocol helper

This template moves tokens between two token accounts. Its `accounts` section shows each kind of
requirement: a pinned program, a signer, and two writable token accounts pinned by owner and size.

::: code-group

<<< @/../clients/js/examples/docs/token-transfer.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/token-transfer.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#token-transfer [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#token-transfer [Rust · Run]

:::

`tokenTransfer` is a shortcut, not a special instruction in the Ballista program. It compiles to an
ordinary CPI: the Token Program's accounts, the byte that selects its Transfer instruction, and the
amount encoded as a `u64`. The Rust tab builds exactly that CPI, and produces the same bytes.

## Generic CPI

`step.invoke` builds any CPI from parts: the program to call, the accounts with the privileges to
pass, and the instruction data as a list of pieces, either literal bytes or encoded values. The
optional `when` condition is covered [below](#conditional-invocation).

::: code-group

<<< @/../clients/js/examples/docs/generic-cpi.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/generic-cpi.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#generic-cpi [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#generic-cpi [Rust · Run]

:::

`MY_PROGRAM` and `MY_DISCRIMINATOR` are placeholders for your program's address and instruction.
A CPI's data can be at most 4,096 bytes. Finalization works out the largest size each CPI's data
can reach, here 8 + 8 + 128 bytes, and rejects a template that could exceed the limit. The Rust
builder cannot see a `bytes` input's maximum length from the segment, so the Rust tab states it with
`set_cpi_max_data_len`.

## Account groups

A CPI's account list is fixed when the template is written. Some programs need accounts the author
cannot know in advance, such as the pools along a swap route. For these, a template declares an
account group: the caller supplies its members at run time, and the CPI passes them after its
declared accounts. Group members have no requirements and cannot be read by the template. A CPI
passes each one as writable whenever the transaction marked it writable, and never as a signer.
[Account groups](./account-groups) covers the rules and shows a template that chooses between
swaps at run time.

## Conditional invocation

`when` makes a single CPI optional. Its condition is evaluated when the run reaches that step. If
the condition is false, the call is skipped and the run continues with the next step. A template
that emits a [run event](/guide/errors-and-events#run-events) records which calls actually ran.

Compare `step.require`, which fails the whole transaction when its condition is false. Use `when`
for work that is sometimes unnecessary, such as creating an account that may already exist. Use
`step.require` for a condition whose failure means something is wrong.

`expression.returnData` reads the data a called program returns, but not from a call with a `when`
condition, because a skipped call returns nothing.
