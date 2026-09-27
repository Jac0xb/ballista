# Withdraw everything, with a minimum

This template empties a marginfi position, reverts unless enough came out, and sends the proceeds
on to a treasury account.

marginfi's `lending_account_withdraw(amount, withdraw_all)` can empty a position, but it doesn't
tell the caller how much came out, so nothing later in the transaction can depend on it. A
withdrawal that returns far less than expected still succeeds: the bank may have been drained, its
utilization (the share of deposits lent out) may have capped the withdrawal, or the position may
have been liquidated. Whatever comes next then runs on a wrong assumption.

The template records the destination token balance, withdraws everything, and measures how much
arrived. If that is less than `minimumWithdrawn`, the run reverts. Otherwise it transfers the
withdrawn amount to the treasury token account.

::: code-group

<<< ../../../clients/js/examples/protocols/marginfi-withdraw-all-with-floor.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The last two bytes of the withdrawal's instruction data are `Option::Some(true)` for
`withdraw_all`. In that mode marginfi ignores `amount`, but the field still has to be there,
because Borsh (the binary format Anchor programs use for instruction arguments) reads every field.

The Rust tab builds the run for [act only on a fresh price](/examples/protocols/pyth-gate). Build
this template's run the same way, with its own accounts and inputs in the order the template
declares them. This template has no account group, so leave out the `.groups(...)` call and the
extra accounts at the end.

Not yet run against marginfi: the template compiles and passes Ballista's verifier, but no test
calls marginfi.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
