# Compound the fees you collected

<p class="protocol-line">Orca</p>

**Status:** Run as real transactions against Orca's Whirlpools program and a SOL/USDC pool, copied
from mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

## What it does

Collects the fees an Orca Whirlpools position has earned and adds them back to the position as
liquidity.

What a position has earned is known only when the transaction runs, since trades keep paying it
fees after you sign. Whoever holds a position's NFT, a token with a supply of one, owns the
position. The template:

1. requires the fee accounts, `tokenOwnerAccountA` and `tokenOwnerAccountB`, to belong to the NFT's
   holder, not merely to the signer (`feesGoToThePositionHolder`). Whirlpools checks only their
   mints, so a run built by someone else could otherwise send the fees anywhere;
2. calls `update_fees_and_rewards` if the position has liquidity. Without it, `fee_owed_a` and
   `fee_owed_b` hold only what the last update recorded, usually 0. Whirlpools refuses the update
   for a position without liquidity;
3. reads `fee_owed_a` and `fee_owed_b`;
4. calls `collect_fees` if either fee is above `dustFloor`, paying both fees to the fee accounts;
5. calls `increase_liquidity_by_token_amounts_v2` if both fees are above `dustFloor` and the
   position has liquidity. With the fees as its limits, Whirlpools adds the most liquidity they buy
   at the price when the transaction runs. One fee is used whole, and the rest of the other stays in
   the holder's account.

While the pool's price is inside the position's range, the prices it provides liquidity for, new
liquidity takes both tokens. So fees in one token are collected but not reinvested, and a position
emptied of liquidity is collected, not refilled.

::: warning Don't use a floor of 0
A fee above the floor can still be too small to buy any liquidity. The deposit then fails with
Whirlpools' `LiquidityZero` (6012), and the whole run reverts, collect included. A few base units (a
token's smallest unit) cover SOL/USDC, but a pool whose token A is worth less per unit needs more:
a few thousand is safer. One floor applies to both fees, each counted in its own token's base units.
:::

## Template

::: code-group

<<< @/../clients/js/examples/protocols/orca-compound-fees.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#orca-compound [Rust · Template]

<<< @/../clients/js/examples/protocols/run/orca-compound.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#orca-compound [Rust · Run]

:::

The Rust tabs' `program`, `anchor` and account flags are
[shared helpers](/examples/protocols/#rust-helpers).

The offsets come from Orca's `Position` account and the SPL Token account; see
[reading offsets](/examples/protocols/#reading-offsets-from-an-account).

## Run it

The Run tabs pass the template's 15 accounts in the order it declares them: `whirlpoolProgram`,
`tokenProgram`, `memoProgram`, `positionAuthority`, `whirlpool`, `position`,
`positionTokenAccount`, `tokenMintA`, `tokenMintB`, `tokenOwnerAccountA`, `tokenOwnerAccountB`,
`tokenVaultA`, `tokenVaultB`, `tickArrayLower` and `tickArrayUpper`. There is no account group.

- `memoProgram` is SPL Memo, which Whirlpools' v2 instructions take.
- `tickArrayLower` and `tickArrayUpper` hold the position's lower and upper ticks, the prices its
  range starts and ends at. Whirlpools keeps a pool's ticks in accounts of 88, called tick arrays.
- Both pool mints must be SPL Token mints, as SOL and USDC are.

The inputs are `dustFloor` (a `u64`), then `minSqrtPrice` and `maxSqrtPrice` (`u128`s), the lowest
and highest pool price the deposit accepts. Whirlpools stores a price as its square root, the sqrt
price, in Q64.64 fixed point: a `u128` whose low 64 bits are the fraction. Orca's
`get_sqrt_price_slippage_bounds` computes both bounds from the pool's current sqrt price and a
tolerance in basis points (hundredths of a percent). If the price is outside them when the run
lands, the deposit fails with `PriceSlippageOutOfBounds` (6069), and the whole run reverts.

`positionAuthority` signs for the position. It can be the holder, or a delegate: an account, such as
a keeper bot, that the holder approved on `positionTokenAccount` with the token program's `approve`.
A delegate that reinvests also needs approval on both fee accounts, since the deposit spends from
them under its signature. Approve a bounded amount there: an unlimited approval lets the keeper
spend those accounts outside this template too.

## What has been tested

- **Against the real program.** `tests/protocols/tests/orca_compound_fees.rs` earns fees with real
  swaps through the pool, then runs the template:
  - Fees in both tokens are updated, collected and reinvested. The liquidity added is exactly what
    Orca's own math says the fees buy, and one fee is used whole. The run took 43,784 compute units
    (Solana's measure of execution cost) and 706 bytes.
  - Fees in one token are collected whole, not reinvested. With no fees, only the update runs. A
    position without liquidity gets no Whirlpools call, and an emptied one is collected, not
    refilled.
  - Fees at or below `dustFloor` stay owed. At a floor equal to the smaller fee, both fees are
    collected and neither is reinvested.
  - A price move inside the bounds still lands.
- **Failures.** A price outside the bounds fails with `PriceSlippageOutOfBounds`, and the update and
  collect revert with it. At a `dustFloor` of 0, a position over the full price range owed a fee of
  1 lamport (a billionth of a SOL), too little to buy liquidity, fails the same way with
  `LiquidityZero`; at a floor of 1 it lands and collects both fees. A stranger's account in both fee
  slots, or in token B's alone, fails at `feesGoToThePositionHolder`.
- **A delegate.** A keeper approved on the NFT and both fee accounts signs the run, and the holder's
  fees are collected and reinvested.
- **Whirlpools alone.** Without Ballista, fees owed rise only on an update, the update fails
  without liquidity, and in-range liquidity needs both tokens
  (`tests/protocols/tests/orca_setup.rs`).
- Every Whirlpools call passes the same accounts, in the same order and with the same signer and
  writable flags, as Orca's own Rust client (`tests/protocols/tests/orca_cpis.rs`).
- A test reads the template and checks that `feesGoToThePositionHolder` compares the fee accounts'
  owners with `positionTokenAccount`'s owner, never with `positionAuthority`
  (`clients/js/src/protocol-semantics.test.ts`).
- An opt-in test checks the Orca offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
