# Real-protocol tests, milestone 1: harness, snapshot and the Jupiter-Pyth-Jito templates

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** A LiteSVM suite that loads a committed, one-slot mainnet snapshot and runs the
live-protocol templates as real signed transactions against the real programs. This milestone
covers the harness, the snapshot tool, and the four templates that need only Jupiter, Pyth and
Jito:
- `jupiterOracleCheckedSwap`
- `tokenSweepIntoSwap`
- `pythFreshPriceGate`
- `jitoProfitGuardedTip`

Every problem the real programs expose gets fixed in its template, together with a test.

**Architecture:** see the spec, `docs/superpowers/specs/2026-09-26-real-protocol-tests-design.md`.
- `scripts/snapshot/` (plain Node `.mjs`, no dependencies) writes `tests/protocols/snapshot/`:
  - account state as JSON, committed;
  - program ELFs through Git LFS.
- `tests/protocols/` is its own Cargo workspace. It uses LiteSVM pinned by git rev together with
  the unchanged `ballista-sdk`.
- Tests may write only wallet balances, oracle price accounts and the clock. Every other state
  change goes through the protocols' own instructions.

**Research this plan depends on:** read the relevant sections before each task. All paths are
under `/private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/`.
- `protocol-research/litesvm-jupiter.md`:
  - LiteSVM API (A.1–A.12) and the recommended `Cargo.toml` (A.11);
  - the Jupiter API: how to force `route` (B.4), and the `dexes` and `maxAccounts` options (B.5);
  - snapshot mechanics (C.1–C.6);
  - working probe code in `litesvm-probe-git/` and working scripts in `jup/*.mjs`.
- `protocol-research/orca-pyth-jito.md`:
  - Pyth PriceUpdateV2 layout and freshness rules (§5);
  - Jito tip accounts (§6);
  - the finding "P1" (feed id never checked);
  - the Jito loss path (`measureProfit` underflow).

**Base:** branch `claude/protocol-tests`, worktree `.claude/worktrees/protocol-tests`, created from
`1d74c68` (cu/integrated plus the example fixes). Build Ballista with
`cargo build-sbf --manifest-path programs/ballista/Cargo.toml`.

**Conventions:**
- Commit after each task. End each message with a blank line and the `Co-Authored-By` line your
  harness gives you.
- Never use `git stash`.
- Never edit `package.json` or `pnpm-lock.yaml`; another session owns them. Run scripts with
  `node scripts/snapshot/snapshot.mjs`.
- The on-chain error codes of Ballista (6000+) collide with those of Orca, Kamino and others.
  Tests identify the failing program from the transaction logs, never from the code alone.

---

## File map

| Path | Responsibility |
| --- | --- |
| `.gitattributes` | `tests/protocols/snapshot/programs/*.so` → LFS |
| `Cargo.toml` (root) | Add `tests/protocols` to `exclude` |
| `scripts/snapshot/snapshot.mjs` | Read the manifests. Fetch programs, accounts, lookup tables and routes at one slot. Write the snapshot. Print a diff against the previous snapshot |
| `scripts/snapshot/rpc.mjs` | JSON-RPC with BigInt-safe parsing, `getMultipleAccounts` in 100-key chunks at one slot, ELF trimming |
| `scripts/snapshot/jupiter.mjs` | Quote plus swap-instructions calls (`useSharedAccounts: false`, `instructionVersion=V1`, pinned `dexes`, `maxAccounts`). Collect every account and table the route touches |
| `scripts/snapshot/manifests/milestone-1.json` | Programs, accounts and routes for this milestone |
| `tests/protocols/Cargo.toml`, `Cargo.lock` | The workspace (research A.11) |
| `tests/protocols/snapshot/{manifest.json,accounts.json,routes.json,programs/*.so}` | The committed snapshot |
| `tests/protocols/src/lib.rs` | Re-exports the modules below |
| `tests/protocols/src/snapshot.rs` | Load and verify the snapshot into LiteSVM (ProgramData before Program; mainnet Token over the bundled one); set the clock; warp |
| `tests/protocols/src/wallet.rs` | Funded keypairs; token accounts with balances (write rule 1) |
| `tests/protocols/src/oracle.rs` | Write a PriceUpdateV2's price, confidence, exponent and publish time (write rule 2) |
| `tests/protocols/src/tx.rs` | Build and sign legacy or v0 transactions with lookup tables; assert wire size ≤ 1,232; send; `Failure { program, code }` read from the logs |
| `tests/protocols/src/template.rs` | Upload a template payload with Ballista's real instructions; build a run by account and input names |
| `tests/protocols/tests/*.rs` | One file per template |
| `clients/js/src/protocol-examples.test.ts` | Record each template's account, input and group order next to its payload |
| `common/src/template/verify.rs` | Keep `every_shared_fixture_parses_and_verifies` reading the richer JSON |
| `.github/workflows/ci.yml` | A job for the suite, with an LFS cache |

