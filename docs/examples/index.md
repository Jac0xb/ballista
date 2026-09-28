# Examples

Complete templates you can copy, grouped by what they do. Each one shows the template and the code
that runs it.

New to Ballista? [Getting started](/guide/getting-started) takes one template from definition to a
run instruction in TypeScript, then [builds the same template in Rust](/guide/getting-started#author-it-in-rust). To
see what a template can do that an ordinary transaction can't, read the guide pages on
[amounts read at run time](/guide/runtime-values), [conditional calls](/guide/conditional),
[loops](/guide/loops) and [guardrails](/guide/guardrails).

Every example outside the protocol section shows its template and the code that runs it in both
TypeScript and Rust. The **Plain transaction?** column below says whether you can get the same
result without Ballista:

- **Yes**: ordinary instructions in one transaction do the same thing. What the template adds is a
  single instruction and a sequence of calls that is stored on chain and was checked when it was
  uploaded.
- **Yes, weaker**: you can send the same instructions, but a check the template makes on chain is
  left to whoever builds the transaction.
- **No**: no sequence of instructions can do it, because a decision depends on account data read
  while the transaction runs.

## Protocol templates {#live-protocols}

Templates that work with Jupiter, Kamino, marginfi, Drift, Orca, Pyth and Jito. They use the real
program addresses and instruction names, but none has been run against the protocols themselves.
See [what has been tested](/examples/protocols/#what-has-been-tested).

| Recipe | Protocol | Decided during the run |
| --- | --- | --- |
| [Deposit swap output](/examples/protocols/jupiter-deposit) | Jupiter → Kamino | How much the swap produced |
| [Oracle-checked swap](/examples/protocols/jupiter-oracle-swap) | Jupiter + Pyth | Whether the swap paid at least the oracle price, less a tolerance |
| [Sell whole balance](/examples/protocols/token-sweep) | SPL Token → Jupiter | How much there is to sell |
| [Repay from swap](/examples/protocols/kamino-repay) | Jupiter → Kamino | How much the swap produced |
| [Liquidate](/examples/protocols/kamino-liquidate) | Kamino | How much collateral the liquidator received |
| [Withdraw](/examples/protocols/marginfi-withdraw) | marginfi | How much the withdrawal returned |
| [Rebalance](/examples/protocols/drift-rebalance) | marginfi → Drift | How much marginfi released |
| [Settle and withdraw](/examples/protocols/drift-settle) | Drift | Whether the withdrawal reached the wallet |
| [Compound fees](/examples/protocols/orca-compound) | Orca | How much the position had earned |
| [Harvest](/examples/protocols/orca-harvest) | Orca | Which positions have earned enough to collect |
| [Price gate](/examples/protocols/pyth-gate) | Pyth → Jupiter | Whether the price is recent, precise and in range |
| [Conditional tip](/examples/protocols/jito-tip) | Jupiter → Jito | Whether the trade's profit covered the tip |

## Composition

Templates that call other programs in sequence, with checks between the calls. Your client still
finds routes, quotes and accounts; the template fixes the order of the calls and the checks.

| Recipe | What it does | Plain transaction? |
| --- | --- | --- |
| [Swap then deposit](/examples/composition#swap-then-deposit) | Swap, check that enough arrived, then deposit | Yes, weaker |
| [Claim then distribute](/examples/composition#claim-then-distribute) | Claim rewards once, then pay each recipient the same amount | Yes |
| [Fallback route](/examples/composition#primary-or-fallback-route) | Exactly one of two routes runs | Yes, weaker |
| [Time-gated governance](/examples/composition#time-gated-governance-execution) | Execute only once a proposal is approved and its time has passed | No |
| [Keeper crank](/examples/composition#bounded-keeper-crank) | Same maintenance call over many accounts | Yes |

## Payments

Templates that pay out SOL.

| Recipe | What it does | Plain transaction? |
| --- | --- | --- |
| [SOL payroll](/examples/payments#bounded-sol-payroll) | Pay up to 30 recipients in one instruction | Yes |
| [Revenue split](/examples/payments#basis-point-revenue-split) | Split an amount by basis points, losing nothing to rounding | Yes, weaker |
| [Weighted rewards](/examples/payments#index-weighted-rewards) | Pay the first recipient 1 × base, the second 2 × base, and so on | Yes, weaker |
| [Deadline refund](/examples/payments#deadline-refund) | Refund only before a deadline | Yes, weaker |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | Move SOL up to a cap without dropping below a reserve | Yes, weaker |

## Token accounts

Templates that create, pay into, close and check SPL token accounts. An ATA (associated token
account) is the standard token account for a given wallet and mint.

| Recipe | What it does | Plain transaction? |
| --- | --- | --- |
| [Create then transfer](/examples/token-accounts#assert-create-then-transfer) | Check each recipient's ATA address, create it if missing, then pay | Yes |
| [Token payroll](/examples/token-accounts#existing-account-token-payroll) | Pay many existing token accounts | Yes |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | Create an ATA only if it doesn't exist yet | Yes |
| [Close empty accounts](/examples/token-accounts#close-empty-token-accounts) | Close each account whose balance is zero | Yes, weaker |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | Check the transfer took exactly the stated amount | Yes, weaker |
