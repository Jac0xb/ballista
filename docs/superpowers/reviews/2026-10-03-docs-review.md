# Docs review: nine reader types

2026-10-03 · `main` at `d831949` · line numbers refer to that commit

Nine reviewers read the docs as different readers, each working the way that reader would.

**Status:** all nine have reported: newcomer, founder, technical editor, DeFi searcher, advanced author, Rust developer, protocol engineer, security auditor, and TypeScript integrator.

**Verification, 2026-10-02:** each entry below has a status line, checked against the merged fixes at `04585e9`, then updated after the follow-up fixes.
- Fixed: 59.
- Changed approach: 2, the Jito join helper and the second signer.
- Partly or not fixed: 0.
- The verification also found four problems the fixes had caused, all since fixed:
  - Getting started installed packages that aren't published;
  - the devnet upgrade-authority statement appeared five times;
  - the privilege-ceiling gaps were restated on three pages;
  - the nested-run tabs didn't follow the reader's language.

**Already fixed on `claude/docs-cleanup`, so not repeated here:**
- the funds and state wording;
- the `unsafe` list;
- the Rust parameter names and the `rateLimit` note;
- the examples-index language claim;
- the fee-cap wording;
- refreshed protocol measurements;
- most of the missing TypeScript exports.

**How to read the entries:**
- **Severity:** **B** blocker, **M** major, **m** minor.
- **Readers:** N newcomer, F founder, E editor, S searcher, A advanced author, R Rust developer, P protocol engineer, Au security auditor, T TypeScript integrator.

## Fix in code, not only docs

- **B · Registry entries can alias.** (A, Au)
  - **Status:** Fixed. A second open of an open entry fails with `InvalidRegistryEntry` (`programs/ballista/src/processor/registry.rs`), tested by `a_second_open_of_an_open_entry_fails` in `tests/ballista/src/lib.rs`.
  - When two entries of one registry have equal keys, they are the same account, and the second open is accepted (`registry.rs:72`).
  - A read-both-then-write-both transfer then credits the sender.
  - Fix: refuse to open an entry that is already open, with a test.
- **B · The Rust SDK can't be installed as documented.** (R, N)
  - **Status:** Fixed. `clients/rust/Cargo.toml` gives `ballista-common` a version. Neither package is published yet (crates.io has no such crate; npm returns 404), so `docs/reference/rust.md` and Getting started install the crate from git, and Getting started packs the TypeScript SDK from the clone.
  - `cargo add ballista-sdk` can't work: `cargo package -p ballista-sdk` fails because `ballista-common` is a path dependency with no version (`clients/rust/Cargo.toml:19`).
  - The npm package wasn't checked; the reviewers worked offline.
  - Fix: say what's published. Until then, give a git install line, and add the version before publishing.
- **B · The Jito round trip has no TypeScript helper.** (S)
  - **Status:** Changed approach. `joinRoundTrip` is a tested example helper in `clients/js/examples/protocols/shared.ts`, not an SDK export; `docs/examples/protocols/jito-tip.md` documents it, which meets the reader's need.
  - The only code that joins two Jupiter quotes is a Rust test (`round_trip`).
  - Fix: ship and document a join helper.
- **M · Simulated errors don't decode.** (T)
  - **Status:** Fixed. `decodeBallistaError` takes `number | bigint` (`clients/js/src/errors.ts`), and `clients/js/examples/sdk/simulate-run.ts` simulates a run on errors-and-events.
  - Kit returns `Custom` as a `bigint` at runtime, so `decodeBallistaError` returns `undefined` (`errors.ts:95`).
  - Fix: accept `bigint`, and add a simulate example.