---

### Task 1: Workspace skeleton and a Ballista smoke test in LiteSVM

- [ ] **Step 1: LFS and the workspace.**
  - Add `.gitattributes` with
    `tests/protocols/snapshot/programs/*.so filter=lfs diff=lfs merge=lfs -text`.
  - Add `"tests/protocols"` to the root `Cargo.toml` `[workspace] exclude`. Read it first; keep
    `tests/ballista` there.
  - Create `tests/protocols/Cargo.toml` exactly as research A.11 recommends: LiteSVM by git rev
    `6a550a1000138cf8a285fac98a17c88ba5b2ad5b`, `ballista-sdk` by path, and the listed solana
    crates. Keep `precompiles` off.
  - Add `tests/protocols/target/` to `.gitignore` if the root ignore doesn't already cover it.
- [ ] **Step 2: Write the failing smoke test,** `tests/protocols/tests/smoke.rs`. It:
  - creates a LiteSVM;
  - loads `../../target/deploy/ballista.so` at `ballista_sdk::ID` (check that the SDK exports the
    program ID under that name; `clients/rust/src/lib.rs` has it);
  - funds a creator;
  - uploads `fixtures/system-transfer.hex` with `ballista_sdk::create_template_instruction`,
    signed and sent as a real transaction;
  - runs it with `ballista_sdk::run_instruction`, moving 1,000,000 lamports from a signer to a
    recipient;
  - asserts both balances.
  Use `litesvm-probe-git/tests/e2e.rs` as the reference for the API calls.
- [ ] **Step 3:** `cargo build-sbf --manifest-path programs/ballista/Cargo.toml && cargo test --manifest-path tests/protocols/Cargo.toml --test smoke`.
  It fails first if the crate doesn't compile yet; fix until it passes.
- [ ] **Step 4:** Commit `Cargo.lock` too. Message: "Run a Ballista template in LiteSVM".

### Task 2: The snapshot tool

- [ ] **Step 1: `scripts/snapshot/rpc.mjs`.** Build it from the working code in the research
  scratchpad (`jup/*.mjs` and research C.5):
  - JSON-RPC over `fetch`, with the URL from `SOLANA_RPC_URL` or defaulting to the public
    endpoint;
  - a BigInt-safe parse of `rentEpoch`;
  - `getMultipleAccounts` in chunks of up to 100 keys, one request per call (the public RPC
    rejects batches). Include the Clock sysvar in every call. Retry until every chunk reports the
    same context slot;
  - lookup-table decoding (56-byte header, then 32-byte keys);
  - ELF extraction: ProgramData minus its 45-byte header, trimmed at the end of the
    section-header table, never by stripping trailing zeros (research C.3; its `tests/elf.rs` has
    the rule).
