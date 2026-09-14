# Ballista 0.3 implementation plan

This plan tracks the clean-break bounded orchestration VM. The 25 workflows in
[`usecases.md`](../usecases.md) are acceptance scenarios for the language surface, subject to the
external programs, accounts, and authorities listed there.

## 1. Flat program and verifier

- [x] Define an alignment-free canonical format for schemas, instructions, CPI descriptors,
  account metas, data segments, pubkeys, and literals.
- [x] Parse fixed tables as borrowed Zerocopy slices with no owned AST.
- [x] Verify section boundaries, opcodes, register types/initialization, account indices, loop shape,
  privileges, CPI expansion, and generated data bounds at finalization.
- [x] Share an exact representative byte fixture between Rust and TypeScript.

## 2. Immutable template lifecycle

- [x] Derive `template-v2` PDAs from creator and template ID.
- [x] Implement one-shot create plus begin, sequential chunk write, finalize, and upload cancel.
- [x] Store creator, ID, state, lengths, bump, and SHA-256 hash in an 80-byte v2 header.
- [x] Make finalized templates public, repeatable, immutable, and non-closeable.

## 3. Bounded execution

- [x] Implement typed inputs, constants, account/clock reads, checked math/casts, comparisons,
  booleans, `min`, `max`, `select`, and `require`.
- [x] Add lexical snapshot bindings for readable pre/post-CPI invariants and canonical PDA/ATA
  relationship assertions with statically bounded seed tables.
- [x] Implement guarded protocol-neutral CPI with literal and register-encoded data segments.
- [x] Enforce that CPI privileges are a subset of the outer transaction privileges.
- [x] Implement one top-level bounded tail-account iterator with inferred row count and no nesting.
- [x] Keep execution stateless: no Ballista custody, PDA signing, scheduler, replay policy, or cache.

## 4. Developer APIs

- [x] Replace `defineTask` with Zod-backed, named `defineTemplate` authoring.
- [x] Add deterministic compilation, hashing, stats, account decoding, inspection, run binding, and
  upload/resume planning.
- [x] Keep System, legacy Token, and ATA helpers in the SDK as generic-CPI compilers.
- [x] Add a Solana Kit adapter for PDA derivation, native instructions, exact message sizing, and
  v1-aware upload chunk planning.
- [x] Apply v1 compute and loaded-account-data limits from one simulation, including 32 KiB data
  page headroom.
- [x] Replace generated Rust clients with small lifecycle/run codecs and shared account parsing.

## 5. Verification and release cleanup

- [x] Remove the recursive Borsh evaluator, boxed macro crate, caches, bespoke protocol evaluators,
  stale generated clients, and dead schema processor.
- [x] Exercise lifecycle and representative execution against the Agave 4.1-aligned Mollusk runtime.
- [x] Cover 30-recipient SOL, existing-account token batching, stride-two ATA creation, generic data
  interpolation, open execution, privilege failures, guards, row bounds, and CPI rollback.
- [x] Record v1 transaction sizes and compute/heap observations for 1, 8, and 30 transfers.
- [x] Pass Rust checks/tests, TypeScript checks/tests, SBF build, fixture parity, and
  `git diff --check`.

## Supported workflow scenarios

The language-level acceptance set includes payroll and token payout batches, ATA creation, revenue
splits, refunds, DAO/treasury distributions, swap/deposit sequences, rebalancing, oracle-guarded
orders, compounding and liquidity operations, lending workflows, wallet cleanup, migrations, NFT
batching, and conditional escrow settlement. They all compile to the same bounded primitives; none
adds a route finder, price feed, authority, or protocol instruction that does not already exist.
