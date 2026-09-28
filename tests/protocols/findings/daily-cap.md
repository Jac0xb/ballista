# `jupiterDailyCapSwap` against the real programs

Tests: [`tests/jupiter_daily_cap.rs`](../tests/jupiter_daily_cap.rs).

The snapshot and route are the same as in [the oracle swap's findings](oracle-swap.md): slot
451,100,151, and route `solToUsdc`, 1 SOL for USDC through Meteora DLMM.

The template charges each route's `inAmount` against a per-caller cap of 1,728,000,000, refilling
at 20,000 a second: 1.728 SOL a day, for a route that sells SOL. The count lives in a `dailySpend`
registry entry keyed by the signing caller's address. The template passes `route` its token program
and the caller, and forwards the rest of the route's accounts as `actionAccounts`. The run takes
`route`'s place in Jupiter's own transaction, and the entry is created inside the caller's first
run.

Each figure is marked:
- **asserted:** the tests fail if it changes;
- **measured:** printed by the tests (`--nocapture`) at this commit, and not asserted.

## Runs

| Run | Result | How |
| --- | --- | --- |
| The wallet's first swap | Lands and creates its entry, owned by Ballista: 88 bytes (a 72-byte header, then `spent` and `lastSpend`), holding the rent-exempt minimum of 1,503,360 lamports. `spent` is 1,000,000,000 and `lastSpend` the clock | Asserted; the rent's value measured |
| The same swap, alongside | It buys 123,106,283 USDC units, as Jupiter's own transaction does, and costs the wallet that transaction's lamports plus the entry's rent | Asserted; the USDC measured |
| A second swap in the same second | Fails at `withinRateLimit` (pc 24) after 3,484 CU of Ballista's run. Jupiter never runs | Asserted; pc and CU measured |
| Clock moved on 13,599 s (write rule 3) | Still fails at `withinRateLimit`, and the entry is unchanged | Asserted |
| One second more: 13,600 s | Lands. `spent` is 1,728,000,000, exactly the cap, and `lastSpend` the new clock | Asserted |
| A second caller, with the wallet's limit spent | Lands on an entry of its own, created in its run, with `spent` 1,000,000,000. The wallet's entry is unchanged | Asserted |
| The second caller passes the wallet's entry, which does not exist yet | `InvalidRegistryEntry` (6025) from Ballista at pc 8, the open before the first step, which no label covers. It costs 2,536 CU, and Jupiter never runs | Asserted; pc and CU measured |
| The same, after the wallet's first swap created it | `InvalidRegistryEntry` at pc 8, after 2,176 CU. The wallet's entry is unchanged, and the second caller's is never created | Asserted; pc and CU measured |

- **The wait.** The test computes it from the constants: ⌈(2 × 1,000,000,000 − 1,728,000,000) ÷
  20,000⌉ = 13,600 s.
- **The second caller.** Its transaction is Jupiter's own, rebuilt for it: `rekeyed` swaps the
  wallet and its two token accounts for the caller's. Jupiter's setup then creates the caller's
  token accounts. Only the caller's SOL is written directly (write rule 1).
- **Why the two refusals cost different amounts.** An entry that does not exist yet costs more to
  refuse: the open derives the address it would create before comparing it (inferred from the
  runtime's code).

## Compute units and size

All measured.

|  | Creating the entry | On an existing entry | Jupiter's own transaction |
| --- | --- | --- | --- |
| Transaction | 81,751 CU | 70,965 CU | 73,084 CU |
| Ballista's run | 49,652 CU | 47,945 CU | none |
| Jupiter's `route`, within it | 40,985 CU | 41,013 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 8,667 CU | 6,932 CU | none |
| Wire size | 807 bytes | 807 bytes | 698 bytes |

- **Creating the entry costs 1,735 CU** of Ballista's own work: the address derivation and the
  System program's CPI. The second caller's creating run cost the same: 49,622 CU in its run, 40,955
  of them Jupiter's, so 8,667 its own.
- **Compare Ballista's own work, not the totals.** The existing-entry run is the second swap on
  the pool, after the first moved it. Its total is also lower because Jupiter's setup found the USDC
  account already there, where the first swap's setup created it (inferred from the numbers).
- **Size.** The run's transaction is 109 bytes larger than Jupiter's own, the same with or without
  the entry's creation. Most of it is three keys that Jupiter's transaction does not carry:
  Ballista, the template and the entry.
- **The template.** It is 716 bytes.

## Findings

### Open: the cap counts whatever the route sells, in that mint's base units

The template charges `inAmount`, which is in the base units of the route's input mint. Nothing pins
that mint, so the cap means 1.728 SOL only for routes that sell SOL.

A one-off probe, not committed, ran route `usdcToSol` (150 USDC for SOL) through the template.
Every run landed, all in the same second:

| Sale | Charged | `spent` after |
| --- | --- | --- |
| 150 USDC | 150,000,000, as if it were 0.15 SOL | 150,000,000 |
| 1 SOL | 1,000,000,000 | 1,150,000,000 |
| 150 USDC | 150,000,000 | 1,300,000,000 |

At the snapshot's price, 150 USDC is worth about 1.22 SOL. A caller who sells only USDC may sell
1,728 USDC a day, about 14 SOL's worth. A mint with fewer decimals, or more value per base unit,
stretches the cap further.

Pinning the input would take the route's source account and a mint check, as the oracle swap has.
Jupiter does not tie `route`'s source account to what its steps spend, though (see
[the oracle swap's findings](oracle-swap.md)), so an exact charge would also need the balance the
swap took. This is left as a design question.

### Open: the cap limits this template's runs, not the caller

The caller signs Jupiter's `route` itself. The same signer can call Jupiter directly, or through
another template, with no cap at all. The cap binds only where the signer can reach Jupiter through
this template alone. This is inferred, not tested.

### Entries are permanent

A caller's first run pays the entry's 1,503,360 lamports. Nothing closes an entry or returns its
rent: the registry's design leaves that out of scope.

## Notes for the docs session

- **Where the code is.**
  - The template: `clients/js/examples/protocols/jupiter-daily-cap-swap.ts`.
  - The Rust mirror: region `jupiter-daily-cap` in `clients/rust/examples/protocol_templates.rs`.
  - The TypeScript run: `clients/js/examples/protocols/run/jupiter-daily-cap.ts`, which exports
    `buildDailyCapRun`.
  - The Rust run: region `jupiter-daily-cap` in `clients/rust/examples/protocol_templates_run.rs`,
    which holds `run_jupiter_daily_cap`.

  Both runs find the entry with `findRegistryEntryAddress` or `find_registry_entry_address`, at
  registry index 0 and the caller's own address.
- **What the runs pass.** The template's accounts are `actionProgram` (Jupiter), `tokenProgram`,
  `actor`, `spend` (the entry) and `systemProgram`. Its inputs are `routePlan`, `inAmount`,
  `quotedOutAmount`, `slippageBps` and `platformFeeBps`. Its group is `route`'s accounts after the
  second. No separate instruction creates the entry.
- **Counts now out of date.**
  - `docs/examples/protocols/index.md`: lines 3–4 say "Twelve example templates. Eleven work with
    real Solana protocols", and "The twelfth" is the signed quote. Lines 8 and 35 say "The eleven
    protocol templates". It is now thirteen templates, twelve of them protocol templates.
  - `docs/reference/rust.md`, line 25, says `protocol_templates` "builds all twelve". It is now
    thirteen.
- **Say what the cap counts.** It is 1.728 SOL only for routes that sell SOL: see
  [the open finding](#open-the-cap-counts-whatever-the-route-sells-in-that-mints-base-units).