- [ ] **Step 2: `scripts/snapshot/jupiter.mjs`.** Call quote and then swap-instructions against
  `lite-api.jup.ag/swap/v1`, or `api.jup.ag` when `JUPITER_API_KEY` is set:
  - pass `useSharedAccounts: false`, `instructionVersion=V1`, ExactIn;
  - use the `dexes` allowlist `Whirlpool`, `Raydium CLMM`, `Meteora DLMM` (check the exact dex
    label strings via the API's `/program-id-to-label`) and `maxAccounts` 30;
  - assert that the instruction's first 8 bytes are `route`'s discriminator;
  - return the instruction, the quote, the lookup-table addresses, and every account the setup,
    swap and cleanup instructions reference.
- [ ] **Step 3: `scripts/snapshot/manifests/milestone-1.json`.**
  - **Programs:** Jupiter v6, the mainnet Token program (p-token), Associated Token, the Pyth
    receiver, and each AMM program the chosen routes touch (Whirlpool, Raydium CLMM, Meteora
    DLMM).
  - **Accounts:**
    - the Pyth SOL/USD shard-0 feed `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE`;
    - one Jito tip account (from `shared.ts`, owned by the Tip Payment program);
    - the USDC and wSOL mints.
  - **Routes**, each named:
    - `solToUsdc`: 1 SOL;
    - `usdcToSol`: 150 USDC;
    - `solToUsdcToSol`: a round trip, for the Jito template. It may need two quotes chained;
      document what you pick.
- [ ] **Step 4: `scripts/snapshot/snapshot.mjs`.** It:
  - reads a manifest and fetches programs and accounts, plus every account and lookup table the
    routes reference, all at one slot;
  - writes `manifest.json`: slot, unix time, and for each program and account its owner,
    lamports, executable flag, data length and sha256;
  - writes `accounts.json` (base64 data), `routes.json`, and `programs/<id>.so`;
  - prints a summary, and a diff when an earlier snapshot exists;
  - runs the research C.6 asserts: tables active and `last_extended_slot < slot`, and the route
    discriminator.
- [ ] **Step 5: Run it and commit.**

```bash
node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/milestone-1.json tests/protocols/snapshot
git lfs track   # confirm the .so files are LFS pointers
git add .gitattributes scripts/snapshot tests/protocols/snapshot
git commit -m "Snapshot mainnet programs, accounts and Jupiter routes for the protocol tests"
```

Check that `git show --stat HEAD` lists the `.so` files as LFS pointers (a few hundred bytes each).

### Task 3: The harness library

Write `tests/protocols/src/{snapshot,wallet,oracle,tx,template}.rs`, adapting
`litesvm-probe-git/tests/jupiter.rs` and research A.12:

- **`snapshot.rs`**
  - `Snapshot::load(dir) -> Snapshot`, which verifies every sha256 against `manifest.json`.
  - `snapshot.into_svm() -> LiteSVM`. It sets accounts sorted so ProgramData comes before
    Program. It skips builtins, sysvars and `Sysvar1nstructions`. It sets the mainnet Token
    program over LiteSVM's bundled one, loads `ballista.so`, and sets the clock to the snapshot's
    exact slot and unix time.
  - `warp(svm, slots, seconds)`, which only moves forward.
  - `lookup_tables() -> Vec<AddressLookupTableAccount>`.
- **`wallet.rs`**
  - `funded(svm, lamports) -> Keypair`. Write the balance with `set_account` (write rule 1)
    rather than `airdrop`. LiteSVM's blockhash never advances by itself, so a second identical
    airdrop is rejected as `AlreadyProcessed`. For the same reason, `tx::send` should call
    `expire_blockhash()` before each send, or tests must never send identical transactions.
  - `token_account(svm, owner, mint, amount) -> Address`. It creates or overwrites an ATA's state
    directly (write rule 1), in the real SPL Token layout and owned by the Token program. Also
    handle wSOL.
- **`oracle.rs`**
  - `set_pyth_price(svm, feed, price: i64, conf: u64, exponent: i32, publish_time: i64)`. It
    writes only those fields at the offsets confirmed in the research, and asserts that the
    verification level byte (40) is still 1 (`Full`).
- **`tx.rs`**
  - `send(svm, payer, signers, instructions, tables) -> Result<Outcome, Failure>`. It builds a v0
    message when tables are given and legacy otherwise, asserts that the serialized size is at
    most 1,232 bytes, and sends. `Outcome` carries the logs and compute units consumed.
  - `Failure { program: Address, code: Option<u32>, logs }`. `program` is the one whose
    `Program <id> failed` line appears **first**: that is the innermost program that failed.
    Every caller up the CPI stack then logs its own `failed` line repeating the same code, so the
    last line would blame Ballista for a System program error (measured in Task 1). Parse
    `meta.logs`, which is plain text, not `pretty_logs()`, which adds ANSI colours.
  - `ballista_error(&Failure) -> Option<(kind, context)>`, which decodes only when
    `program == ballista_sdk::ID`.
- **`template.rs`**
  - `upload(svm, creator, id, payload) -> Address`, using real create/write/finalize
    instructions (`ballista_sdk` has them; chunk when larger than one transaction).
  - `Run::new(template, &examples["name"]).account("owner", key, writable, signer)
    .input_u64("x", v).input_bytes(..).group("routeAccounts", metas).build() -> Instruction`.
    It orders by the names recorded in the fixture (Task 4) and panics on an unknown or missing
    name.

Unit-test what can be tested without a network: oracle writes round-trip; a run built by name
matches the hand-built one for `jupiterDepositExactOutput`'s current order. Commit:
"Add the protocol-test harness".

### Task 4: Record each template's order in the fixture

- [ ] **Step 1:** In `clients/js/src/protocol-examples.test.ts`, write each entry as
  `{ "payload": hex, "fixedAccounts": [...], "inputs": [...], "rowInputs": [...], "batchAccounts": [...], "accountGroups": [...] }`,
  taken from `compileTemplate(...)`'s `fixedAccountOrder`, `inputOrder`, `rowInputOrder`,
  `batchAccountOrder` and `accountGroupOrder`.
- [ ] **Step 2:** `common/src/template/verify.rs`'s `every_shared_fixture_parses_and_verifies`
  currently finds payloads by splitting the JSON on `"` and taking long strings. The new JSON
  still holds each payload as a long hex string and no other strings over 64 characters. Confirm
  that, and make the test's assertion (12 examples) still hold. If names could ever exceed 64
  characters, switch it to reading `"payload":` values explicitly.
- [ ] **Step 3:** `pnpm fixtures`, `pnpm --dir clients/js check`,
  `cargo test -p ballista-common every_shared_fixture_parses_and_verifies`. Commit:
  "Record the protocol examples' account and input order".

### Task 5: The oracle-checked swap, for real, and pinning the feed

- [ ] **Step 1: Failing tests,** in `tests/protocols/tests/oracle_checked_swap.rs`. Upload the
  template from the fixture and use route `solToUsdc`. Fund the trader with SOL and a wSOL
  account for the route's input (or include the route's setup instructions from `routes.json`).
  Pass the feed and the correct `priceExponent` and `scaleDivisor` for SOL (9 decimals) to USDC
  (6 decimals). Three runs:
  - **pass:** `toleranceBps` 100. The fill clears the floor.
  - **fail:** the oracle price is written 5% above market (write rule 2). Expect the Ballista
    failure at label `fillBeatTheOracle`; decode the program counter against the source map, or
    compare to the label's pc from the compiled fixture's source map.
  - **fail (P1):** a different Full price account from the snapshot, such as USDC/USD, if it is
    added to the manifest. It must be rejected.
