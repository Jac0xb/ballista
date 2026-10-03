# `jupiterDailyCapSwap` against the real programs

Tests: [`tests/jupiter_daily_cap.rs`](../tests/jupiter_daily_cap.rs).

The snapshot and route are the same as in [the oracle swap's findings](oracle-swap.md): slot
451,100,151, and route `solToUsdc`, 1 SOL for USDC through Meteora DLMM.

The template charges each route's `inAmount` against a per-caller cap of 1,728,000,000 lamports,
refilling at 20,000 a second: 1.728 SOL a day. The count lives in a `dailySpend` registry entry
keyed by the signing caller's address. The route must sell the caller's own wrapped SOL, and exactly
`inAmount` of it, so the cap counts lamports whatever the route. The template passes `route` its
token program, the caller and that wrapped-SOL source, and forwards the rest of the route's accounts
as `actionAccounts`. The run takes `route`'s place in Jupiter's own transaction, and the entry is
created inside the caller's first run.

Each figure is marked:
- **asserted:** the tests fail if it changes;
- **measured:** printed by the tests (`--nocapture`) at this commit, and not asserted.

## Runs

| Run | Result | How |
| --- | --- | --- |
| The wallet's first swap | Lands and creates its entry, owned by Ballista: 88 bytes (a 72-byte header, then `spent` and `lastSpend`), holding the rent-exempt minimum of 1,503,360 lamports. `spent` is 1,000,000,000 and `lastSpend` the clock | Asserted; the rent's value measured |
| The same swap, alongside | It buys 123,106,283 USDC units, as Jupiter's own transaction does, and costs the wallet that transaction's lamports plus the entry's rent | Asserted; the USDC measured |
| A second swap in the same second | Fails at `withinRateLimit` (pc 33) after 4,091 CU of Ballista's run. Jupiter never runs | Asserted; pc and CU measured |
| Clock moved on 13,599 s (write rule 3) | Still fails at `withinRateLimit`, and the entry is unchanged | Asserted |
| One second more: 13,600 s | Lands. `spent` is 1,728,000,000, exactly the cap, and `lastSpend` the new clock | Asserted |
| A second caller, with the wallet's limit spent | Lands on an entry of its own, created in its run, with `spent` 1,000,000,000. The wallet's entry is unchanged | Asserted |
| The second caller passes the wallet's entry, which does not exist yet | `InvalidRegistryEntry` (6025) from Ballista at pc 10, the open before the first step, which no label covers. It costs 2,676 CU, and Jupiter never runs | Asserted; pc and CU measured |
| The same, after the wallet's first swap created it | `InvalidRegistryEntry` at pc 10, after 2,346 CU. The wallet's entry is unchanged, and the second caller's is never created | Asserted; pc and CU measured |
| The wallet sells 150 USDC (write rule 1) on route `usdcToSol` | Fails at `spendsWrappedSol` (pc 13) after 4,216 CU. Jupiter never runs, and no entry is left | Asserted; pc and CU measured |
| The same sale, with the wallet's second wrapped-SOL account at `sourceAta` | Fails at `soldWhatTheCapCharged` (pc 44), once the route has run, after 58,485 CU of Ballista's run. The USDC and the decoy are untouched, and no entry is left | Asserted; pc and CU measured |
| The wallet's swap with a 100 bps platform fee paid to an attacker's USDC account, at `slippageBps` 200 | Fails at `platformFeeWithinCap` (pc 39) after 6,228 CU of Ballista's run. Jupiter never runs, no entry is left, and the attacker gets nothing. It landed before the fix (85,728 CU, 839 bytes) | Asserted; pc and CU measured |

- **The wait.** The test computes it from the constants: ⌈(2 × 1,000,000,000 − 1,728,000,000) ÷
  20,000⌉ = 13,600 s.
- **The second caller.** Its transaction is Jupiter's own, rebuilt for it: `rekeyed` swaps the
  wallet and its two token accounts for the caller's. Jupiter's setup then creates the caller's
  token accounts. Only the caller's SOL is written directly (write rule 1).
- **The decoy.** It is made through the System and Token programs, as a wallet would make it:
  `CreateAccount`, then `InitializeAccount3`.
- **Why the refusals cost what they do.** Every run opens its entry before the first step, and the
  open creates a missing one before any check can fail. So the USDC sale's refusal pays for a
  creation that the failure then reverts. And a missing entry costs more to refuse than an existing
  one: the open derives the address it would create before comparing it. Both are inferred from the
  numbers and the runtime's code.

## Compute units and size

All measured.

|  | Creating the entry | On an existing entry | Jupiter's own transaction |
| --- | --- | --- | --- |
| Transaction | 82,728 CU | 71,942 CU | 73,084 CU |
| Ballista's run | 50,629 CU | 48,922 CU | none |
| Jupiter's `route`, within it | 40,985 CU | 41,013 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 9,644 CU | 7,909 CU | none |
| Wire size | 807 bytes | 807 bytes | 698 bytes |

- **Creating the entry costs 1,735 CU** of Ballista's own work: the address derivation and the
  System program's CPI. The second caller's creating run cost the same: 50,599 CU in its run, 40,955
  of them Jupiter's, so 9,644 its own.
- **The platform-fee cap costs 108 CU a run** of Ballista's own work: before it, 9,536 and 7,801.
  It adds 48 bytes to the template and none to the transaction.
- **The source checks cost 869 CU a run** of Ballista's own work: before them it was 8,667 and 6,932.
  They add 250 bytes to the template and none to the transaction. The source moved from the group
  to a fixed slot, and the inputs did not change.
- **Compare Ballista's own work, not the totals.** The existing-entry run is the second swap on
  the pool, after the first moved it. Its total is also lower because Jupiter's setup found the USDC
  account already there, where the first swap's setup created it (inferred from the numbers).
- **Size.** The run's transaction is 109 bytes larger than Jupiter's own, the same with or without
  the entry's creation. Most of it is three keys that Jupiter's transaction does not carry:
  Ballista, the template and the entry.
- **The template.** It is 1,014 bytes: 966 before the platform-fee cap, 716 before the source
  checks.

## Findings

### P1, fixed: the cap counted whatever the route sold, in that mint's base units

**The problem.** The template charged `inAmount`, which is in the base units of the route's input
mint, and nothing pinned that mint. So the cap meant 1.728 SOL only for routes that sold SOL.

**Before** (`ed4a7c3`). A one-off probe, not committed, ran route `usdcToSol` (150 USDC for SOL)
through the template. Every run landed, all in the same second:

| Sale | Charged | `spent` after |
| --- | --- | --- |
| 150 USDC | 150,000,000, as if it were 0.15 SOL | 150,000,000 |
| 1 SOL | 1,000,000,000 | 1,150,000,000 |
| 150 USDC | 150,000,000 | 1,300,000,000 |

At the snapshot's price, 150 USDC is worth about 1.22 SOL. So a caller who sold only USDC could sell
1,728 USDC a day, about 14 SOL's worth. `selling_usdc_fails_at_spends_wrapped_sol` was written first
and failed against that template: the sale landed, in 72,524 CU and 726 bytes.

**Pinning the source alone was not enough.** Checks of the source's mint and owner stopped the plain
USDC sale at `spendsWrappedSol`. But Jupiter asks of `route`'s source position only that it hold
`in_amount`, of any mint and any owner. It moves the accounts its steps name, not that one: see the
oracle swap's [P1](oracle-swap.md#p1-fixed-decoy-token-accounts-made-the-fill-check-pass-at-any-price).
So, with only the two checks, `a_wrapped_sol_decoy_at_the_source_fails_at_sold_what_the_cap_charged`
landed. The wallet's second wrapped-SOL account at `sourceAta` passed both, the step sold all
150 USDC, and the entry was charged 150,000,000. That version was never committed.

**The fix.** Three requirements:
- `spendsWrappedSol`: the source's mint (offset 0) is wrapped SOL, a template constant;
- `sourceBelongsToTheCaller`: the source's owner (offset 32) is the actor;
- `soldWhatTheCapCharged`: once the route has run, the source holds exactly `inAmount` less than
  it did before.

The first two come before the rate limit's registry writes and the swap. The third can only follow
the swap; when it fails, the charge reverts with everything else. Jupiter requires the source to
hold `in_amount`, so the third's subtraction cannot underflow.

**After.** Both runs above now fail: the USDC sale at `spendsWrappedSol`, before Jupiter; the decoy
at `soldWhatTheCapCharged`, after the route. Neither leaves an entry. All of this is asserted.

**What it still allows.** The caller signs `route`, and Jupiter passes that authority to every
step. A route can sell exactly `inAmount` from the source and pass every check, while a further
step debits another of the caller's token accounts, uncounted. This is the oracle swap's
[open question](oracle-swap.md#open-the-signers-authority-over-its-other-token-accounts). It is
inferred, not tested here.

### P1, fixed: the route's platform fee was the builder's

**The problem.** `route` pays `platform_fee_bps` of its output to the platform fee account at
position 6 of its list, in `actionAccounts`. The run's builder chose both, and nothing checked
either, so a caller's swaps within the cap could each pay up to 2.55% to whoever built the run.

**What it allowed.** The wallet's swap with a 100 bps fee paid to an attacker's USDC account, at
`slippageBps` 200, **landed** against the unfixed template (85,728 CU, 839 bytes): the red run of
`a_hostile_platform_fee_fails_at_platform_fee_within_cap`.

**The fix.** The template declares `MAX_PLATFORM_FEE_BPS`, 0, and requires `platformFeeBps` to be
at most it (`platformFeeWithinCap`) right before the swap. The same run fails there (pc 39) after
6,228 CU of Ballista's run; the whole transaction reverts, so the rate limit's charge and the
entry's creation go with it. See [`platform-fee.md`](platform-fee.md).

### Open: the cap limits this template's runs, not the caller

The caller signs Jupiter's `route` itself. The same signer can call Jupiter directly, or through
another template, with no cap at all. The cap binds only where the signer can reach Jupiter through
this template alone. This is inferred, not tested.

### Entries are permanent

A caller's first run pays the entry's 1,503,360 lamports. Nothing closes an entry or returns its
rent: the registry's design leaves that out of scope.

## Bisecting

Two commits fail one test on their own: `every_example_lines_up_with_its_payload`, in the protocol
harness.
- `9492647` adds the template, and `83abe29` its TypeScript run.
- Both hold 13 examples in the fixture, while the harness still expects 12 until `0023952`.

Skip both when bisecting that test. The history is left as it is.

## Notes for the docs session

- **Where the code is.** Every name below is unchanged by the fix except `DailyCapAccounts`, which
  is new.
  - The template: `clients/js/examples/protocols/jupiter-daily-cap-swap.ts`.
  - The Rust mirror: region `jupiter-daily-cap` in `clients/rust/examples/protocol_templates.rs`.
  - The TypeScript run: `clients/js/examples/protocols/run/jupiter-daily-cap.ts`, which exports
    `buildDailyCapRun`. It now takes `sourceAta`.
  - The Rust run: region `jupiter-daily-cap` in `clients/rust/examples/protocol_templates_run.rs`.
    It holds `run_jupiter_daily_cap`, which now takes a `DailyCapAccounts` of the actor and the
    source.

  Both runs find the entry with `findRegistryEntryAddress` or `find_registry_entry_address`, at
  registry index 0 and the caller's own address.
- **What the runs pass.**
  - Accounts: `actionProgram` (Jupiter), `tokenProgram`, `actor`, `sourceAta` (the actor's wrapped
    SOL), `spend` (the entry) and `systemProgram`.
  - Inputs: `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`.
  - The group: `route`'s accounts after the third.

  No separate instruction creates the entry.
- **Counts now out of date.**
  - `docs/examples/protocols/index.md`: lines 3–4 say "Twelve example templates. Eleven work with
    real Solana protocols", and "The twelfth" is the signed quote. Lines 8 and 35 say "The eleven
    protocol templates". It is now thirteen templates, twelve of them protocol templates.
  - `docs/reference/rust.md`, line 25, says `protocol_templates` "builds all twelve". It is now
    thirteen.
- **Say what the cap counts.** It counts lamports: the route must sell the caller's own wrapped SOL,
  exactly `inAmount` of it. Also say what it does not stop: see
  [P1's last paragraph](#p1-fixed-the-cap-counted-whatever-the-route-sold-in-that-mints-base-units).
