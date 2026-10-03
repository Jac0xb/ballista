# Liquidate with a minimum payout

<p class="protocol-line">Kamino</p>

**Status:** Run as real transactions against Kamino, copied from mainnet at one slot into LiteSVM,
a local Solana runtime. Not yet run on mainnet itself.

## What it does

Liquidates a Kamino loan and reverts unless you receive at least a set amount of collateral.

A Kamino loan lives in an obligation, the borrower's account of deposits and debts. Once the debt
passes a set share of the collateral's value, the obligation is unhealthy, and anyone may repay
part of the debt for collateral worth more. Kamino's own minimum,
`min_acceptable_received_liquidity_amount`, bounds its calculation, not what reaches you.

The template:

- requires the liquidator to own both accounts Kamino pays, `userDestinationLiquidity` and
  `userDestinationCollateral`, since Kamino checks only their mints (`bountyGoesToTheLiquidator`,
  `seizedCollateralGoesToTheLiquidator`);
- records the balance of `userDestinationLiquidity`, where Kamino pays the seized collateral once
  redeemed: SOL, for SOL collateral;
- liquidates with Kamino's `liquidate_obligation_and_redeem_reserve_collateral_v2`;
- requires that balance to have grown by at least `minimumBounty`, or the whole run reverts
  (`liquidationPaidTheBounty`).

`minimumBounty` is in the collateral's units, lamports for SOL, and is not netted against what you
repaid. Collateral Kamino could not redeem stays in `userDestinationCollateral` as cTokens
(Kamino's deposit receipts) and doesn't count.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-liquidate-with-proof.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#kamino-liquidate [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/kamino-liquidate.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#kamino-liquidate [Rust · Run]

:::

If the obligation is healthy again when the run lands, say because another liquidator got there
first, Kamino refuses and the run reverts. To skip instead, make both the liquidation and the
bounty check depend on two `u128` fields of the obligation, as the refresh leaves them: it can be
liquidated when `borrow_factor_adjusted_debt_value_sf` (byte 2208, counting the discriminator) is
at least `unhealthy_borrow_value_sf` (byte 2256). No template reads them yet, so this is untested.

## Run it

Kamino's v2 liquidation takes 25 accounts: 20 the template passes, then the `farmAccounts`
[account group](/guide/account-groups) of five. A reserve is Kamino's pool for one token, and a
farm is a Kamino Farms rewards pool attached to one. The group holds:

- the obligation's user state in the withdrawn reserve's collateral farm, and that farm;
- its user state in the repaid reserve's debt farm, and that farm;
- Kamino's Farms program.

For a farm a reserve doesn't have, both of its slots hold the Kamino program, read-only.

The Run tabs pass the 19 declared accounts in order (`kamino`, `tokenProgram`,
`instructionsSysvar`, `liquidator`, Kamino's twelve from `obligation` to
`withdrawReserveFeeReceiver`, then `userSourceLiquidity`, `userDestinationCollateral` and
`userDestinationLiquidity`), then the inputs `liquidityAmount`, `minAcceptableReceived` and
`minimumBounty`, then `farmAccounts`.

Before the run, **refresh Kamino in the same transaction:** `refresh_reserve` for each reserve the
obligation holds, then `refresh_obligation`. A refresh brings a reserve's interest and price, or an
obligation's values, up to date. Kamino liquidates only against reserves and an obligation
refreshed in the same slot, and the template doesn't refresh. `buildKaminoRefreshes` (TypeScript)
and `kamino_refreshes` (Rust), next to the runs, build them.

## What has been tested

- **Against the real programs.** `tests/protocols/tests/kamino_liquidate_with_proof.rs` liquidates
  an obligation that deposited 1 SOL and borrowed 70% of its value in USDC, after the test cut
  SOL's price 12%, behind Kamino's refreshes. The run repaid 10% of the debt, the market's limit
  per liquidation, and received more SOL than that USDC was worth at the oracle price, with no
  cTokens left over. The whole transaction took 185,402 compute units (Solana's measure of
  execution cost) and 1,045 bytes.
- **Failures.** A `minimumBounty` one lamport above the payout fails at `liquidationPaidTheBounty`,
  with nothing moved. An attacker's account, approved for the liquidator, as
  `userDestinationLiquidity` or `userDestinationCollateral` fails at the matching owner check,
  before Kamino is called.
- **Not tested.** Mainnet itself, a repaid reserve with a debt farm, the health gate above, and
  Token-2022 tokens: the template accepts SPL Token accounts only. The inputs are the run
  builder's choice.
- A test reads the template and checks that the liquidation is v2 with `farmAccounts` as its group,
  and that the bounty is measured on `userDestinationLiquidity`, not the cToken account
  (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares, right after Kamino's refreshes
  (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
