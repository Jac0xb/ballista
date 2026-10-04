# Ballista

Ballista is a transaction engine for Solana that makes complex, conditional onchain actions
reusable. Instead of deploying a custom helper program, you store a multi-step transaction as an
immutable, shareable template.

As it runs, a template reads live state, does checked math, loops over accounts, calls other
programs, verifies their results, and remembers state between runs, all or nothing. Ballista is a
public good: MIT licensed, with no protocol fee.

[Documentation](https://ballista.sh/) · [Getting started](https://ballista.sh/guide/getting-started) · [How it works](https://ballista.sh/guide/mental-model) · [Examples](https://ballista.sh/examples/)

> [!WARNING]
> **Not audited.** Ballista is on mainnet, but no third party has audited the program or the SDKs.
> Use it at your own risk. [Details](https://ballista.sh/guide/security#audit-status)

## Why it exists

A Solana transaction fixes its instruction data when it is signed, so it cannot move whatever
balance is there, repay exactly what is owed, or make one payment depend on the one before it. The
usual answer is a custom program, even for a few lines of logic. With Ballista you upload the
sequence once, the program checks and locks it, and anyone can run it with new inputs.

## What a template can do

- [Amounts read at run time](https://ballista.sh/guide/runtime-values): read a balance, then spend it.
- [Conditional calls](https://ballista.sh/guide/conditional): skip a call instead of failing the transaction.
- [Loops over rows and counts](https://ballista.sh/guide/loops): pay creditors in order, or collect only from funded accounts.
- [Remember state between runs](https://ballista.sh/guide/registries): a daily limit per caller, or an allowlist.
- [Safety guardrails](https://ballista.sh/guide/guardrails): check a result after a call returns, such as a swap's minimum output, and undo everything if it fails.

The [examples](https://ballista.sh/examples/) cover payments, token accounts and composition, plus
[twelve protocol templates](https://ballista.sh/examples/protocols/) for Jupiter, Kamino, Orca,
pump.fun, Pyth and signed quotes, tested against snapshots of the mainnet programs.

## Example

This template pays creditors in order from what a treasury holds above a reserve, read as it runs.

```ts
export const waterfallUntilTheMoneyRunsOut = defineTemplate({
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 8,
    minIterations: 1,
    row: { creditor: { writable: true } },
    rowInputs: { owed: { type: 'u64' } },
  },
  steps: [
    step.let(
      'remaining',
      expression.subtract(expression.accountField(account.fixed('treasury'), 'lamports'), expression.input('reserve')),
    ),
    step.forEach(
      [
        step.let('pay', expression.min(expression.variable('remaining'), expression.rowInput('owed'))),
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('treasury'),
          to: account.iteration('creditor'),
          lamports: expression.variable('pay'),
          when: expression.greaterThan(expression.variable('pay'), expression.u64(0)),
        }),
        step.assign('remaining', expression.subtract(expression.variable('remaining'), expression.variable('pay'))),
      ],
      { carry: ['remaining'] },
    ),
  ],
});
```

`carry` passes `remaining` from row to row, so each payment depends on the ones before it, and
`when` skips the transfer once the money runs out. The
[Loops guide](https://ballista.sh/guide/loops#waterfall-until-the-money-runs-out) has the imports,
the Rust version, and the code that runs it.

## When to write your own program instead

A template has no authority of its own and every loop has a fixed maximum, so write a program for
custody or PDA signing, protocol-owned state, routes past Solana's
[account and CPI limits](https://ballista.sh/reference/limits), compute-critical or unbounded work,
or automation with no signer. [Details](https://ballista.sh/guide/why-ballista#boundary)

## Try it

Neither the program nor the SDKs are published yet, so you build them from this repository. You
need the [Solana CLI](https://solana.com/docs/intro/installation), Rust, and for TypeScript,
Node.js 22 or later and pnpm.

```bash
git clone https://github.com/Jac0xb/ballista.git && cd ballista
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
solana-test-validator --reset \
  --bpf-program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD target/deploy/ballista.so
```

Then, in a second terminal, upload a template, run it, and decode a failed run:

```bash
pnpm install && pnpm build:sdk
pnpm --dir clients/js exec tsx examples/start/getting-started.ts           # TypeScript
cargo run --manifest-path clients/rust/examples/getting-started/Cargo.toml  # or Rust
```

[Getting started](https://ballista.sh/guide/getting-started) walks through the same five steps and
sets up a project of your own.

## Contributing

Run these from the repository root after `pnpm install`.

| Command | What it does |
| --- | --- |
| `pnpm test` | Rust core and property tests, then the TypeScript SDK tests |
| `pnpm check` | Rust check, SDK typecheck and tests, and a docs build |
| `pnpm build:program` | Build the onchain program |
| `pnpm build:sdk` | Build the TypeScript SDK |
| `pnpm test:integration` | Run the built program's integration tests (`build:program` first) |
| `pnpm fixtures` | Regenerate the shared compiler fixtures after a compiler change |
| `pnpm docs:dev` | Serve the docs from `docs/` locally |

## Repository layout

- `programs/ballista`: the onchain program, built with Pinocchio.
- `common`: the template format, parser and verifier, shared by the program and the Rust SDK.
- `clients/js`, `clients/rust`: the TypeScript (`@jac0xb/ballista`) and Rust (`ballista-sdk`) SDKs, with runnable examples.
- `tests/ballista`, `tests/protocols`: integration tests, and the protocol templates run against mainnet snapshots.
- `certora`, `kani`, `fuzz`, `fuzz-executor`: formal verification and fuzzing.
- `docs`: the source of [ballista.sh](https://ballista.sh/).

## License

[MIT](LICENSE)