- **M · Error decoding blames Ballista for other programs' errors.** (T)
  - **Status:** Fixed. `failedProgram` and `decodeBallistaFailure` are in `clients/js/src/errors.ts`, `explainRunError` takes `{ logs }`, and `run-jupiter-deposit.ts` uses them.
  - Anchor programs also number their errors from 6000.
  - So `describeFailure(6001)`, which is Jupiter's slippage error, prints "InvalidTemplateAccount at inputs.routePlan" (`run-jupiter-deposit.ts:147`).
  - Fix: move `failedProgram(logs)` (`run-orca-harvest.ts:115`) into the SDK, and decode by program.
- **M · The oracle swap trusts builder inputs.** (Au)
  - **Status:** Fixed. `FEED_ID` and `TOLERANCE_BPS` are constants in `jupiter-oracle-checked-swap.ts` and its Rust twin; `jupiter-oracle-swap.md` lists confidence and the price choice under "does not guard against".
  - `feedId` and `toleranceBps` are run inputs (`jupiter-oracle-checked-swap.ts:74,86`), so a hostile builder can name another feed or a wide tolerance.
  - There is no confidence check, and whoever posts the pull update can pick any price from the last 60 seconds.
  - Fix: make them template constants, as the fee cap is, or list them under "does not guard against".
- **M · The Rust instruction builders hardcode the program ID** (`clients/rust/src/lib.rs:179-266`). (R)
  - **Status:** Fixed. Every builder and address function has a `_for_program` variant (`clients/rust/src/lib.rs`), named in `rust.md`.
  - The docs imply other deployments are possible.
  - Fix: add `_for_program` variants.
- **M · Templates run out of registers.** (A, P)
  - **Status:** Fixed. `reuseRegisters` in `clients/js/src/compiler.ts`; a signed quote with `rateLimit` runs in `tests/ballista/src/tests/register_reuse.rs`; the budget is on `limits.md#registers`.
  - The compiler never reuses a register (`compiler.ts:1254`).
  - The oracle swap uses all 64, and a signed quote plus `rateLimit` fails.
  - Fix: register reuse. Until then, document the budget.
- **M · Nothing decodes events or return data.** (T, A)
  - **Status:** Fixed. `decodeRunEvent` and `parseProgramData` (`clients/js/src/events.ts`), `decode_run_event` and `program_data` (`clients/rust/src/logs.rs`), and a return-data example; `errors-and-events.md` now says a `Program data:` line names no program.
  - Neither SDK decodes run events or `emit` output.
  - `Program data:` lines don't name their program, so a decoder has to track the invoke stack. `errors-and-events.md:197` says otherwise.
  - Fix: add `decodeRunEvent` and a return-data example.
- **m · A second `rateLimit` in one template fails** with "Variable already defined". (A)
  - **Status:** Fixed. Its variables now take the registry's name (`clients/js/src/helpers.ts`), so two limits fit; `typescript.md` and `registries.md` describe `name`.
  - Its `name` option is documented only at `typescript.md:506`.
- **M · Calling your own program needs helpers that aren't exported.** (N, S, T)
  - **Status:** Fixed. `addressBytes` and `anchorDiscriminator` are SDK exports (`clients/js/src/helpers.ts`), Rust has `anchor_discriminator`, and `getting-started.md` has step 6, "Call your own program".
  - The guide says to swap in your own program (`runtime-values:15`, `loops:22`, `guardrails:11`), but nothing shows how to turn a Kit address into the 32 bytes a template needs, or how to build an Anchor discriminator.
  - The natural `getAddressEncoder().encode()` fails `tsc` (TS2739).
  - `addressBytes`, `anchorDiscriminator`, `splitJupiterRoute`, `at` and `pinned` exist only in `examples/protocols/shared.ts` and the runners. On the Rust side, so do `RouteQuote::split` and `kamino_refreshes`.
  - Fix: export the address and discriminator helpers from the SDK, and add a "Call your own program" step to Getting started.
- **m · A source comment contradicts registries.** (A)
  - **Status:** Fixed. `signed-quote-settlement.ts` now says this template keeps no state, and that a registry entry could count settlements.
  - `signed-quote-settlement.ts:20-21` says Ballista keeps no state.

