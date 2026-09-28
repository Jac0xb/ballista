# Compound the fees you collected

<p class="protocol-line">Orca</p>

**Status:** Compiles and passes the verifier; not yet run against Orca.

## What it does

Collects the fees an Orca Whirlpool position has earned and adds them back to the position as
liquidity.

Orca's `increase_liquidity` takes limits, `token_max_a` and `token_max_b`, not amounts. Set them
too low and the call fails. Set them too high and it takes the extra from your wallet. The right
limits are the fees the position has earned when the transaction runs.

The template reads `fee_owed_a` and `fee_owed_b` from the position account before collecting,
because collecting resets them to zero, and uses them as the limits. You choose
`liquidityAmount`, the liquidity to add, when you build the run; the limits cap what it can cost.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/orca-compound-fees.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#orca-compound [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/orca-compound.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#orca-compound [Rust · Run]

:::

Both calls have a `when` condition: if token A's fees are not above `dustFloor`, both calls are
skipped and the run succeeds without doing anything. A scheduled compounder that finds nothing to
compound then finishes cleanly instead of failing. The condition looks only at token A's fees.

The fee offsets come from Orca's `Position` account; see
[reading offsets](/examples/protocols/#reading-offsets-from-an-account).

## Run it

The Run tabs pass the template's 12 accounts in the order it declares them, and two inputs:
`liquidityAmount` (a `u128`) and `dustFloor`. There is no account group.

## What has been tested

- The template compiles and passes Ballista's verifier.
- An opt-in test checks the Orca offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Orca.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
