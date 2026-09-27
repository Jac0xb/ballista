# Withdraw everything, with a minimum

<p class="protocol-line">marginfi</p>

**Status:** Compiles and passes the verifier; not yet run against marginfi.

## What it does

Empties a marginfi position, reverts unless enough came out, and sends the proceeds on to a
treasury account.

marginfi's `lending_account_withdraw(amount, withdraw_all)` can empty a position, but it doesn't
tell the caller how much came out, so nothing later in the transaction can depend on it. A
withdrawal that returns far less than expected still succeeds: the bank may have been drained, its
utilization (the share of deposits lent out) may have capped the withdrawal, or the position may
have been liquidated. Whatever comes next then runs on a wrong assumption.

The template records the destination token balance, withdraws everything, and measures how much
arrived. If that is less than `minimumWithdrawn`, the run reverts. Otherwise it transfers the
withdrawn amount to the treasury token account.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/marginfi-withdraw-all-with-floor.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#marginfi-withdraw [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/marginfi-withdraw.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#marginfi-withdraw [Rust · Run]

:::

The last two bytes of the withdrawal's instruction data are `Option::Some(true)` for
`withdraw_all`. In that mode marginfi ignores `amount`, but the field still has to be there,
because Borsh (the binary format Anchor programs use for instruction arguments) reads every field.

## Run it

The Run tabs pass the template's 10 accounts in the order it declares them, and one input,
`minimumWithdrawn`. `authority` signs but is not writable. There is no account group.

## What has been tested

- The template compiles and passes Ballista's verifier.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls marginfi.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