## Raised by several readers

- **B · Status and maturity are buried.** (N, F, E, S, P, Au)
  - **Status:** Fixed. Status line on the cover (`FieldManual.vue`), why-ballista and getting-started; immutability in the future tense with the devnet key named as trusted; footer `SDK v1.0 · PROGRAM PRE-RELEASE` (`docs/.vitepress/config.mts`).
  - The current program isn't deployed and isn't audited. That appears only at `devnet.md:22`, `security.md:7-9` and `trust-model.md:93-95`, while the front page and the "v1.0" footer (`config.mts:224`) read as production-ready.
  - `trust-model.md:86` and `scope.md:50` claim "no upgrade authority". But the devnet build has one, and both SDKs default to its address (`kit.ts:214`, Rust `lib.rs:22`).
  - That default, `BALLISTA_ADDRESS`, is the pre-release build, and it rejects current templates (6002).
  - "What you reviewed is what runs" ignores called programs, which can be upgraded (`security.md:73`, `inspecting-templates.md:3`).
  - Fix:
    - add a status line under the hero, and on why-ballista and getting-started;
    - put the immutability claims in the future tense, and name that upgrade authority as trusted;
    - change the footer to "SDK v1.0 · program pre-release".
- **M · Cost is never stated.** (F, S, P)
  - **Status:** Fixed. `why-ballista.md#cost`, with run figures from `fixtures/cu-ceilings.json`, and a Cost line on all 13 protocol pages.
  - No page says that Ballista charges no fee, what upload and entry rent costs, or what the run overhead is. Measured: empty run 595 CU, one transfer 2,403, 30-row payroll 46,713, and about 3,400 per Jupiter pass.
  - Fix: add a "What it costs" section to why-ballista, and a Cost line under each protocol page's Status.
- **M · The boundary contradicts itself.** (F, P)
  - **Status:** Fixed. `why-ballista.md#boundary` lists custody, PDA signing, protocol-owned state, about 61 accounts, deep CPI routes and compute-critical work, and agrees with scope and language.
  - `why-ballista.md:46-49` says permission rules and replay prevention need a custom program. But line 56, `scope.md:29` and `language.md:636` say registries handle them.
  - Fix: list the real reasons to leave: custody, PDA signing, private state, more than about 61 accounts, deep CPI routes, and compute-critical paths.
- **M · Each protocol example has three names.** (F, E, S)
  - **Status:** Fixed. Sidebar, table and page title share one name (`config.mts`, `examples/protocols/index.md`); `examples/index.md` links to the one table.
  - The sidebar, the table and the page title each name it differently.
  - The 13-row table appears twice, in different orders (`examples/index.md:31-45`, `protocols/index.md:14-28`).
  - Fix: one name and one table.
- **M · Rules repeat across pages.** (N, E, A, R, P, F, Au, T)
  - **Status:** Fixed. Finalization checks, output and tag rules, registry rules, the step semantics, the sweep, signing, the privilege ceiling, the devnet upgrade authority and the PDA and compute-unit definitions each live once and are linked. `typescript.md#authoring` keeps only the API index and links to `language.md#steps`.
  - Registry rules appear on 5 pages, 3 times in `registries.md` alone.
  - Output rules appear 5 times, the `emit` tag rule on 6 pages, and the introspection table 4 times.
  - The finalization checks appear 4 times: `why-ballista:73`, `mental-model:109`, `trust-model:51` and `security:15`.
  - The step table appears 3 times, and the protocol list 3 times, with different titles each time.
  - Loops and batching teach the same things.
  - "Never signs" appears about 12 times.
  - PDA and compute units are each defined on 9–13 pages.
  - The sweep template appears three times: index, getting-started and runtime-values.
  - Fix: keep each rule once, in language.md, the trust model or the glossary, and link to it.
