# A tip you only pay when you earned it

Jito's advice is to keep the tip in the same transaction as the strategy, so that a failed
strategy pays no tip. That covers failure. It does not cover the more common case: an arbitrage
that succeeds but comes out thinner than the bid. A transaction cannot compare its own profit
against its own tip, so it pays in full either way.

This one snapshots the searcher's lamports, runs the strategy, and refuses to continue unless the
realized profit covers the tip plus a margin. A run that misses the bar reverts before the
transfer.

::: code-group

<<< ../../../clients/js/examples/protocols/jito-profit-guarded-tip.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The tip is a run input, fixed before signing exactly like an ordinary tip. Ballista could compute
it as a share of profit instead — a tip is a plain SOL transfer and Jito accepts one made by CPI
— but the block engine is closed source and public accounts disagree on whether a
runtime-computed amount is scored at its simulated value in the auction or read from the
instruction. A tip that pays but ranks as zero is worse than no tip, so this bids a fixed amount
and uses the template for the part that is unambiguous.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
