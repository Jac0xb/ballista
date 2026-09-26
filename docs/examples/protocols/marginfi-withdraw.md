# Empty a position, then insist it was worth it

`lending_account_withdraw(amount, withdraw_all)` will empty a position. It does not tell the
caller how much that was, and the caller cannot condition on it.

A withdrawal that returns far less than expected — the bank was drained, utilization capped it,
the position had been liquidated — still succeeds. Whatever was sequenced after it then proceeds
on a false assumption.

::: code-group

<<< ../../../clients/js/examples/protocols/marginfi-withdraw-all-with-floor.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The two trailing bytes are `Option::Some(true)` for `withdraw_all`. The `amount` before them is
ignored in that mode, but Borsh still reads it, so it still has to be there.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