- **M · Embedded examples can't be copied as-is.** (E, T)
  - **Status:** Fixed. Examples import `@jac0xb/ballista`, with no `../../src` imports left, mapped by `paths` in `clients/js/tsconfig.json`; the protocols index links `shared.ts` and shows the run helpers.
  - About 100 import lines read `../../src/index.js`.
  - Some regions use helpers that exist only in the repo.
  - Fix: map the package names with tsconfig `paths`, and show the helpers or name where to get them.
- **M · The testing claims overstate.** (F)
  - **Status:** Fixed. Each protocol page and `examples/protocols/index.md` say tested locally in LiteSVM, not yet run on devnet or mainnet.
  - "Run as real transactions… copied from mainnet" reads as on chain.
  - Fix: "Tested locally against copies of the mainnet programs; not yet run on devnet or mainnet."

## Limits page

- **M · The account limit is wrong.** (P)
  - **Status:** Fixed. `limits.md#accounts-per-transaction` gives 64 accounts in any version, about 61 runtime accounts, and `account-groups.md` redoes the budget.
  - A v0 or v1 transaction can't go past 64 accounts even with lookup tables. Their keys count toward the lock limit, and `increase_tx_account_lock_limit` isn't active.
  - Fix: say about 61 runtime accounts on every version, and redo the 85-account budget (`limits.md:117`, `account-groups.md:78-83`).
- **M · Call depth isn't documented.** (P)
  - **Status:** Fixed. `limits.md#call-depth`, and "A call nests too deep" with `CallDepth` in `failure-modes.md`.
  - Run is frame 1, so nested runs go deeper. The nested scenario measured exactly 5 frames, with no headroom.
  - Fix: add a Call depth section, and add `CallDepth` to failure-modes.
- **M · The register budget isn't explained.** (A, P)
  - **Status:** Fixed. `limits.md#registers` covers what takes a register, loops, `let` and `stats.registers`; the 128-steps row is gone.
  - Fix: say what takes a register, that loops don't multiply them, to use `let` for repeated reads, and to check `stats.registers`.
  - Drop the "128 top-level steps" row.
- **m · The trace arithmetic is off.** (A, P)
  - **Status:** Fixed. `limits.md#instruction-trace` has 15 + 7 × 7 = 64 and at most 63 CPIs; `batching.md` links it and `failure-modes.md` covers the overflow.
  - 15 + 7×7 = 64 should fit; the omitted payout row is what makes 65.
  - The run itself is one trace entry, so 64 CPIs never fit (`limits.md:129-133`).
  - Fix: link the trace limit from batching (`batching.md:120-127`), and add trace overflow to failure-modes.

## Getting started, lifecycle, transactions

- **M · Getting started runs steps 1–4 twice,** on the Template tab and again on the Run tab (`getting-started.md:8-11`). (E, N)
  - **Status:** Fixed. `getting-started.md` is one numbered sequence with only TypeScript and Rust tabs, and `codeLang.ts` carries the choice across blocks.
  - The Template/Run choice doesn't carry across code blocks (`codeLang.ts`), and each block opens on Template. So step 3's run code sits behind a second tab, and step 1's "Template" tab is only wallet setup.
  - Fix: make the page one sequence of numbered steps, Connect, Define, Upload, Run, Read a failure, with only a language toggle.
- **M · Uploading any template over about 1 KB fails as documented.** (R, P, N)
  - **Status:** Fixed. The TypeScript upload passes `transactionMessage` (`examples/start/getting-started.ts`), Rust writes 1,000-byte chunks (`docs_start.rs`), and `limits.md#transaction-size` gives both limits.
  - In TypeScript, without a `transactionMessage` the upload plan sends the whole template in one instruction of up to 3,500 bytes (`instructions.ts:120`). But Getting started's `send` builds v0 transactions, which cap at 1,232 bytes.
  - So uploading the daily cap, the oracle swap and the signed quote (1,288–1,794 bytes) fails.
  - In Rust, the 3,000-byte chunks overflow the legacy transaction Getting started builds (measured: 3,209 bytes). A legacy transaction allows 1,023-byte chunks and a 960-byte one-shot payload (`template-lifecycle.md:16`, `:81`, `rust.md:349`).
  - Fix: pass the v0 message to the plan, use chunk sizes that fit the transaction the page builds, and state both limits.
