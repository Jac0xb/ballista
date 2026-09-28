# Withdraw everything, with a minimum

<p class="protocol-line">marginfi</p>

**Status:** Run as real transactions against marginfi, copied from mainnet at one slot into
LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

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

<<< ../../../clients/js/examples/protocols/marginfi-withdraw-all-with-floor.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#marginfi-withdraw [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/marginfi-withdraw.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#marginfi-withdraw [Rust · Run]

:::

The last two bytes of the withdrawal's instruction data are `Option::Some(true)` for
`withdraw_all`. In that mode marginfi ignores `amount`, but the field still has to be there,
because Borsh (the binary format Anchor programs use for instruction arguments) reads every field.

## Run it

The Run tabs pass the 10 declared accounts in order (`marginfi`, `tokenProgram`, `marginfiGroup`,
`marginfiAccount`, `authority`, `bank`, `bankLiquidityVault`, `bankLiquidityVaultAuthority`,
`destinationAta`, `treasuryAta`), then the input `minimumWithdrawn`, then the `healthAccounts`
[account group](/guide/account-groups). `authority` signs but is not writable. Nothing has to go
before the run.

After the withdrawal, marginfi checks the account's health: whether what it still holds covers
what it owes, at each bank's oracle price. It reads the banks and oracles from the accounts after
its own eight. So `healthAccounts` holds, for each balance still open once this one is emptied,
its bank then its oracle, highest bank address first. It is empty if this was the only balance;
leave a balance out and marginfi refuses the withdrawal. `marginfiHealthAccounts` (TypeScript) and
`marginfi_health_accounts` (Rust), next to the runs, build it, for banks priced by one oracle
account.

## What has been tested

- **Against the real programs.** `tests/protocols/tests/marginfi_withdraw_all_with_floor.rs`
  empties a 100 USDC marginfi balance and sweeps it to the treasury: all of it, less at most the
  one base unit marginfi's rounding can keep. With 1 SOL also deposited, `healthAccounts` carried
  the SOL bank and its oracle, and the USDC came out, leaving the SOL. The whole transaction took
  60,162 compute units (Solana's measure of execution cost) and 550 bytes, or 78,056 and 616 with
  the SOL balance.
- **Failures.** A `minimumWithdrawn` one unit above the deposit fails at `withdrawalMetItsFloor`.
  An attacker's account as `treasuryAta` fails at `sweepGoesToTheAuthority`, and as both
  `destinationAta` and `treasuryAta` at `withdrawalGoesToTheAuthority`, before marginfi is called.
  Called directly without the SOL bank and oracle, marginfi refuses the same withdrawal
  (`InvalidBankAccount`, in `tests/protocols/tests/marginfi_contract.rs`).
- **Not tested.** Mainnet itself, more than one remaining balance, banks priced by more than one
  account (staked, Kamino), and Token-2022 tokens: the template accepts SPL Token accounts only. No
  test holds a marginfi debt or updates marginfi's oracles, and the health check values an asset
  with a stale price at zero.
- A test reads the template and checks that the owner checks come first, and that the withdrawal
  passes `healthAccounts` after its eight accounts, with the vault authority read-only
  (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
