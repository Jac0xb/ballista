# Harvest only the positions that earned

<p class="protocol-line">Orca</p>

**Status:** Compiles and passes the verifier; not yet run against Orca.

## What it does

Collects fees from up to 12 Orca positions in one transaction, skipping the ones that have not
earned enough.

A liquidity provider may hold dozens of positions. Some have earned fees since the last harvest
and some haven't, and which is which depends on trades that happen after the transaction is
signed. Sending one `collect_fees` per position fails the whole transaction at the first position
Orca refuses. Filtering the list before signing doesn't solve it: a position that looked empty may
have earned by the time the transaction lands, and the reverse.

For each position, the template reads `fee_owed_a` from the position account during the run and
calls `collect_fees` only if it is above `dustFloor`.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/orca-harvest-many-positions.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#orca-harvest [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/orca-harvest.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#orca-harvest [Rust · Run]

:::

The fee offset comes from Orca's `Position` account; see
[reading offsets](/examples/protocols/#reading-offsets-from-an-account).

## Run it

The positions form a batch. The Run tabs pass the eight declared accounts in order, then one row
of two accounts per position (the position and its position token account), and one input,
`dustFloor`. The template repeats its steps once per row, and the row count comes from the account
list, so there is no count to pass. You choose which positions to include; the template decides
during the run which of them to collect from.

## What has been tested

- The template compiles and passes Ballista's verifier.
- An opt-in test checks the Orca offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Orca.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