- **M · The Rust snippet at `transaction-v1.md:43` doesn't compile.** (R, P)
  - **Status:** Fixed. The page embeds the compiled `docs_start.rs#v1` region, which signs and sends, and Getting started's install line adds `solana-message@4`.
  - It passes an extra `&[]` to `try_compile_with_config`, and it never signs or sends.
  - Fix: also add `solana-message@4` to the install line.
- **m · The shipped examples need `pnpm install` in the clone first,** and `getting-started:426-437` doesn't say so. (N)
  - **Status:** Fixed. "Run the shipped examples" in `getting-started.md` says to run `pnpm install` first.
- **m · `mental-model:81` says a loop runs "never more than max"**, but a count above `max` fails the run with `LoopCountExceeded`; it isn't clamped. (N)
  - **Status:** Fixed. `mental-model.md` now says a count above `max` fails with `LoopCountExceeded`.
- **m · Sidebar and front page:** (E, F)
  - **Status:** Fixed. `config.mts` has an Upload and run group, inspecting under Security, transaction-v1 listed, and Guide opening on why-ballista; `docs/index.md` no longer repeats the gallery's example.
  - the Deploy group holds only devnet;
  - inspecting-templates belongs under Security;
  - transaction-v1 is missing from the Guide sidebar;
  - the Guide nav skips Why Ballista;
  - the front page repeats the gallery's example.

## Security and trust

- **M · The trust model names one adversary.** (Au)
  - **Status:** Fixed. `trust-model.md#who-controls-what` adds the builder, other instructions, upgrade authorities and replay.
  - `trust-model.md:9-23` lists the caller, but not the transaction builder, the other instructions in the transaction, the called programs' upgrade authorities, or replay.
  - The protocol pages, meanwhile, assume a hostile builder (`guardrails.md:37`, `signed-quote.md:83`).
  - Fix: add those rows.
- **M · An owner pin is not a type pin.** (Au)
  - **Status:** Fixed. `trust-model.md#pins` gives the multisig case and asks for type and identity checks; `guardrails.md` dropped "look-alike" and "fake oracle".
  - `trust-model.md:40-41`, `guardrails.md:61` ("look-alike account") and `:68` ("fake oracle") claim otherwise.
  - A 355-byte Token multisig passes `minDataLength: 165`, and Anchor account types differ only by discriminator.
  - `guardrails.md:116` contradicts both claims.
  - Fix: check the discriminator or the exact length, plus the account's identity.
- **M · Two pages show a caller-controlled override as a pattern.** (E)
  - **Status:** Fixed. `emergencyOverride` is gone from `assertions.md`, and `language.md` now says never to `or` a check with an input.
  - `language.md:340-341` and `assertions.md:79-88` (`emergencyOverride`) let the caller skip a check.
  - That contradicts `registries.md:95-99` and `guardrails.md:31-32`.
  - Fix: remove it.
- **M · Accounts can alias.** (P, Au)
  - **Status:** Fixed. `trust-model.md#aliased-accounts`, and exact lamport delta requires `notEqual` in both languages (`examples/docs/exact-lamport-delta.ts`).
  - A run accepts the same account in two slots, checking each one on its own (`execute.rs:478-545`, `:524`), so before-and-after checks can be fooled.
  - Fix: state it, and show `require(notEqual(accountKey(a), accountKey(b)))`.
