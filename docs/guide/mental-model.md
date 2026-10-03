# How it works

A template is three things: the **inputs** a caller supplies, the **accounts** it works with, and
the **steps** it runs. You write it once and upload it. After that, anyone can run it with new
inputs and accounts, and every run applies the same checks.

## A template, in one screen

This one pays up to eight people from a treasury, each a different amount, and refuses to go below
a reserve.

```ts
const template = defineTemplate({
  // Inputs: values the caller supplies with each run.
  inputs: { reserve: { type: 'u64' } },

  // Accounts: what each account must be. A run fails if one does not match.
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },

  // Batch: a row of accounts (and inputs) repeated once per recipient.
  batch: {
    maxIterations: 8,
    row: { recipient: { writable: true } },
    rowInputs: { amount: { type: 'u64' } },
  },

  // Steps: run in order, top to bottom.
  steps: [
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('recipient'),
        lamports: expression.rowInput('amount'),
      }),
    ]),
    step.require(
      expression.greaterThanOrEqual(
        expression.accountField(account.fixed('treasury'), 'lamports'),
        expression.input('reserve'),
      ),
      'keepsReserve',
    ),
  ],
});
```

## Inputs

Named, typed values the caller sends with each run: `bool`, `u64`, `i64`, `u128`, `pubkey` or
`bytes`. **Row inputs** are sent once per batch row, so each recipient can get a different amount.

## Accounts

Each declared account states what it must be: a signer, writable, a program (`executable`), a fixed
`address`, a fixed `owner`, a minimum data length. The caller supplies the real accounts, and the
run fails if one does not match.

Steps name accounts in two ways:

| Reference | Means |
| --- | --- |
| `account.fixed('treasury')` | An account from `accounts`, the same for the whole run |
| `account.iteration('recipient')` | The current row's account, inside `forEach` only |

Some templates also take an [account group](/guide/account-groups): a list of accounts, sized by
the caller, passed along to one call without being read.

## Steps

Steps run in order, top to bottom:

- `step.let` computes a value and names it, and `step.require` fails the whole run unless a
  condition holds.
- `step.invoke` calls another program, and a `when` condition on it skips just that call. Helpers
  such as `systemTransfer` build one for you.
- `step.forEach` runs its steps once per batch row, and `step.repeat` a counted number of times. A
  count above the loop's `max` fails the run with `LoopCountExceeded`; it isn't cut down to `max`.
- Other steps log events, set return data, and write [registry](/guide/registries) fields.

The [template language](/reference/language#steps) lists every step and its rules.

## Expressions

Steps compute with expressions. An expression can read:

- an input (`expression.input`, `expression.rowInput`);
- an account's address (`expression.accountKey`), owner, lamports or data length
  (`expression.accountField`);
- a field of a [registry entry](/guide/registries) (`expression.registry`);
- a number or address at a byte offset in an account's data (`expression.accountData`);
- the clock, a derived PDA, or data returned by the last call;
- the other instructions in the transaction, through the Instructions sysvar
  (`expression.instructionCount`, `expression.instructionData` and others);
- a byte range of a read-only account, without copying it.

It can combine them with checked arithmetic (`add`, `subtract`, `multiply`, `divide`, `remainder`,
`min`, `max`), exact `multiplyDivide` that rounds down or up, `powerOfTen`, shifts and bitwise
operations, comparisons, `and`/`or`/`not`, and `select`. Overflow fails the run instead of wrapping.
[Inputs and expressions](/guide/expressions) covers the math, including
[prices and decimals](/guide/expressions#prices-and-decimals). The
[language reference](/reference/language#introspection) covers reading other instructions and byte
ranges, and lists every source.

## Upload: checked, then locked

When you upload a template, the Ballista program checks all of it once;
[Trust model](/guide/trust-model#finalization-checks) lists what it checks. A template that passes
is **finalized**: locked for good. It cannot be changed or closed, and a new version gets a new
template ID. See [Template lifecycle](/guide/template-lifecycle).

## Run: all or nothing

A run checks the caller's accounts and inputs against the declarations, then works through the
steps in order. If any check or call fails, the whole Solana transaction is undone, including calls
that had already succeeded.

A template has no authority of its own: see [when Ballista signs](/guide/trust-model#signing). It
keeps nothing between runs except in the [registry entries](/guide/registries) it declares.

For how a template is stored as bytes, see [Wire format](/reference/wire-format).
