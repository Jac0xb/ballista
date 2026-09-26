# Accounts and CPIs

An account schema is a capability declaration. It states the maximum privilege a template may ever
use for that slot, and `Run` rejects any account that does not satisfy it. A CPI inside the
template can request that privilege or less, never more.

This is what makes a finalized template safe to hand to a caller you do not control. The privileges
are fixed when the template is authored and checked again at finalization, so reading the schema
tells you the upper bound on what any run can do. Ballista forwards signer status from the outer
transaction and never signs as its own PDA, so a template cannot manufacture authority that the
transaction did not already carry.

```ts
accounts: {
  tokenProgram: {
    executable: true,
    address: TOKEN_PROGRAM_BYTES,
  },
  authority: { signer: true },
  source: {
    writable: true,
    owner: TOKEN_PROGRAM_BYTES,
    minDataLength: 165,
  },
  destination: {
    writable: true,
    owner: TOKEN_PROGRAM_BYTES,
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

`tokenTransfer` is not a runtime opcode. It compiles to generic CPI accounts plus the Token
Program's literal transfer discriminator and an encoded `u64` register.

## Generic CPI

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

The generated CPI data is capped at 4,096 bytes, and its maximum length is proven during template
finalization.

## Account groups

A CPI's declared account list is fixed when the template is authored. When a callee needs accounts
the author cannot know in advance, such as the pools on a swap route, the template declares an
account group and the invocation forwards it after its declared accounts. Members are supplied by
the caller, carry no constraints, cannot be read, and never sign. See
[Account groups](./account-groups) for the rules and a template that chooses between swaps at run
time.

## Conditional invocation

`when` makes a single CPI optional. Its boolean expression is evaluated at that point in the run.
If the result is false the invocation is skipped and execution continues with the next step; the
run event records which invocations actually fired.

This is the counterpart to `step.require`, which aborts the whole transaction when its condition
fails. Use `when` for work that is legitimately unnecessary, such as creating an account that may
already exist, and `step.require` for a condition whose failure means something is wrong. A guarded
invocation cannot be the source of a `returnData` read, because the value may never be produced.