- **m · Declared flags don't bound everything.** (Au)
  - **Status:** Fixed. `trust-model.md#privileges` names both gaps, and `inspecting-templates.md` adds "CPI data built from inputs".
  - Group members take the transaction's writable flag (`execute.rs:1585`).
  - A declared signer can be passed to every CPI (`inspecting-templates.md:82`, `accounts-and-cpis.md:17`).
  - Fix: add "check CPI data built from inputs".
- **m · `currentInstructionIndex` returns the top-level index,** so under a CPI it names the outer instruction. This matters for signed quotes. (P)
  - **Status:** Fixed. `language.md#introspection` and `signed-quote.md` say it is the top-level instruction's index.
- **m · An `emit` tag doesn't identify the template.** (A)
  - **Status:** Fixed. `language.md#output` says the tag doesn't identify the template and that logs past 10,000 bytes are cut.
  - Any template can log the same tag.
  - Many emits can pass Solana's 10,000-byte log limit and be truncated.
- **m · why-ballista says finalization "proves"** (line 76), while formal-verification says some rules aren't proved. (F)
  - **Status:** Fixed. `why-ballista.md` now says checks, and links to the trust model's list.
  - Fix: say "checks".
- **m · The formal-verification counts are stale.** (Au)
  - **Status:** Fixed. `formal-verification.md` says 15 set up, 14 blocked plus 3 diagnostic, as in `certora/ballista-specs/*.conf`, and names `multiplyDivide`.
  - There are 15 rules set up and 14 blocked, plus 3 diagnostic, not 14 and 13.
  - The blocked `multiplyDivide` rule isn't mentioned.

## Protocol pages

- **M · Getting a Jupiter route is incompletely explained.** (S)
  - **Status:** Fixed. `examples/protocols/index.md#jupiter-routes` covers the quote parameters, `/swap-instructions`, where to slice, setup, tables and compute.
  - Besides `useSharedAccounts: false`, the repo's fetcher pins `instructionVersion=V1` and `swapMode=ExactIn`.
  - The pages don't say that `/swap-instructions` gives the route data.
  - They don't say where to slice the accounts: 4, or 3 for the daily cap and 2 for the price gate.
  - They don't say to keep the lookup tables and setup instructions, or to set your own compute limit.
  - Fix: one "Getting a Jupiter route" section.
- **M · The Pyth gate's guarantees are incomplete.** (S)
  - **Status:** Fixed. `pyth-gate.md` adds "does not guard against" and which update to pass, and both Pyth pages give the full feed ID.
  - It has no "does not guard against" list. The token accounts are in the group, so nothing checks the fill or the recipient.
  - It doesn't say which price update to pass.
  - Feed IDs are truncated, also at `jupiter-oracle-swap.md:73`.
- **M · A delegate approval hands over the Orca position.** (Au)
  - **Status:** Fixed. Warnings in `orca-harvest.md` and `orca-compound.md` say an approval hands over the position.
  - `orca-harvest.md:69-72` and `orca-compound.md:78-82` imply a keeper's harvest still pays the holder.
  - But a delegate can call `collect_fees` directly, or move the NFT under the SPL approval.
  - Fix: warn that the approval hands over the position.
- **M · The Kamino refresh helpers aren't shown.** (S)
  - **Status:** Fixed. `examples/protocols/index.md#kamino-refreshes` embeds both helpers, and `marginfi-withdraw.md` no longer contradicts itself.
  - The Scope and reserve order appear only in `run/kamino.ts`.
  - `marginfi-withdraw.md:52` and `:77` disagree on what must go first.
  - Fix: embed the helpers.
- **M · The liquidation's inputs aren't defined.** (S)
  - **Status:** Fixed. `kamino-liquidate.md` has a "You supply three inputs" list.
  - `minimumBounty` is gross collateral, not profit.
  - `liquidityAmount` and `minAcceptableReceived` are undefined.
  - Fix: add a "You supply" list (`kamino-liquidate.md:19-30`).
