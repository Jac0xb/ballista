# Withdraw everything, with a minimum

<p class="protocol-line">marginfi</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against marginfi's
program and accounts copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 5,472 of the tested transaction's 60,030
[compute units](/reference/glossary#compute-units); marginfi and the token transfer took the rest.
Ballista charges no fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Empties a marginfi position, reverts unless enough came out, and moves the proceeds to a treasury
account of your own.

marginfi's `lending_account_withdraw(amount, withdraw_all)` can empty a position, but it doesn't
tell the caller how much came out, so nothing later in the transaction can depend on it. A
withdrawal that returns far less than expected still succeeds: the bank (marginfi's pool for one
token) may have been drained, its utilization (the share of deposits lent out) may have capped the
withdrawal, or the position may have been liquidated. Whatever comes next then runs on a wrong
assumption.

The template:

- requires `destinationAta` and `treasuryAta` to belong to `authority`, since marginfi pays
  whatever token account it is given (`withdrawalGoesToTheAuthority`, `sweepGoesToTheAuthority`);
- records the destination balance, withdraws everything, and measures how much arrived;
- reverts if that is less than `minimumWithdrawn` (`withdrawalMetItsFloor`);
- otherwise transfers exactly that amount to `treasuryAta`.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/marginfi-withdraw-all-with-floor.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#marginfi-withdraw [Rust · Template]

<<< @/../clients/js/examples/protocols/run/marginfi-withdraw.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#marginfi-withdraw [Rust · Run]

:::

The Rust tabs' `program`, `anchor` and account flags are
[shared helpers](/examples/protocols/#rust-helpers).

The last two bytes of the withdrawal's instruction data are `Option::Some(true)` for
`withdraw_all`. In that mode marginfi ignores `amount`, but the field still has to be there,
because Borsh (the binary format Anchor programs use for instruction arguments) reads every field.

## Run it

The Run tabs pass the 10 declared accounts in order (`marginfi`, `tokenProgram`, `marginfiGroup`,
`marginfiAccount`, `authority`, `bank`, `bankLiquidityVault`, `bankLiquidityVaultAuthority`,
`destinationAta`, `treasuryAta`), then the input `minimumWithdrawn`, then the `healthAccounts`
[account group](/guide/account-groups). `authority` signs but is not writable.

After the withdrawal, marginfi checks the account's health: whether what it still holds covers
what it owes, at each bank's oracle price. It reads the banks and oracles from the accounts after
its own eight. So `healthAccounts` holds, for each balance still open once this one is emptied,
its bank then its oracle, highest bank address first. It is empty if this was the only balance;
leave a balance out and marginfi refuses the withdrawal. `marginfiHealthAccounts` (TypeScript) and
`marginfi_health_accounts` (Rust), next to the runs, build it, for banks priced by one oracle
account.

Nothing has to go before the run while the account owes nothing. Once it owes something, the health
check needs current prices for every balance left: marginfi counts an asset with a stale price as
zero and refuses a debt with one. Update those oracles earlier in the transaction.

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/marginfi_withdraw_all_with_floor.rs` empties a 100 USDC
  marginfi balance and sweeps it to the treasury: all of it, less at most the one base unit
  marginfi's rounding can keep. With 1 SOL also deposited, `healthAccounts` carried the SOL bank and
  its oracle, and the USDC came out, leaving the SOL. The whole transaction took 60,030 compute
  units and 550 bytes, or 77,924 and 616 with the SOL balance.
- **Failures.** A `minimumWithdrawn` one unit above the deposit fails at `withdrawalMetItsFloor`.
  An attacker's account as `treasuryAta` fails at `sweepGoesToTheAuthority`, and as both
  `destinationAta` and `treasuryAta` at `withdrawalGoesToTheAuthority`, before marginfi is called.
  Called directly without the SOL bank and oracle, marginfi refuses the same withdrawal
  (`InvalidBankAccount`, in `tests/protocols/tests/marginfi_contract.rs`).
- **Not tested.** Devnet and mainnet, more than one remaining balance, banks priced by more than one
  account (staked, Kamino), and Token-2022 tokens: the template accepts SPL Token accounts only. No
  test holds a marginfi debt or updates marginfi's oracles.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
