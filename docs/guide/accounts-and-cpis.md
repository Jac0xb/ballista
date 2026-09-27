# Accounts and CPIs

This page explains how a template declares the accounts it uses and how it calls other programs
through CPIs (cross-program invocations: one program calling another).

Each account a template uses is declared in its `accounts` section, with the requirements the
caller's account must meet. It can be required to be a signer (it signed the transaction), writable
(the transaction allows it to change), or executable (it is a program). A declaration can also fix
the account's exact `address`, the program that owns it (`owner`), and a minimum data length
(`minDataLength`). A run rejects any account that does not meet its declaration.

The declaration is also a ceiling. A CPI in the template may pass an account as a signer or as
writable only if the account's declaration requires that privilege.

That ceiling is what makes it safe to publish a template for callers you do not control. The
privileges are fixed when the template is written and checked again at finalization, the one-time
check that locks the template on chain, so the declarations tell you the most any run can do.
Ballista passes on only the signatures the outer transaction already carries, and it never signs as
a PDA (program-derived address: an address a program controls, with no private key) of its own, so
a template cannot create authority the transaction did not already have.

```ts
accounts: {
  tokenProgram: {
    executable: true,
    address: TOKEN_PROGRAM_ADDRESS_BYTES,
  },
  authority: { signer: true },
  source: {
    writable: true,
    owner: TOKEN_PROGRAM_ADDRESS_BYTES,
    minDataLength: 165,
  },
  destination: {
    writable: true,
    owner: TOKEN_PROGRAM_ADDRESS_BYTES,
    minDataLength: 165,
  },
}
```

## Protocol helper

```ts
tokenTransfer({
  tokenProgram: account.fixed('tokenProgram'),
  source: account.fixed('source'),
  destination: account.fixed('destination'),
  authority: account.fixed('authority'),
  amount: expression.input('amount'),
});
```

`tokenTransfer` is a shortcut, not a special instruction in the Ballista program. It compiles to an
ordinary CPI: the Token Program's accounts, the byte that selects its Transfer instruction, and the
amount encoded as a `u64`.

## Generic CPI

`step.invoke` builds any CPI from parts: the program to call, the accounts with the privileges to
pass, and the instruction data as a list of pieces, either literal bytes or encoded values. The
optional `when` condition is covered [below](#conditional-invocation).

::: code-group

```ts [TypeScript · template]
step.invoke({
  program: account.fixed('program'),
  accounts: [
    { account: account.fixed('vault'), writable: true, signer: false },
    { account: account.fixed('authority'), writable: false, signer: true },
  ],
  data: [
    data.literal(MY_DISCRIMINATOR),
    data.encode('u64', expression.input('amount')),
    data.encode('bytes', expression.input('clientPayload')),
  ],
  when: expression.input('enabled'),
});
```

```rust [Rust · call]
let run = ballista_sdk::run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(program, false),
        AccountMeta::new(vault, false),
        AccountMeta::new_readonly(authority, true),
    ],
    &encoded_inputs,
);
```

:::

The Rust tab shows the caller's side: the run instruction passes the accounts in the order the
template declares them. A CPI's data can be at most 4,096 bytes. Finalization works out the largest
size each CPI's data can reach and rejects a template that could exceed the limit.

## Account groups

A CPI's account list is fixed when the template is written. Some programs need accounts the author
cannot know in advance, such as the pools along a swap route. For these, a template declares an
account group: the caller supplies its members at run time, and the CPI passes them after its
declared accounts. Group members have no requirements, cannot be read by the template, and are never
passed as signers. [Account groups](./account-groups) covers the rules and shows a template that
chooses between swaps at run time.

## Conditional invocation

`when` makes a single CPI optional. Its condition is evaluated when the run reaches that step. If
the condition is false, the call is skipped and the run continues with the next step. A template
that emits a [run event](/guide/errors-and-events#run-events) records which calls actually ran.

Compare `step.require`, which fails the whole transaction when its condition is false. Use `when`
for work that is sometimes unnecessary, such as creating an account that may already exist. Use
`step.require` for a condition whose failure means something is wrong.

`expression.returnData` reads the data a called program returns, but not from a call with a `when`
condition, because a skipped call returns nothing.