- **M · The daily cap's wording overstates it.** (A, Au)
  - **Status:** Fixed. `daily-cap.md` says 1.728 SOL at once, refilling daily, and about 3.456 SOL in any 24 hours.
  - "1.728 SOL a day" ignores the refill: about 3.456 SOL can move in any 24 hours.
  - Fix: "1.728 SOL at once, refilling daily" (`daily-cap.md:10`).
- **M · Protocol pages embed whole files.** (E)
  - **Status:** Fixed. Every protocol page embeds a `#template` region, with at most 6 header lines left in the files.
  - Their 19–59-line header comments repeat "What it does".
  - Fix: embed a region instead.
- **m · The Rust tabs call helpers the page doesn't show:** `program`, `anchor` and `READ`. (R)
  - **Status:** Fixed. `examples/protocols/index.md#rust-helpers` shows `program`, `anchor` and `READ`, and each page links there.
  - Fix: show the helpers region once, on the protocols index.
- **m · Repetition and jargon:** (S, E)
  - **Status:** Fixed. The `useSharedAccounts` tip and the two test bullets now live only in `examples/protocols/index.md`, and "accounts of 88" is gone.
  - the `useSharedAccounts` tip appears on 7 pages;
  - two bullets repeat on all 13 pages;
  - "accounts of 88".

## TypeScript SDK and dApp integration

- **M · A second signer isn't covered.** (T)
  - **Status:** Changed approach. `KitAccountBinding` now takes a `signer` that the instruction carries (`clients/js/src/kit.ts`, `examples/sdk/second-signer.ts`), rather than `addSignersToTransactionMessage`; it resolves the missing-signature failure.
  - `KitAccountBinding` takes no `TransactionSigner`, so when the fee payer isn't the template's signer, signing fails with "Transaction is missing signatures".
  - Fix: show `addSignersToTransactionMessage`.
- **M · Input value types aren't documented.** (T)
  - **Status:** Fixed. `typescript.md#run-construction` has the four-row table, which matches `encodeInput` in `instructions.ts`.
  - `typescript.md:382-391` doesn't map input types to JS values. For example, a `pubkey` input needs 32 bytes, not a Kit `Address`; only `registries.md:371` shows this.
  - Fix: add a 4-row table.
- **M · No page has lookup-table code.** (T)
  - **Status:** Fixed. `examples/sdk/lookup-tables.ts` uses `compressTransactionMessageUsingAddressLookupTables`, embedded in `typescript.md`.
  - `account-groups.md:76-84` and `limits.md:117` say large runs need a lookup table.
  - Fix: show Kit's `compressTransactionMessageUsingAddressLookupTables`.
- **M · The lifecycle examples don't work as written** (`template-lifecycle.md:20-90`). (T, N)
  - **Status:** Fixed. `template-lifecycle.md` embeds `examples/start/template-lifecycle.ts`, which reuses Getting started's helpers and `buildKitResumeTemplateUploadPlan`.
  - Line 25 sends a data-only `{kind, data}`, a type error against Kit's `Instruction`.
  - `send(plan.instructions[0])` sends an instruction with no accounts.
  - `baseV1Message` is never defined.
  - Resuming uses the non-Kit function; line 69 should use `buildKitResumeTemplateUploadPlan`.
  - Fix: reuse Getting started's helpers and the Kit plan functions.
- **m · The Kit reference is incomplete.** (T)
  - **Status:** Fixed. `typescript.md` lists the missing exports and marks async calls, and the decode sample is a tested file with `code` and `?.message`.
  - Missing: `BALLISTA_ADDRESS`, `SYSTEM_PROGRAM_ADDRESS`, `KitAccountBinding`, `TEMPLATE_STATE_FINALIZED` (Getting started compares `state === 1`) and `measureInstructionInTransaction`.
  - It doesn't mark which calls are async.
  - `errors-and-events.md:126` needs `?.message`, and its sample output omits `code`.
  - Some of these are already added on `claude/docs-cleanup`; check before fixing.
