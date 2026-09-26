# Sell a whole balance

The smallest shape here and the one that turns up everywhere: a fee account, an airdrop claim, a
vesting withdrawal, the dust a route left behind. The balance is still moving when the
transaction is signed.

A route built for a fixed input fails when the balance came up short and strands the difference
when it came up long. Here the amount is read out of the account, with a floor below which the
run does not bother and a check afterwards that nothing meaningful was left behind.

::: code-group

<<< ../../../clients/js/examples/protocols/token-sweep-into-swap.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

Under Token-2022's transfer-fee extension the `amount` at offset 64 includes the withheld
portion, which is not spendable. Subtract the `TransferFeeAmount` extension for a fee-bearing
mint; its position depends on TLV ordering, so read it from the account rather than assuming.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
