# Sell all of a coin above a floor

<p class="protocol-line">pump.fun</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against pump.fun's
bonding-curve program and live curves copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 4,925 of the tested transaction's 71,505
[compute units](/reference/glossary#compute-units); pump.fun took the rest. Ballista charges no
fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Sells everything in a pump.fun coin's token account back to its bonding curve, and fails unless
the seller receives at least `minSolOut` lamports.

pump.fun's `sell` takes the amount to sell, fixed when you sign. If a buy or a transfer lands
before your sale, the balance has changed: the sale leaves the difference behind, or fails. The
template reads the balance while the transaction runs, and sells exactly that.

The template:

1. requires the seller to own the token account (`sellsTheSellersOwnTokens`). pump.fun sells from
   any account the seller can move tokens out of, including one it is only a delegate on;
2. requires the account to hold `mint` (`holdsTheCurvesCoin`). pump.fun derives the curve from
   `mint` and refuses any other;
3. requires the curve not to have graduated (`curveNotGraduated`), by its `complete` flag. A
   graduated coin trades on PumpSwap, and pump.fun would refuse the sale with its own
   `BondingCurveComplete` (6005);
4. reads the balance and requires it to be above 0 (`hasTokensToSell`);
5. sells exactly that balance;
6. requires the seller's lamports to have grown by at least `minSolOut`
   (`receivedAtLeastMinSolOut`): what arrived, after pump.fun's and the creator's fees.

pump.fun's own `min_sol_output` is left at 0. The last check covers it, on what actually arrived,
and a failure names its step.

It does not guard against **a floor set too low**. `minSolOut` is yours to set: quote the sale from
the curve's reserves and pump.fun's fees, less a tolerance.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/pump-fun-sell-all.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#pump-sell-all [Rust · Template]

<<< @/../clients/js/examples/protocols/run/pump-sell-all.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#pump-sell-all [Rust · Run]

:::

The token account's mint, owner and balance sit at offsets 0, 32 and 64, as in SPL Token. The
Rust template takes its program addresses, offsets, `token_2022_account()` and `balance_of()` from
the [shared helpers](/examples/protocols/#rust-helpers).

## Run it

The Run tabs pass the 16 declared accounts in the order pump.fun's `sell` takes them: `global`,
`feeRecipient`, `mint`, `bondingCurve`, `curveTokenAccount`, `sellerTokenAccount`, `seller`,
`systemProgram`, `creatorVault`, `tokenProgram`, `eventAuthority`, `pumpProgram`, `feeConfig`,
`feeProgram`, `bondingCurveV2` and `buybackFeeRecipient`, with `pumpProgram` first. The input is
`minSolOut`.

- `sellerTokenAccount` is the seller's associated token account for the coin.
- `feeRecipient` is an ordinary fee recipient, or a reserved one for a mayhem-mode coin.
- `bondingCurveV2` is a PDA pump.fun has taken since its April 2026 upgrade, whether or not it
  exists.

:::: details Deriving pump.fun's accounts
::: code-group

<<< @/../clients/js/examples/protocols/run/pump-fun.ts#pump-accounts [TypeScript]

<<< @/../clients/rust/examples/protocol_templates_run.rs#pump-accounts [Rust]

:::
::::

The sale fits a transaction without a lookup table (707 bytes) and the default compute limit. Like
the [basket](/examples/protocols/pump-buy-basket), it pins Token-2022 and calls pump.fun's `sell`,
so it trades SOL-paired coins created with `create_v2`.

## What has been tested

- **In LiteSVM.** In `tests/protocols/tests/pump_fun_sell_all.rs`, the seller first buys 2,000,000
  of each live coin through pump.fun's own `buy`. The template sold an ordinary coin's whole
  balance for 56,955,651 lamports, exactly what pump.fun's own `sell` paid for the same amount, in
  71,505 compute units and 707 bytes. A mayhem-mode coin sold the same way.
- **A balance that grew.** A run built for one balance, landing after a second buy had doubled it,
  sold all of it.
- **The floor.** A floor equal to the proceeds lands. One lamport more fails at
  `receivedAtLeastMinSolOut`, and the sale reverts.
- **Failures.** A graduated coin fails at `curveNotGraduated`, an empty account at
  `hasTokensToSell`, and the seller's account of another coin at `holdsTheCurvesCoin`. Another
  coin's curve fails in pump.fun with Anchor's `ConstraintSeeds` (2006). A stranger's account that
  the seller is a delegate on fails at `sellsTheSellersOwnTokens`, though pump.fun alone sells the
  stranger's coins and pays the seller.
- **Accounts.** The semantics test checks `sell`'s accounts and writable flags against a mainnet
  transaction.

::: warning Check before you upload
pump.fun changes often: its April 2026 upgrade added two accounts to every `buy` and `sell`. Check
the accounts against pump.fun's current IDL and a recent transaction before you upload.
:::

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