- **m · No "Run from a dApp" section.** (T) Add a short one covering signing, lookup tables, simulation, and events.
  - **Status:** Fixed. `typescript.md#run-from-a-dapp` covers signers, lookup tables, simulation and events.

## Registries, output and composition

- **M · No page shows how to nest a run.** (A)
  - **Status:** Fixed. "Run another template" in `examples/composition.md`, in both languages.
  - Nothing shows a template calling Ballista's Run and reading its return data. Only `scenarios/nested-split-sell.ts` does.
  - Fix: add a composition example.
- **m · `conditional.md` leaves rules out.** (A)
  - **Status:** Fixed. "Only calls take `when`" in `conditional.md` covers all four.
  - `when` works only on `invoke`.
  - Registry writes need a `select` write-back.
  - `emit` can't be skipped.
  - Return data can't follow a guarded call.
- **m · The error codes live in a guide page:** 60 rows at `errors-and-events.md:39-111`. (E)
  - **Status:** Fixed. The codes moved to `docs/reference/errors.md`, in the Reference sidebar.
  - Fix: move them to Reference.

## Rust reference

- **M · The `verify` failure table is missing three causes** (`rust.md:77`). (R)
  - **Status:** Fixed. `rust.md#checking-a-template` lists the three causes, with codes that match `errors.ts`.
  - A read needs `min_data_len` of at least offset + width.
  - A `bytes` CPI segment needs `set_cpi_max_data_len`.
  - `create_pda` uses the bump you supply.
- **m · "Read the context yourself" in Getting started.** (R)
  - **Status:** Fixed. Step 5 of `getting-started.md` gives `ProgramView::parse(&payload)?.instructions[n]`.
  - Declarations return `()`.
  - Fix: say that program counter n is `ProgramView::parse(..)?.instructions[n]`.
- **m · Run events are covered for TypeScript only** (`errors-and-events.md:171`). (R)
  - **Status:** Fixed. `errors-and-events.md#run-events` names `builder.flags(PROGRAM_FLAG_EMIT_EVENT)` and `decode_run_event`.
  - Fix: mention `builder.flags(PROGRAM_FLAG_EMIT_EVENT)`.
- **m · No test compiles the 33 inline Rust blocks.** (R)
  - **Status:** Fixed. No inline Rust blocks are left in the docs; the Rust is embedded from `clients/rust/examples/`.
  - That's how the transaction-v1 snippet went stale.
  - Fix: move them into `docs_*.rs` regions.
- **m · `rust.md` runs to 431 lines.** (R) It repeats language.md's registry rules and other tables.
  - **Status:** Fixed. `rust.md` is 315 lines and links to language.md's registry rules.

## What worked

- **Accuracy.**
  - Opcodes, errors and limits match the code exactly.
  - All 212 code embeds and every internal link resolve.
  - Getting started's TypeScript and Rust payloads come out byte-identical.
  - Five execution-model claims checked against the code held.
  - The strongest security claims hold:
    - template CPIs carry no seeds;
    - the privilege ceiling is enforced;
    - the registry header check and borrow mark work;
    - return data is checked against its program.
- **Getting started works.**
  - The Rust tabs compile and run end to end.
  - The TypeScript type-checks, and its printed error reproduces exactly.
  - The embedded `docs_*.rs` regions are tested byte for byte.
- **Protocol pages.**
  - They share one layout.
  - Each names its checks, and each check is backed by an adversarial test.
  - Their "Not tested" lists are honest.
- **`buildKitRunInstruction`** takes accounts, rows and groups by name, with precise errors.
- **Teaching:** error decoding is shown with real codes and step names, and `failure-modes.md` is the scannable shape the owner wants, in short "What happens / Recover" pairs.
- **The gallery diagrams** sell the idea without any code.
