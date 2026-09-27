# Compound the fees you collected

This template collects the fees an Orca Whirlpool position has earned and adds them back to the
position as liquidity.

Orca's `increase_liquidity` takes limits, `token_max_a` and `token_max_b`, not amounts. Set them
too low and the call fails. Set them too high and it takes the extra from your wallet. The right
limits are the fees the position has earned when the transaction runs.

The template reads `fee_owed_a` and `fee_owed_b` from the position account before collecting,
because collecting resets them to zero, and uses them as the limits. You choose
`liquidityAmount`, the liquidity to add, when you build the run; the limits cap what it can cost.

::: code-group

<<< ../../../clients/js/examples/protocols/orca-compound-fees.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

Both calls have a `when` condition: if token A's fees are not above `dustFloor`, both calls are
skipped and the run succeeds without doing anything. A scheduled compounder that finds nothing to
compound then finishes cleanly instead of failing. The condition looks only at token A's fees.

The fee offsets come from Orca's `Position` account; see
[reading offsets](/examples/protocols/#reading-offsets-from-an-account).

The Rust tab builds the run for [act only on a fresh price](/examples/protocols/pyth-gate). Build
this template's run the same way, with its own accounts and inputs in the order the template
declares them. This template has no account group, so leave out the `.groups(...)` call and the
extra accounts at the end.

Not yet run against Orca: the template compiles and passes Ballista's verifier, and an opt-in test
checks its offsets against devnet accounts, but no test calls Orca.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
