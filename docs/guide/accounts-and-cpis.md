# Accounts and CPIs

Account schemas constrain the addresses and privileges accepted by `Run`. CPI descriptors can use
only a subset of those privileges.

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
finalization. Ballista forwards signer status; it never signs for its own PDAs.