- [ ] **Step 2: Fix P1 in both Pyth templates.**
  - In `jupiter-oracle-checked-swap.ts` and `pyth-fresh-price-gate.ts`, require that the feed id
    (`PriceUpdateV2.price_message.feed_id`, 32 bytes at the offset the research gives) equals a
    `feedId` input: `expression.accountData(priceUpdate, FEED_ID_OFFSET, 'pubkey')` compared with
    `expression.input('feedId')`, as a pubkey.
  - Add `feedId` to `PYTH` in `shared.ts`.
  - Update `protocol-semantics.test.ts`: both templates require the feed id to equal the input.
  - Regenerate the fixtures and re-run the real tests.
- [ ] **Step 3:** All three runs behave as described. Commit: "Run the oracle-checked swap
  against the real programs and pin its price feed".

### Task 6: The token sweep, for real

- [ ] Failing tests first, then any template fix they force. Route `usdcToSol`.
  - **pass:** a wallet holding a USDC balance different from the quoted amount (for example the
    quote plus 3%). Pass the quote's route plan and numbers. After the run, the source holds at
    most `dustFloor` and the proceeds meet the rescaled quote.
  - **fail:** a balance at `dustFloor` fails at `worthSelling`.
  - Also record the compute units used and the transaction size.
  Commit: "Run the token sweep against the real programs".

### Task 7: The Pyth price gate, for real

- [ ] Route `solToUsdc` as the action.
  - **pass:** fresh and in band.
  - **fail:** clock warped past `maximumAge`, failing at `priceIsFresh`.
  - **fail:** a band that excludes the price, failing at `priceAboveFloor` or
    `priceBelowCeiling`.
  - **fail:** the wrong feed id (after Task 5's fix).
  Commit: "Run the Pyth price gate against the real programs".

### Task 8: The Jito tip, for real

- [ ] Route `solToUsdcToSol` as the strategy. First find out, by running it, what the searcher's
  lamports do during a Jupiter `route` that starts and ends in SOL. Wrapping happens outside
  `route`, so the lamport delta may always be about 0 or negative.
  - If the template's lamport-based profit cannot work with a Jupiter route: change it to measure
    profit on the wSOL (or quote-token) account the route pays into, with a snapshot and a
    subtraction.
  - Also make a loss fail at the requirement rather than underflow at `measureProfit`: compare
    `after >= before + tip + edge` instead of subtracting.
  - Update the header, the semantics tests and the fixtures.
- [ ] Runs:
  - **pass:** a tip at or under the realised profit, with the tip actually transferred to the tip
    account. Profit comes from write rule 1 only if the route can't produce it; document which.
  - **fail:** a tip above the profit fails at `profitCoversTheTip`.
- [ ] Commit: "Run the Jito tip against the real programs".

### Task 9: CI

- [ ] Add a job to `.github/workflows/ci.yml`. It:
  - checks out with `lfs: true`;
  - caches `.git/lfs` with `actions/cache`, keyed on `hashFiles('tests/protocols/snapshot/manifest.json')`;
  - installs Agave the same way as the existing SBF job;
  - runs `cargo build-sbf --manifest-path programs/ballista/Cargo.toml` and then
    `cargo test --manifest-path tests/protocols/Cargo.toml`.
  Commit: "Run the protocol tests in CI".

### Task 10: Findings report

- [ ] Write `tests/protocols/FINDINGS.md`. For each template, record whether it passed as
  written, what was fixed (with commits), what remains impossible and why, and the compute units
  and transaction size of each passing run. Commit.
