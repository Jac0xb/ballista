# Safety properties

Status: draft · 2026-10-03 · the one list that every fuzz target, property test, Kani proof and
Certora rule maps to. Built from `docs/guide/trust-model.md`, `docs/reference/language.md`,
`limits.md`, `wire-format.md` and the code, at `53a39be`.

## How to read it

- **Kind.** **S**, safety: a violation lets an upload or a run do what it shouldn't, or misleads
  someone reading the chain. **L**, liveness: a violation makes a valid run fail, or report the
  wrong error, and changes nothing, because the run reverts.
- **Layer**, who enforces it:
  - **SVM**, the Solana runtime, whatever Ballista does;
  - **V**, the verifier, once, at create or finalize;
  - **R**, the program at upload or run time;
  - **TS**, the TypeScript compiler or SDK;
  - **off**, nothing on chain.
- **Status.**
  - **proved**: a Certora rule in `run.conf`. None was confirmed for this list, since `CERTORAKEY`
    is unset, and one of the 15 is wrong (F1). No Kani proofs exist.
  - **sampled**: randomized checks, from proptest or a seeded loop.
  - **tested**: deleting the check makes a named test fail. Seventeen checks were mutated to confirm
    this (see Mutation evidence); for the rest, a cited test asserts the exact error code or end
    state that only the check produces.
  - **masked**: a test aims at the property but passes for the wrong reason, because another
    check, the runtime or the harness rejects or accepts first.
  - **unchecked**: nothing reaches a violation; at most the happy path runs.
- **Marks.** **✗** means the property doesn't hold today (see its finding). **doc ✗** means the
  code holds the property as stated here, but a document claims more.
- **Check prefixes.**
  - V: verifier unit tests (`verify.rs`). X: executor unit tests (`execute.rs`). U: other unit
    tests, in the named file.
  - M: Mollusk tests (`tests/ballista/src/lib.rs`, unless a file is named). R: its
    `mod registry`. MG: `generated_programs_never_hit_structural_errors`.
  - PT: proptests (`common/tests`). PR: protocol tests (`tests/protocols/tests`).
  - C: a Certora rule in `run.conf`. CB: a rule in `run-blocked.conf`.
  - TS: tests in `clients/js/src`.
  - F: this review's tests (`common/tests/property_findings.rs`,
    `tests/ballista/src/tests/property_findings.rs`).
- **Paths.**
  - `lib.rs`, `error.rs`, `processor/{execute,registry,introspect,math}.rs` and `utils/pda.rs` are
    in `programs/ballista/src`.
  - `verify.rs`, `wire.rs`, `account.rs` and `builder.rs` are in `common/src/template`;
    `instruction.rs` is in `common/src`.
  - `compiler.ts`, `instructions.ts` and `errors.ts` are in `clients/js/src`.

## Summary

94 properties, plus 8 that Solana provides (G1–G8).

| Area | Total | Proved | Sampled | Tested | Masked | Unchecked |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Lifecycle | 14 | 0 | 2 | 10 | 0 | 2 |
| Finalization | 28 | 0 | 2 | 25 | 1 | 0 |
| Run | 27 | 1 | 1 | 23 | 0 | 2 |
| Arithmetic | 11 | 5 | 1 | 5 | 0 | 0 |
| Parsing | 5 | 1 | 4 | 0 | 0 | 0 |
| Template-level and off-chain | 9 | 0 | 0 | 2 | 1 | 6 |
| **All** | **94** | **7** | **10** | **65** | **2** | **10** |

Five properties were unchecked or masked until this review's guard tests (P6, P8, P9, P45, P49).
A mutant deleting each of their checks left every Mollusk and host test passing; each guard now
fails under its mutant.

Six properties don't hold today (P67, P87–P91). The three doc ✗ marks (P12, P18, P51) are closed: the docs now state them.

**The 10 most important properties that are unchecked, masked or only sampled:**

1. **P53**: Ballista never signs a template's CPI. Code review is the only check.
2. **P87**: nothing binds a run to the payload its signers expect.
3. **P69**: a run never writes its template account. Only a blocked Certora rule states it.
4. **P2**: uploads need the creator's signature, on a fresh account. No negative test exists.
5. **P42**: register reuse keeps its meaning. Masked: the replay shares its operand table with the
   allocator, so a wrong table passes both.
6. **P88**: the SDKs default to the upgradeable pre-release deployment.
7. **P93**: flows that read logs work under mainnet's 10,000-byte limit. Masked: the protocol
   harness turns the limit off.
8. **P82**: parsed sections tile the payload. Sampled only, and its Certora rule is wrong.
9. **P77**: `mul_div` is exact. Sampled; its Certora rule has never run.
10. **P40**: the executor accepts what the verifier accepts. Sampled by a generator that emits no
    CPI, PDA, data read, introspection or return-data read.

## Findings

Each finding has a reproduction: a test that passes today and pins the behaviour. The exceptions
are F5, whose repro is the CI command itself; F9, whose test is skipped because it fails; and F10
and F11, from the test-methodology critic's runs.

| ID | Kind | What | Repro | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| F1 | Spec bug | The Certora rule `rule_parsed_sections_exactly_consume_the_payload` (in `run.conf`) asserts `inputs.len() == input_count()`, but `parse` reads `input_count + row_input_count` descriptors. Any payload with a row input is a counterexample, so at most 14 of the 15 `run.conf` rules can prove | F `a_row_input_descriptor_is_a_counterexample_to_the_parser_rule` (28 bytes) | Medium: false assurance | Compare with `total_input_count()` |
| F2 | Oracle bug | `TypeMismatch` (6012) also fails a value: a `bool` read of a byte other than 0 or 1, in account data, return data, instruction data or a registry field. `generate.rs` `STRUCTURAL_RUNTIME_ERRORS` and the CB typing rules' `value_dependent` treat 6012 as a verifier gap. The CB account-read rule also leaves out 6009, from a dynamic read past the end, so it would fail on semantics as well as on the prover's memory model | F `a_bool_read_of_a_byte_above_one_fails_with_type_mismatch`; M `dynamic_offset_reads_use_the_register_value` (6009) | Low today. It breaks fuzz oracles once generators read data | Exclude bool decodes from the oracles, or give them a kind of their own |
| F3 | Error code | A data read of an open registry entry through another slot fails with Solana's `AccountBorrowFailed`, with no Ballista code or pc. `errors.md` lists only the missing-signer exception. The run fails closed | F `a_data_read_of_an_open_entry_through_another_slot_fails_without_a_ballista_code` | Low | Map it to a Ballista kind, or document it |
| F4 | Encoding | Closed. The unused operands of FOREACH, REQUIRE and most other opcodes accepted any value, so encodings weren't canonical, and those bytes could never take a meaning without a version bump. One operand table now refuses anything but `0xff`, or zero for the immediate (P17) | F `unused_operands_of_foreach_and_require_are_refused` | Low | Done |
| F5 | CI | The nightly "Extended property tests" job runs `cargo test -p ballista-common --test no_panic --test generated` without `--features proptest`. Cargo refuses the whole command, so the 4,096-case runs never happen; pull requests run 1,000 and 512 cases | The command itself | Medium: lost coverage | Add `--features proptest` |
| F6 | doc ✗, closed | The privilege ceiling is per slot, not per address. A pinned, read-only account still reaches a CPI writable, through an account group or through another writable slot. Group members take Ballista's own instruction privilege, which is the calling program's choice when Ballista runs under a CPI. `language.md` ("tells you the most any run of it can do with the accounts it declares") and `trust-model.md#privileges` ("whenever the transaction marked it writable") claimed more; both now state the ceiling per slot | F `a_pinned_read_only_account_reaches_a_cpi_writable_through_another_slot` | Medium: reviewers misread schemas | Done; flag group forwarding in reviews |
| F7 | doc ✗, closed | `trust-model.md` said "Templates and registry entries are the only accounts Ballista owns." Anyone can create or assign a zero-filled account to Ballista. Such accounts are inert, as the page now says | F `an_account_someone_else_makes_ballista_owned_is_refused_everywhere` | Low | Done |
| F8 | doc ✗, closed | The verifier bounds steps and CPIs, not compute: a 22-instruction template with no CPI exhausts 1.4M compute units. `language.md` said the worst case is known "rather than being cut off by the transaction's compute budget"; it now says compute is not bounded | F `a_verified_template_without_cpis_can_exhaust_the_compute_budget` | Low | Publish a compute bound per opcode |
| F9 | SDK | For a failure inside a nested run, `explainRunError` names a step of the outer template | TS `errors.test.ts` "does not explain a failure inside a nested run by a step of the outer template" (skipped; it fails today) | Low | Track invoke depth, then unskip |
| F10 | Harness | The protocol harness builds LiteSVM with `with_log_bytes_limit(None)` (`tests/protocols/src/snapshot.rs`, `into_svm`), so no scenario meets mainnet's 10,000-byte log limit. The test-methodology critic reports that 3 scenarios fail on cut log lines at that limit; not reproduced here | The critic's branch `claude/critic-tests` | Medium: event readers break on mainnet | Run the scenarios at the mainnet limit, and size their logs |
| F11 | Tests | Tests that assert only `is_err()` pass whatever rejects the input. The critic's mutants show `one_shot_open_run_guards_and_privileges` survives deleting the signer, writable and address checks; this review's show the write-after-finalize case surviving the state check, which the offset check backs up | Mutation evidence below | Medium: false coverage | Assert the exact code and context |

## Given by Solana

Ballista builds on these and doesn't check them.

| ID | Given | Ballista relies on it in |
| --- | --- | --- |
| G1 | Lamports are conserved. Only an account's owner debits it or changes its data; anyone may credit it | P13, P61 |
| G2 | An instruction, and every CPI it makes, leaves an account it holds read-only unchanged | P63 |
| G3 | A CPI can't hold more than its caller: an account is a signer only if the caller held its signature or signs for it as a PDA, and writable only if the caller held it writable | P24, P44, P51 |
| G4 | A program can't be re-entered, except by a direct call to itself | P59 |
| G5 | A failed transaction changes nothing. Earlier CPIs, entry creation and field writes revert with it. Seen in M `thirty_recipient_batch_bounds_and_atomic_rollback` and R `a_second_open_of_an_open_entry_fails` | every property |
| G6 | Only a program signs for its PDAs, so no one else creates an account at a template or entry address | P1, P58 |
| G7 | Limits: a 64-entry instruction trace, 5 call frames, the compute budget (at most 1.4M units), 1,232-byte transactions and 64 accounts. Seen in PR `the_instruction_trace_caps_the_slices_below_the_loops_max`, PR `the_call_depth_limit_is_five_frames` and F8 | P18, P26 |
| G8 | Changing an account's owner needs it writable, not executable and zero-filled, and `CreateAccount` zero-fills. So any Ballista-owned account that Ballista didn't make holds only zeros | P12 |

## Lifecycle

| ID | Property | Kind·Layer | Enforced in | Checks today | Status | Next check |
| --- | --- | --- | --- | --- | --- | --- |
| P1 | **Template address.** A template account exists only at `find_program_address(["template", creator, id LE], ballista)`, with the canonical bump; create and begin refuse any other address | S·R | `lib.rs` `validate_create_accounts`; `pda.rs` `get_template_address` | U `pda.rs` `template_addresses_match_find_program_address` (500 seeded); PR `smoke` | sampled | M: create at a non-PDA address → 6001 |
| P2 | **Upload authority.** Create and begin need the creator to sign and be writable, the template address writable, System-owned and empty, and the System program at its address | S·R | `lib.rs` `validate_create_accounts` | Happy paths only | unchecked | M: an unsigned creator, a used address and a wrong System program each fail |
| P3 | **Rent and dust.** A new template is rent-exempt for 80 + payload bytes. A pre-funded address is topped up by the shortfall, then allocated and assigned, so dust can't block an ID | L·R | `lib.rs` `create_template_account` | M `create_template_succeeds_on_a_prefunded_pda` | tested | — |
| P4 | **Write authority.** Write, finalize and cancel need the header's creator to sign and be writable, and the template to be Ballista's, at the PDA that the header's creator, ID and bump give | S·R | `lib.rs` `validate_owned_writable_template`, `validate_creator_and_pda` | M `chunked_upload_resume_wrong_offsets_hash_and_cancel` (wrong creator; `is_err` only, so either the creator or the PDA check satisfies it) | tested | Lifecycle fuzz (see Next checks), asserting 6005 |
| P5 | **Sequential chunks.** A chunk lands only at `offset == written_len`, with `offset + len ≤ payload_len`; `written_len` never shrinks | S·R | `lib.rs` `write_template_chunk`; `account.rs` `set_written_len` | M chunked (offset 1; `is_err` only) | tested | Lifecycle fuzz, asserting 6006 |
| P6 | **Finalize gate.** Finalize needs the upload complete, `sha256(payload)` equal to the hash recorded at begin, and `parse` + `verify` to accept the payload | S·R | `lib.rs` `finalize_template` | F `finalize_refuses_a_chunked_upload_the_verifier_rejects` (kills the mutant that skips `verify`, which survived every other test); M chunked (hash mismatch, `is_err`). The completeness check is masked: the hash check and `header.finalize()` back it up | tested | M: assert 6006 and 6007 |
| P7 | **Create gate.** One-shot create runs `parse` + `verify` and the hash check before it creates anything | S·R | `lib.rs` `create_template` | M `return_data_set_before_an_invoke_is_rejected_at_create` (6130), `a_log_that_copies_the_run_event_is_rejected_at_create` (6130); R (6132); MG | tested | — |
| P8 | **Finalized is final.** Write, finalize and cancel fail on a finalized template (6003), and nothing returns one to uploading. Without cancel's check, a creator could close a finalized template and upload other bytes at its address | S·R | `lib.rs`: the state checks in all three | F `a_finalized_template_refuses_cancel_write_and_finalize` (kills both state mutants, which survived every other test); M `one_shot_open_run_guards_and_privileges` (a write, masked by the offset check); CB `rule_finalized_templates_reject_chunk_writes`, `rule_finalized_templates_cannot_be_cancelled` | tested | Lifecycle fuzz: a finalized account's bytes and lamports never change |
| P9 | **Uploading never runs.** Run executes only an account that is Ballista's, non-empty and finalized; an uploading one fails with 6004 | S·R | `lib.rs` `run_template`; `account.rs` `finalized_program_unchecked`, `finalized_program` | F `a_fully_written_template_that_is_not_finalized_never_runs` (kills the mutant that drops the slow path's state check, which survived every other test); PT `generated_programs_parse_and_verify` (the fast path); CB `rule_uploading_templates_never_run` | tested | — |
| P10 | **Finalized implies verified.** A finalized account's payload passed `parse` + `verify`, and the run's fast parse splits it exactly as `parse` does | S·R | `lib.rs` `create_template`, `finalize_template`; `wire.rs` `parse_finalized` | PT `no_panic` (same sections wherever `parse` succeeds); PT `generated_programs_parse_and_verify` | sampled | Kani: `parse(p).is_ok() ⇒ parse_finalized(p) == parse(p)`, for p ≤ 96 bytes |
| P11 | **Cancel.** Cancel works only while uploading, moves every lamport to the creator, and closes the account | S·R | `lib.rs` `cancel_template` | M chunked (the creator's balance rises); CB `rule_finalized_templates_cannot_be_cancelled` | tested | M: cancelling a finalized template fails with 6003, and no balance changes |
| P12 | **Strangers' Ballista accounts are inert.** Anyone can create or assign a zero-filled account to Ballista (G8). Every instruction refuses one, as a template (6001) or as an entry (6025), and nothing moves its lamports. Templates and entries never pass for each other (F7) | S·R | `account.rs` `TemplateAccount::parse`, `split_template_account_mut`; `registry.rs` `check_entry` | F `an_account_someone_else_makes_ballista_owned_is_refused_everywhere` | tested | M: pass an entry to write, finalize, cancel and run, and a template to an open |
| P13 | **No withdrawals.** Ballista debits lamports only when cancel closes an uploading template. Nothing debits or closes a finalized template, an entry or an inert account, and G1 bars every other program | S·R+SVM | `lib.rs` `cancel_template`, the only `set_lamports` | R `the_first_run_creates_the_entry_and_later_runs_reopen_it` (an entry holds its rent) | tested | Lifecycle fuzz: the lamports of finalized templates and entries never fall |
| P14 | **Instruction decoding.** `BallistaInstruction::parse` accepts exactly six layouts: run data of at most 1,024 bytes; a create payload or a chunk of 1 to 10,240 bytes; a begin length of 1 to 10,240 with no trailing bytes; finalize and cancel with no data. Anything else fails with 6000 | L·R | `instruction.rs` `BallistaInstruction::parse` | U `parses_borrowed_create_and_run_payloads` (happy path only) | unchecked | cargo-fuzz `parse` against an encoder; a unit test for each rejection |

## Finalization

What the verifier, and the TypeScript compiler before it, checks before a template exists.
Liveness rows are the ones where the executor checks again at run time.

| ID | Property | Kind·Layer | Enforced in | Checks today | Status | Next check |
| --- | --- | --- | --- | --- | --- | --- |
| P15 | **Declarations are well formed.** At most 32 inputs, of which at most 8 are row inputs and only with a batch. At most 256 input values at the maximum row count, 64 registers and 120 accounts at the maximum row count, and 1 to 128 instructions. A stride of at most 8, which is 0 exactly when the maximum row count is 0. A minimum row count no larger than the maximum, and at most 8 groups. Constraint flags only signer, writable and executable, with pins inside the pubkey table. Valid input types, and a `bytes` maximum of 1 to 1,024 | L·V | `verify.rs` `verify` (prologue) | V `header_limits_are_enforced_at_the_boundary`, `minimum_iterations_are_bounded_by_the_maximum`, `row_inputs_and_account_groups_are_verified` | tested | PT: mutate one header field of a valid program; verify accepts exactly when the limit holds |
| P16 | **Reserved fields are zero.** `parse` refuses the header's reserved byte and unknown flag bits. `verify` refuses non-zero reserved bytes in records, and any instruction flag but the dynamic-offset bit on read opcodes | L·V | `wire.rs` `parse`; `verify.rs` `verify_record_header`, `verify_cpi_shape`, `verify_segment` | V `header_magic_version_flags_and_reserved_bytes_are_checked_at_parse`, `instruction_reserved_bytes_must_be_zero`, `math_ops_reject_flags`, `introspection_and_byte_opcodes_reject_flags` | tested | — |
| P17 | **Unused fields are fixed.** Every field an opcode leaves unused holds `0xff`, or zero for the immediate: one operand table checks every record before its opcode's own rules. A FOREACH reports a stray field as `InvalidBatch`, a REPEAT as `InvalidLoop`, a registry opcode as `InvalidRegistry`, a group opcode as `InvalidAccountGroup`, any other as `InvalidInstruction`. Templates finalized before the rule keep running, since a run never verifies again (F4) | —·V | `verify.rs` `operand_fields`, `verify_record_header` | V `unused_fields_hold_no_index_or_zero`; F `unused_operands_of_foreach_and_require_are_refused`; FV2's breaks `unused-operand`, `unused-immediate` and mutant `verify-unused-fields` | tested | — |
| P18 | **Steps and CPIs are bounded; compute is not.** There are no jumps. Loops sit only at the root, at most 8, never nested, each with a non-empty body inside the program and passes capped at its maximum. So steps ≤ root instructions + Σ(body × maximum passes), and CPIs ≤ 64. A 22-instruction template with no CPI still exhausts 1.4M compute units (F8) | L·V+R | `verify.rs` `verify`, `verify_instruction` (loop arms); `execute.rs` `dispatch` | V `loop_shape_rules`, `a_template_holds_up_to_eight_loops_in_sequence_and_none_nested`; X `a_loop_inside_a_loop_body_fails_as_an_invalid_program`; F `a_verified_template_without_cpis_can_exhaust_the_compute_budget` | tested | PT on generated programs: the steps dispatched stay within the static bound |
| P19 | **Registers are set before they're read.** At the root, in order. In a body, from the state before the loop. A carried register, before its loop. The executor checks again (6011) | L·V+R | `verify.rs` `read_register`, `verify_carry_before`, `verify` (drops body typing after the loop) | V `every_opcode_rejects_uninitialized_or_mistyped_operands`, `loop_carried_registers_must_be_initialized_and_keep_their_type`; MG; CB typing rules | tested | Kani: per-instruction typing preservation, the CB typing rule on host |
| P20 | **Typing.** Each operand has the type its opcode needs, and the destination gets the recorded type. A `bytes` register's recorded maximum bounds its length. The executor checks every type again | L·V+R | `verify.rs` `verify_instruction`, `verify_segment` | V `every_opcode_rejects_uninitialized_or_mistyped_operands`, `destination_types`, `select_and_loop_register_typing`; MG; CB typing rules (the account rule is mis-specified, F2) | tested | The same Kani harness; widen the CB rules' error set |
| P21 | **Carries keep their type.** A carried register is set before its loop and leaves the body with the type and `bytes` maximum it entered with | L·V+R | `verify.rs` `verify_carry_before`, `verify_carry_after` | V `loop_carried_registers_must_be_initialized_and_keep_their_type` | tested | — |
| P22 | **References resolve.** An account reference names a fixed account, or inside a FOREACH a row account below the stride, and never a group member. `LOAD_INPUT` names a fixed input, or inside a FOREACH a row input. The executor re-checks accounts but not row inputs: one inside a REPEAT would read row `pass`'s values | L·V+R | `wire.rs` `account_constraint`, `input_descriptor`; `verify.rs` `names_row`; `execute.rs` `resolve` | V `blob_pubkey_account_and_input_ranges`, `count_loop_bodies_have_an_index_but_no_rows`, `row_inputs_and_account_groups_are_verified` | tested | — |
| P23 | **Tables are in range.** Every pubkey, blob range, data segment and CPI descriptor an instruction names exists. A u128 literal is exactly 16 bytes, and a `bytes` literal at most 1,024 | L·V+R | `verify.rs` `verify_instruction`, `verify_segment`, `verify_cpi_shape` | V `blob_pubkey_account_and_input_ranges`, `data_segment_rules` | tested | — |
| P24 | **Privilege ceiling, per slot.** Each CPI record's flags are at most signer and writable, and no more than its slot declares. The CPI's program slot and a PDA's program slot are declared executable. **Verifier only:** the executor copies record flags into the CPI unchecked (`execute.rs` `invoke_cpi`), and G3 bounds only by what Ballista's instruction holds | S·V | `verify.rs` `verify_cpi`, `verify_pda_seeds` | V `cpi_privilege_and_shape_rules`, `pda_seed_rules`. Deleting the rule fails 1 host test and nothing in the Mollusk or protocol suites | tested | Kani: `verify_cpi` accepts ⇒ every record's flags ⊆ its slot's flags; a Mollusk test that uploads an escalating CPI |
| P25 | **CPI shape, and nothing unused.** Every descriptor has zero reserved bytes, a group below the group count, at most 64 listed accounts, a data maximum of at most 4,096 and in-range tables, and an INVOKE names it. Every data segment is part of an invoked descriptor's data, an output or a PDA's seeds, and every use checks its fields alike, naming a bad one by its table index. An invoked descriptor's data maximum equals its worst case | L·V+R | `verify.rs` `verify_cpi_shape`, `verify_cpi`, `verify_segment`, `verify_references` | V `cpi_account_counts_are_bounded_even_when_never_invoked`, `cpi_privilege_and_shape_rules`; U `common/tests/fuzz_findings.rs` (all six); FV2's breaks `uninvoked-descriptor`, `unreferenced-segment`, `cpi-segment-literal-register`, `cpi-segment-register-fields` and their mutants | tested | — |
| P26 | **CPI count.** Root INVOKEs + Σ(INVOKEs in a body × its maximum passes) + 3 × opens ≤ 64. Loop counts can't pass their maximums at run time, so no run makes more calls. G7's trace limit usually binds first | L·V+R | `verify.rs` `verify`; `execute.rs` `enter_loop`, `loop_count` | V `worst_case_invocations_add_up_over_every_loop` | tested | MG once the generator emits INVOKE: the run event's `expanded` ≤ `max_expanded_cpis` |
| P27 | **Return-data reads follow their call.** A RETURN_DATA directly follows an unguarded INVOKE in the same range, names a read opcode, and ends within 1,024 bytes | L·V+R | `verify.rs` `verify_instruction` (`OP_RETURN_DATA`) | V `return_data_reads_must_directly_follow_an_unconditional_invoke`; PR `a_return_data_read_before_the_inner_call_is_refused_at_upload` | tested | — |
| P28 | **Output shape.** An EMIT or SET_RETURN_DATA names no register, at least one in-range segment, and at most 1,024 bytes in the worst case | L·V+R | `verify.rs` `verify_output` | V `outputs_are_encoded_like_invocation_data_up_to_the_return_data_limit`, `output_records_name_a_non_empty_range_and_no_register` | tested | — |
| P29 | **Emit tags.** Every EMIT starts with a literal of at least 4 bytes that doesn't start with `BEV`. Verifier only | S·V | `verify.rs` `verify_emit_tag` | V `an_emit_starts_with_a_tag_outside_the_run_event_family`; M `a_log_that_copies_the_run_event_is_rejected_at_create` | tested | Kani: an accepted EMIT starts with a literal of 4 or more bytes outside `BEV` |
| P30 | **Return-data placement.** SET_RETURN_DATA appears at most once, at the root, with no INVOKE, SET_RETURN_DATA or OPEN_REGISTRY after it. Verifier only | S·V | `verify.rs` `verify_instruction` (`OP_SET_RETURN_DATA`), `verify_open_registry` | V `return_data_is_set_once_outside_every_loop_after_the_last_invoke`, `return_data_is_not_set_inside_a_count_loop`; M `return_data_set_before_an_invoke_is_rejected_at_create` | tested | — |
| P31 | **Introspection is pinned.** Opcodes 64 to 72 name a fixed account pinned to the Instructions sysvar. Their operands are `u64` registers, and byte reads take 1 to 1,024 bytes. The executor checks the address again | S·V+R | `verify.rs` `require_instructions_sysvar` | V `introspection_needs_a_fixed_account_pinned_to_the_instructions_sysvar`, `introspection_indexes_positions_and_offsets_are_u64_registers` | tested | — |
| P32 | **Static read bounds.** A fixed-offset read ends within the account's declared minimum length; a dynamic one takes a `u64` offset register and a zero immediate. The check is static: P64 covers the account as it is when read | L·V+R | `verify.rs` `verify_read_bounds` | V `fixed_offset_reads_must_fit_the_declared_minimum_data_length`, `dynamic_offset_reads_take_a_u64_register_and_skip_the_static_bound` | tested | — |
| P33 | **Account byte reads.** READ_ACCOUNT_BYTES names a declared account, never a fixed one declared writable, for 1 to 1,024 bytes. The executor checks writability again (6024) | L·V+R | `verify.rs` `verify_instruction` (`OP_READ_ACCOUNT_BYTES`) | V `account_byte_reads_take_a_declared_account_and_a_u64_offset`, `account_byte_reads_refuse_a_fixed_account_declared_writable` | tested | — |
| P34 | **PDA seeds.** DERIVE_PDA and CREATE_PDA take an executable program slot and 1 to 15 seeds of at most 32 bytes each. CREATE_PDA's bump is a `u64` register | L·V+R | `verify.rs` `verify_pda_seeds` | V `pda_seed_rules`, `create_pda_requires_a_u64_bump` | tested | — |
| P35 | **Loop shape.** A batch exists exactly when a FOREACH does, and every FOREACH body is non-empty. A REPEAT has a body, a maximum of 1 to 255, `dst` 0xff and a `u64` count, and names no row account or row input | L·V+R | `verify.rs` `verify`, `names_row` | V `loop_shape_rules`, `count_loops_take_a_u64_count_a_body_and_a_maximum`, `count_loop_bodies_have_an_index_but_no_rows` | tested | — |
| P36 | **Registry opens.** OPEN_REGISTRY sits at the root, at most 8 times, once per slot, with no SET_RETURN_DATA and no INVOKE before it, in a loop body or not. Its index is below 8, and its size is 1 to 512 and the same for every open of one index. The entry slot is declared exactly writable, the payer is a fixed signer and writable slot, the System program slot is pinned, and the key is a `pubkey` register or zero. So every CPI comes after the opens and meets the borrow mark (P59, P92) | S·V | `verify.rs` `verify_open_registry` | V `registry_rules`; R (6132 at upload); M `fuzz/registry_ordering.rs` `aliased_entry_read_should_be_refused`, `fuzz/critic_lost_write.rs` `critic_the_verifier_refuses_a_call_before_an_open`; FV2's negative break `open-after-invoke` | tested | — |
| P37 | **An entry slot is never listed writable.** No CPI record names an opened entry slot writable, invoked or not. Every invoke follows the opens (P36), so the run would refuse such a call anyway (P59) | L·V | `verify.rs` `verify_open_registry` (the record scan) | V `no_cpi_passes_an_entry_account_writable`; R `a_cpi_that_passes_an_open_entry_writable_fails_with_registry_reentry` (the 6132 part) | tested | — |
| P38 | **Entries are read only through fields.** READ_REGISTRY and WRITE_REGISTRY name a slot opened earlier and a field inside its size. A write takes one of the five full-width selectors and a value of its type. No data read names an opened slot. That last rule is the verifier's alone for a read before the open, and it's per slot: another slot holding the entry can read its bytes before the open | S·V | `verify.rs` `registry_field`, `refuse_entry_data` | V `registry_rules`, `entry_data_is_read_only_through_its_fields`; R `a_template_that_reads_an_entry_as_data_is_refused_at_upload` | tested | — |
| P39 | **Verification is total.** `parse` and `verify` return an error, and never panic, for any bytes. So does `verify_single_instruction`, which the specs call, on a view declaring more than 64 registers | L·V | `wire.rs` `parse`; `verify.rs` `verify`, `verify_single_instruction` | PT `no_panic` (5 strategies × 1,000 cases; the nightly 4,096-case run never runs, F5); U `common/tests/fuzz_findings.rs` `verifying_one_instruction_never_panics` | sampled | cargo-fuzz `parse` + `verify`, seeded from `fixtures/` |
| P40 | **The executor accepts what the verifier accepts.** For any verified program and any run, the executor never panics and fails only because of values. It never fails with 6002 or 6011, and with 6012 only for a `bool` byte other than 0 or 1 (F2). This is liveness, since the executor checks again and reverts | L·V+R | `verify.rs`, `execute.rs` | MG (48 random programs, with no CPI, PDA, data read, introspection or return-data read); PT `generated_programs_parse_and_verify`; CB typing rules | sampled | Extend `generate.rs` to those opcodes; cargo-fuzz the executor on host with generated programs |
| P41 | **Compiled templates pin what they call and read.** Every program a TypeScript template invokes or derives a PDA under pins its address and is executable. Every account whose data it reads pins its owner or address. `unsafeUnpinned` waives the pins, never the sysvar's. The program checks no pins | S·TS | `compiler.ts` `requirePinnedProgram`, `requirePinnedForRead`, `encodeSysvar` | TS `compiler.test.ts` "requires pins for programs and data reads unless the author opts out", "the sysvar must be a fixed account pinned to its address" | tested | TS tests for the executable rule, and for `unsafeUnpinned` on the sysvar |
| P42 | **The compiler's output verifies, and register reuse keeps its meaning.** Every TypeScript-compiled template passes the Rust verifier. Renumbering past 64 registers computes what one register per value would. Only the accept side is checked: no template the compiler refuses reaches the verifier | S·TS | `compiler.ts` `compile`, `reuseRegisters`, `REGISTER_OPERANDS` | TS `fixtures.test.ts` + V `every_shared_fixture_parses_and_verifies`; M `typescript_compiled_fixtures_run_end_to_end`, `register_reuse.rs`. The reuse replay is masked: it reads the same operand table as the allocator, so with `mulDiv` or `select` missing an operand it accepts a miscompile (the critic's `clients/js/critic/reuse-demo.ts`) | masked | Check the operand table against `verify.rs` independently; differential runs of renumbered against unrenumbered programs |

## Run

| ID | Property | Kind·Layer | Enforced in | Checks today | Status | Next check |
| --- | --- | --- | --- | --- | --- | --- |
| P43 | **Account layout.** At most 120 runtime accounts, and rows = (accounts − fixed − Σ group lengths) / stride exactly, between the minimum and maximum. No rows without a batch, and one group-length byte per group (6010, 6008) | L·R | `execute.rs` `validate_runtime_accounts`, `split_group_prefix` | M `one_hundred_twenty_runtime_accounts_are_the_ceiling`, `thirty_recipient_batch_bounds_and_atomic_rollback`, `carried_sum_enforces_a_budget_across_rows`, `account_groups_are_forwarded_after_declared_accounts`; X `group_prefix_is_one_byte_per_declared_group`; CB `rule_account_count_must_match_the_schema` | tested | Kani on `validate_runtime_accounts` with symbolic counts |
| P44 | **Signers, writability and pins are checked at run start.** Each fixed and row slot's account signed if declared a signer (otherwise Solana's `MissingRequiredSignature`), is writable if declared so, and has its pinned address and owner (otherwise 6020 with its index). For a signer that no CPI receives, which only authorizes the run, this check is the only one: G3 checks only signatures passed on. It runs once: a CPI can later resize, close or reassign the account (P64) | S·R | `execute.rs` `validate_account` | Deleting each check fails: signer, 1 protocol test (and a compute ceiling); writable, R `an_account_that_is_not_the_named_entry_fails`; address pin, 2 protocol tests; owner pin, 1 Mollusk and 1 protocol test. M `one_shot_open_run_guards_and_privileges` is masked for all of them: it asserts only `is_err`. CB `rule_signer_constraints_require_signers`, `rule_writable_and_executable_constraints_are_enforced`, `rule_pinned_address_and_owner_are_enforced` | tested | Assert exact codes in M `one_shot_open_run_guards_and_privileges` |
| P45 | **Executable flag and minimum length.** The same check enforces `executable` and the minimum data length (6020) | L·R | `execute.rs` `validate_account` | F `executable_and_minimum_length_declarations_are_enforced` (kills both mutants, which survived every other test); CB `rule_writable_and_executable_constraints_are_enforced`, `rule_minimum_data_length_is_enforced` | tested | — |
| P46 | **Inputs decode exactly.** The group prefix, the fixed values, then one row of values per row. A `bool` is 0 or 1, a `bytes` value has a u16 length within its maximum, and nothing is left over (6008, with the value's index) | S·R | `execute.rs` `parse_run_inputs`, `InputReader` | X `input_parsing_covers_each_type_and_rejects_malformed_bytes`, `run_inputs_carry_a_row_per_iteration`; M `row_inputs_pay_a_different_amount_per_recipient` | tested | PT: decoding random encoded values returns them; any truncation or extension fails |
| P47 | **Rows bind in order.** In FOREACH pass p, row account k is runtime account fixed + p × stride + k, and row input k is value fixed + p × row inputs + k. No reference reaches a group member | S·R | `execute.rs` `resolve`, `step` (`LOAD_INPUT`), `enter_loop` | M `i32_reads_resolve_each_rows_account_in_a_loop`, `row_inputs_pay_a_different_amount_per_recipient`, `thirty_recipient_batch_bounds_and_atomic_rollback` | tested | — |
| P48 | **Loop passes.** A FOREACH makes one pass per row, from the first. A REPEAT reads its `u64` count once, at entry, fails with 6022 above its maximum, and otherwise makes exactly that many passes | S·R | `execute.rs` `enter_loop`, `loop_count`, `next_pass` | X `count_loops_run_their_body_count_times_and_carry_values_out`, `rewriting_the_count_in_the_body_leaves_the_passes_unchanged`, `loops_run_in_sequence_and_every_foreach_starts_at_the_first_row`; M `typescript_loops_fixture_runs_count_and_row_loops_in_sequence` (6022) | tested | — |
| P49 | **Loop registers.** Each pass starts from the registers as they were before the loop, plus the carried values. After the loop, a register not carried holds its value from before the loop, and a carried one its value from the last pass | S·R | `execute.rs` `enter_loop` (snapshot, `restore`), `next_pass` | F `a_loop_restores_the_registers_it_does_not_carry` (kills the mutant that skips the restore, which survived every other test); X `count_loops_run_their_body_count_times_and_carry_values_out`, `every_loop_of_a_run_snapshots_into_one_buffer`; M `carried_sum_enforces_a_budget_across_rows` | tested | Differential: generated programs on the executor and on a reference interpreter without the `restore` shortcut |
| P50 | **A reused CPI equals a fresh one.** In a loop that invokes one descriptor, the executor reuses the account list, rebinding only row slots. When no segment reads a register the body writes, it reuses the data too. Either way it sends exactly what a fresh build would | S·R | `execute.rs` `invoke_cpi`, `loop_cache_plan`, `cpi_data_is_loop_invariant`, `rebind_row_accounts` | M `invocation_data_derived_in_the_loop_is_rebuilt_every_row` and 4 more fail when the data is always reused | tested | The same differential, with a logging callee recording each CPI |
| P51 | **CPI privileges, per address.** A CPI passes address X as a signer only through a slot declared signer whose record asks for it, and only if X signed. It passes X writable only through a slot declared writable whose record asks for it, or as a forwarded group member that Ballista's instruction holds writable: the transaction's choice at the top level, the calling program's under a CPI. A pinned, read-only declaration doesn't keep X from a CPI writable if X also fills a group or another writable slot (F6) | S·R+SVM | `execute.rs` `invoke_cpi`, `validate_account`; G3 | F `a_pinned_read_only_account_reaches_a_cpi_writable_through_another_slot`; M `account_group_members_never_sign`; PR `a_settlement_the_maker_does_not_co_sign_fails` | tested | M: one signer in two slots |
| P52 | **Group members never sign.** They follow the listed accounts, never as signers, and are never read or checked | S·R | `execute.rs` `invoke_cpi` (the group arm), `resolve` | M `account_group_members_never_sign`, `account_groups_are_forwarded_after_declared_accounts` | tested | — |
| P53 | **Ballista never signs a template's CPI.** Template CPIs go through `bounded_invoke`, with no seeds. Ballista signs only `CreateAccount`, `Allocate` and `Assign`, for a template at upload or an entry at open | S·R | `execute.rs` `bounded_invoke` (`invoke_signed_unchecked(.., &[])`); `lib.rs` `create_template_account`; `registry.rs` `create_entry` | Review only: those six calls are the only signed invokes | unchecked | M: a template that transfers from its own template PDA, or from an open entry, fails with `MissingRequiredSignature`; a source test that `invoke_signed` appears only in those two functions |
| P54 | **CPI bounds at run time.** Listed plus group accounts number at most 64 (6021, with the total). Data stays within the descriptor's maximum and 4,096 bytes (6016) | L·R | `execute.rs` `invoke_cpi`, `bounded_invoke` | M `cpi_account_limit_counts_the_group` (6021), `many_large_cpis_fit_in_the_default_heap` | tested | — (6016 can't happen to a verified template) |
| P55 | **Return data comes from the program just called.** RETURN_DATA reads only if the INVOKE before it ran, that program set the current return data, and the data covers the read (6018, 6019) | S·R | `execute.rs` `read_return_data`, `invoke_cpi` (`last_invoked`) | M `return_data_is_readable_right_after_the_invoke_that_set_it` (6018), `a_nested_template_reads_the_return_data_its_callee_set`; PR `a_nested_run_cannot_pass_off_another_programs_return_data` (6019). Deleting the provenance check leaves every Mollusk and host test passing; only that protocol test asserts it | tested | M: a callee whose own callee sets the return data → 6019 |
| P56 | **A run's return data and logs.** What SET_RETURN_DATA sets is the instruction's return data at the end. Every `Program data:` line Ballista logs is a tagged EMIT, or the 47-byte run event, logged once after a successful run that sets the flag | S·V+R | `execute.rs` `write_output`, `run`, `encode_event`; with P29 and P30 | M `typescript_output_fixture_logs_every_row_and_returns_the_total`, `event_flag_does_not_change_run_semantics`, `run_logs_for_the_sdk_decoders`; X `run_events_have_a_fixed_documented_layout` | tested | — |
| P57 | **An open checks an existing entry.** An existing account opens only if Ballista owns it, it is writable and 72 + size bytes long, and its header is `BREG`, 1, index, 0, template, key (otherwise 6025) | S·R | `registry.rs` `open`, `check_entry` | U `registry.rs` `an_existing_entry_opens_only_when_everything_matches`; R `an_account_that_is_not_the_named_entry_fails`, `an_existing_entry_of_another_size_fails`; PR `a_caller_cannot_use_another_callers_entry` | tested | — |
| P58 | **An open creates an entry only at its address.** That is `find_program_address(["registry", template, [index], key], ballista)`, on a System-owned empty account. The entry is rent-exempt for 72 + size bytes, paid by the payer (only the shortfall if pre-funded), owned by Ballista, with its header written and its fields zero | S·R | `registry.rs` `create_entry`; `pda.rs` `get_registry_address` | U `pda.rs` `registry_addresses_match_find_program_address` (500), `registry_addresses_match_the_shared_vectors`; U `registry.rs` `a_missing_entry_must_sit_at_its_derived_address`; R `the_first_run_creates_the_entry_and_later_runs_reopen_it`, `a_pre_funded_address_is_topped_up_allocated_and_assigned`, `a_created_entry_is_the_header_plus_the_declared_size`; PR `the_first_swap_creates_the_callers_entry` | tested | — |
| P59 | **An open entry stays out of reach.** An account opens once per run (otherwise 6025, and nothing is created). From its open to the end of the run, no CPI receives it writable through any slot, row or group member (6026). No CPI runs before the opens (P36), so no nested run can write it between this run's read and its write | S·V+R | `registry.rs` `open` (the borrow mark it keeps); `execute.rs` `bounded_invoke`, `reentry_or`; `verify.rs` `verify_open_registry` | U `registry.rs` `an_existing_entry_opens_only_when_everything_matches` (second open); R `a_second_open_of_an_open_entry_fails`, `a_cpi_that_passes_an_open_entry_writable_fails_with_registry_reentry` (group member only; it alone fails when the borrow check is deleted); M `fuzz/registry_ordering.rs` `a_call_through_the_alias_after_the_open_fails_with_registry_reentry` (another fixed slot), `fuzz/critic_lost_write.rs` `critic_with_the_open_first_the_nested_run_meets_the_borrow_mark` | tested | M: the same through a row |
| P60 | **Fields stay inside their entry.** READ_REGISTRY and WRITE_REGISTRY act only on an account this run's open marked. A write stores exactly its selector's width at 72 + offset, inside the account, from a value of the selector's type | S·R | `execute.rs` `registry_instruction`; `registry.rs` `write_field`, `put` | U `registry.rs` `fields_are_written_at_their_width_and_type`, `reads_and_writes_go_through_the_executor_on_an_open_entry`; R `every_writable_width_round_trips` | tested | — |
| P61 | **Only its template's runs write an entry.** A run writes only entries whose header names the running template, and G1 bars every other program | S·R+SVM | `registry.rs` `check_entry`, `create_entry` | R `an_account_that_is_not_the_named_entry_fails` (another template's entry) | tested | — |
| P62 | **Introspection reads the sysvar only.** The executor checks the address again. An index, position or range outside the transaction fails with 6023 | S·R | `introspect.rs` `sysvar_data`, `read_instruction`, `byte_range` | U `introspect.rs` `introspection_refuses_any_account_but_the_sysvar`, `indexes_and_positions_outside_the_transaction_fail`; M `typescript_introspection_fixture_reads_the_transaction` (6023), `signed_quote_settles_only_as_the_maker_signed` | tested | — |
| P63 | **Lent bytes don't change.** A `bytes` value lent from an account comes only from one that is read-only in this instruction (otherwise 6024), so G2 keeps it fixed. One lent from instruction data comes only from the sysvar | S·R+SVM | `introspect.rs` `read_only_data`, `sysvar_data` | U `introspect.rs` `account_bytes_come_only_from_read_only_accounts_and_hold_no_borrow`; M introspection fixture (6024) | tested | — |
| P64 | **A read sees the account as it is now.** Every data read is checked against the account's current data (6009, or 6023 for bytes). After a CPI shrinks or closes the account, a fixed-offset read fails; after one grows or rewrites it, a read returns what is there now; after a reassignment, the owner pin checked at run start no longer holds. A template must read `owner` again after a CPI to rely on it | S·R | `execute.rs` `read_account`, `read_array`; `introspect.rs` `byte_range` | M `dynamic_offset_reads_use_the_register_value` (6009), `data_length_reflects_reallocation_after_a_cpi`; X `register_and_byte_access_reject_unset_and_out_of_range` | tested | M: a fixed-offset read after a CPI shrinks the account; Kani on `read_array` |
| P65 | **PDAs match Solana's.** DERIVE_PDA equals `find_program_address`, trying bumps 255 down to 1. CREATE_PDA equals `create_program_address(seeds ‖ [bump])`. A bump above 255, or a result on the curve, fails with 6017 | S·R | `pda.rs` `Preimage::find`, `Preimage::create`; `execute.rs` `derive_pda` | U `pda.rs` `find_and_create_match_the_address_crate` (1,000), `seed_bounds_match_the_address_crate`; M `pda_equivalence.rs` (5 seeded tests on SBF), `a_supplied_bump_derives_once_and_still_rejects_substitutes` | sampled | Kani on `Preimage`'s offsets with the hash stubbed: the layout is the concatenation, with no overflow |
| P66 | **Error encoding.** Codes round-trip. Runtime kinds 6000–6026 and verifier kinds 6100–6132 are contiguous, distinct and disjoint, and the name tables match the shared fixtures | L·R | `wire.rs` `encode_error`, `decode_error`, `TemplateError::code`; `error.rs` `vm_error` | C `rule_error_codes_round_trip`, `rule_runtime_error_codes_carry_context_and_stay_in_range`, `rule_verifier_error_codes_are_distinct_and_in_range`; U `wire.rs` `error_codes_are_unique_and_round_trip_context`; U `error.rs` (2 tests) | proved | — |
| P67 | **Every run failure carries a code.** A failure the run raises is `Custom(kind \| context << 16)`, and a callee's error passes through unchanged. ✗ Two exceptions: a missing signer, which is documented, and `AccountBorrowFailed` for a data read of an open entry through another slot (F3) | L·R | `execute.rs` `RunError::at`, `before_execution` | X `vm_errors_carry_the_program_counter_and_pass_callee_errors_through`; M `callee_errors_pass_through_unchanged`; F `a_data_read_of_an_open_entry_through_another_slot_fails_without_a_ballista_code` | tested ✗ | Map the borrow failure to a Ballista kind, or document it (owner decision) |
| P68 | **Bounded heap and stack.** A run allocates its inputs, its registers, one set of CPI buffers, one register snapshot and one 1,024-byte output buffer, and reuses each. 64 CPIs of 4 KiB fit the 32 KiB heap, and no SBF frame exceeds 4 KiB | L·R | `execute.rs` `Scratch`, `enter_loop`, `write_output` | M `many_large_cpis_fit_in_the_default_heap`, `pda_derivation_in_every_row_fits_in_the_default_heap`, `eight_loops_share_one_register_snapshot_in_the_default_heap`; X `every_loop_of_a_run_snapshots_into_one_buffer`; frame sizes: the `cargo certora-sbf` report, in CI's prover job only | tested | Run `cargo certora-sbf` on every pull request; it needs no key |
| P69 | **A run never writes its template.** The run holds a shared borrow of the template account throughout, so no CPI receives it writable and no step writes it | S·R | `lib.rs` `run_template`; `execute.rs` `bounded_invoke` | CB `rule_run_never_writes_the_template_account` | unchecked | M: fill a writable slot with the template account and pass it to a CPI; the run fails and the template's bytes and lamports don't change |

## Arithmetic

| ID | Property | Kind·Layer | Enforced in | Checks today | Status | Next check |
| --- | --- | --- | --- | --- | --- | --- |
| P70 | **u64 and i64 arithmetic is checked.** ADD, SUB, MUL, DIV, MIN and MAX equal Rust's checked operations. Overflow fails with 6013 and a zero divisor with 6014; `i64::MIN / −1` is 6013 | S·R | `execute.rs` `arithmetic` | C `rule_u64_arithmetic_is_checked`, `rule_i64_arithmetic_is_checked`, `rule_i64_division_reports_zero_and_overflow` (its errors only); X `arithmetic_checks_every_numeric_type` | proved | Kani: the i64 quotient |
| P71 | **u128 arithmetic is checked.** The same, for u128 | S·R | `execute.rs` `arithmetic` | X `arithmetic_checks_every_numeric_type`; CB `rule_u128_arithmetic_is_checked` | tested | Kani |
| P72 | **Mixed operands fail.** Arithmetic on mixed or non-numeric operands fails with 6012 | L·R | `execute.rs` `arithmetic` | C `rule_arithmetic_rejects_mixed_and_non_numeric_operands` | proved | — |
| P73 | **Comparisons.** All six match Rust on u64 and i64. Ordering takes numbers only; EQ and NE take any two values of one type | S·R | `execute.rs` `compare` | C `rule_u64_comparisons_match_rust`, `rule_i64_comparisons_match_rust`, `rule_ordering_is_numeric_only`; X `comparisons_restrict_ordering_to_numbers` | proved (u128, pubkey and bytes: tested) | Kani for u128 and bytes |
| P74 | **Casts.** A cast succeeds exactly when the value fits (otherwise 6013); a non-numeric value fails with 6012 | S·R | `execute.rs` `cast` | C `rule_casts_succeed_exactly_when_the_value_fits` (5 of the 6 pairs); X `casts_cover_every_pair_and_reject_out_of_range` | proved (u128 → i64: tested) | Kani for the last pair |
| P75 | **u64 remainder, shifts and bit operations.** REM is `checked_rem`, and a zero divisor fails with 6014. SHL fails with 6013 rather than drop a set bit. SHR by 64 or more gives 0. AND, OR and XOR are exact | S·R | `math.rs` `integer`, `remainder`, `shift`, `bitwise` | C `rule_u64_integer_operations_match_rust` | proved | — |
| P76 | **The same for u128, and i64 remainder** | S·R | `math.rs` | U `math.rs` `remainder_follows_rust`, `shifts_never_drop_a_set_bit_silently`, `bitwise_operations` | tested | Kani |
| P77 | **`mul_div` is exact.** MUL_DIV and MUL_DIV_CEIL return ⌊a·b/c⌋ and ⌈a·b/c⌉ of the exact product. A zero c fails with 6014, and a quotient too large for the type with 6013 | S·R | `math.rs` `mul_div`, `mul_div_u64`, `mul_div_u128`, `mul64`, `full_product`, `divlu`, `divide_3by2`, `divide_wide` | U `math.rs` `mul_div_is_the_exact_floor_or_ceiling`, `mul_div_u64_matches_the_u128_path`, `knuth_d_matches_the_bit_by_bit_reference`, `divlu_matches_u128_division` (50,000 random cases each), `mul_div_edges`; CB `rule_u64_mul_div_is_exact` (never run); M math fixture; PR `the_floor_is_exact_to_the_last_unit` | sampled | Kani over every u64 (`mul64`, `divlu`, `mul_div_u64`); run the CB rule; cargo-fuzz u128 against a bignum |
| P78 | **Powers of ten.** POW10(n) is 10ⁿ as a u128 for n ≤ 38, and fails with 6013 above | S·R | `math.rs` `pow10` | U `math.rs` `powers_of_ten_up_to_ten_to_the_thirty_eighth` (every n ≤ 38, then 39 and `u64::MAX`) | tested | Kani over every n |
| P79 | **No silent narrowing.** The u8, u16, u32 and u64 encodings of a u64 or u128 fail with 6013 when the value doesn't fit | S·R | `execute.rs` `encode_register_segment` | X `data_segments_encode_each_kind_and_reject_narrowing_overflow`, `outputs_reject_bad_operands_and_unverified_shapes` | tested | Kani |
| P80 | **Typed reads decode exactly.** Each read opcode decodes its width little-endian. A `bool` takes only 0 or 1 (otherwise 6012), and an i32 sign-extends | S·R | `execute.rs` `decode_value` | X `typed_reads_cover_every_width_and_reject_unknown_opcodes`, `i32_reads_sign_extend`; M `i32_reads_at_dynamic_offsets_sign_extend_or_fail_at_the_read`; F `a_bool_read_of_a_byte_above_one_fails_with_type_mismatch` | tested | — |

## Parsing

| ID | Property | Kind·Layer | Enforced in | Checks today | Status | Next check |
| --- | --- | --- | --- | --- | --- | --- |
| P81 | **The header comes first.** `ProgramView::parse` refuses a payload over 10,240 bytes, then one under 24 (`Truncated`), then a wrong magic, then a wrong version, before it reads any section | L·R | `wire.rs` `ProgramView::parse` | C `rule_parse_checks_magic_then_version_first`, `rule_short_payloads_are_truncated_not_misparsed` (payloads of up to 96 bytes) | proved (≤ 96 bytes) | — |
| P82 | **Sections tile the payload.** On success each table holds its header count, with fixed plus row inputs, and header, tables and blob make up exactly the payload, with no overflow | S·R | `wire.rs` `parse`, `take_records` | C `rule_parsed_sections_exactly_consume_the_payload` (wrong, F1); PT `no_panic` | sampled | Fix the rule; Kani on `parse` for payloads of up to 96 bytes |
| P83 | **Parsing never panics.** For any bytes, `parse` returns a view or an error, and reads nothing past the payload | L·R | `wire.rs` `parse` | PT `no_panic` | sampled | cargo-fuzz |
| P84 | **The fast parse agrees.** `parse_finalized(p)` returns the sections `parse(p)` does, whenever `parse(p)` succeeds | S·R | `wire.rs` `parse_finalized` | PT `no_panic` (`same_sections`); PT `generated_programs_parse_and_verify` | sampled | Kani, for payloads of up to 96 bytes |
| P85 | **The template account parses strictly.** `TemplateAccount::parse` accepts only discriminator 1, version 2, zero reserved bytes, and state 0 or 1. The payload length is 1 to 10,240 and equals the data after the 80-byte header. The written length is at most the payload length, and equals it once finalized | S·R | `account.rs` `TemplateAccount::parse` | PT `generated_programs_parse_and_verify` (finalized against uploading) | sampled | PT over random headers |

## Template-level and off-chain

What a signer or a reviewer has to trust beyond the program.

| ID | Property | Kind·Layer | Enforced in | Checks today | Status | Next check |
| --- | --- | --- | --- | --- | --- | --- |
| P86 | **An address names one payload, once finalized.** After finalize, the bytes at a template address never change. Before it, the creator can cancel and upload different bytes at the same address. So "trust a template by its exact address" (`failure-modes.md`) holds only for a finalized template | S·R | P8, P11 | F `a_finalized_template_refuses_cancel_write_and_finalize`; M chunked (cancel, but no re-upload) | tested | SDK: show the state and payload hash before an address is trusted |
| P87 | **A run is bound to the payload its signers expect.** ✗ Nothing binds one. `Run` executes whatever finalized template account it gets. `buildRunInstruction` (`instructions.ts`) pairs any compiled template with any template address. The SDK has no disassembler (`inspectTemplate` returns counts) | S·off | — | None | unchecked | SDK: fetch the account, and refuse to build or sign unless its payload's sha256 matches the compiled template's; a disassembler |
| P88 | **The SDKs point at a trustworthy deployment.** ✗ `BALLISTA_PROGRAM_ADDRESS` (`instructions.ts`) and `ballista_sdk::ID` default to `BLSTAx…`, the pre-release devnet build. It has an upgrade authority and rejects this repository's templates (`devnet.md`) | S·off | `instructions.ts`; `clients/rust/src/lib.rs` | None (documented) | unchecked | No default until a release, or one per cluster |
| P89 | **A deployment matches this source.** ✗ No verifiable build ties a deployed binary to the repository | S·off | — | None | unchecked | Verifiable, reproducible builds for releases |
| P90 | **The proofs cover the deployed binary.** ✗ Certora analyzes a `cargo certora-sbf` build: platform tools v1.53, the `spec-api` and `no-entrypoint` features, and the rules linked in. CI tests and would deploy the `cargo build-sbf` output instead, built with v1.54 | —·off | `certora/` | None | unchecked | Compare the program's functions across the two builds, or prove on the release object |
| P91 | **Errors are attributed to the right template.** ✗ For a failure inside a nested run, `explainRunError` maps the inner run's program counter onto the outer template's source map, since `failedProgram` returns the first failure line, which is the inner one (F9) | L·TS | `errors.ts` `failedProgram`, `explainRunError` | TS `errors.test.ts` "does not explain a failure inside a nested run by a step of the outer template" (skipped; it fails today) | unchecked | Fix, then unskip the test |
| P93 | **Flows that read logs work on mainnet.** A scenario whose client reads its `Program data:` lines keeps them under mainnet's 10,000-byte log limit. Masked: the protocol harness turns the limit off (F10) | L·off | `tests/protocols/src/snapshot.rs` `into_svm` | Every PR scenario that reads events, with no limit | masked | Run the PR suite at the mainnet limit; assert each event-reading scenario's log size |
| P94 | **The Pyth gate refuses a partially verified price.** `priceIsFullyVerified` fails a price update below Full verification. Unchecked: the harness writes only Full updates (`tests/protocols/src/oracle.rs` `price_update` asserts it), and `pyth-gate.md` notes no run fails the step | S·TS | `clients/js/examples/protocols/pyth-fresh-price-gate.ts` | TS `protocol-semantics.test.ts` (the step's position only) | unchecked | PR: write a Partial update with its own offsets and expect `priceIsFullyVerified` |
| P92 | **Entries open before any call.** Compiled templates open every entry before their first step, and the verifier refuses an open with any INVOKE before it (P36). Before that rule a Rust-built template could call out before an open, with harm: it read field 0's raw bytes through a second slot, ran itself nested in the window, then opened the entry and wrote the read plus one, so the nested run's write was lost. That template is now refused at upload | S·V+TS | `verify.rs` `verify_open_registry`; `compiler.ts` `compile` (opens come first) | V `registry_rules`; M `fuzz/critic_lost_write.rs` `critic_a_raw_read_before_the_open_cannot_lose_a_nested_runs_write`; TS `compiler.test.ts` "opens every entry before the first step, in declaration order"; FV2's mutant `verify-registry-open-after-invoke` | tested | — |

## Mutation evidence

Each mutant deletes one check. "Survived" means the Mollusk and host suites (and, for the
critic's, the protocol suite) all passed without it. This review's mutants were run against the
Mollusk and host suites only, with clean SBF builds (one first attempt overflowed a stack frame
and was discarded).

| Check deleted | Property | Before this review | With its guard |
| --- | --- | --- | --- |
| Signer flag at run start | P44 | 1 protocol test fails (critic) | — |
| Writable flag at run start | P44 | R `an_account_that_is_not_the_named_entry_fails` fails | — |
| Address pin at run start | P44 | 2 protocol tests fail (critic) | — |
| Owner pin at run start | P44 | 1 Mollusk and 1 protocol test fail (critic) | — |
| Executable flag at run start | P45 | survived | F guard fails |
| Minimum data length at run start | P45 | survived | F guard fails |
| Checked ADD (wrapping instead) | P70 | 1 host test fails (critic) | — |
| The verifier's CPI privilege rule | P24 | 1 host test fails (critic) | — |
| Cancel's state check | P8 | survived | F guard fails |
| Write's state check | P8 | survived (the offset check rejects) | F guard fails |
| Finalize's `verify` call | P6 | survived | F guard fails |
| The run's slow-path state check | P9 | survived | F guard fails |
| Loop register restore | P49 | survived | F guard fails |
| The CPI borrow check for open entries | P59 | R `a_cpi_that_passes_an_open_entry_writable_fails_with_registry_reentry` fails | — |
| Return-data provenance | P55 | survived the Mollusk and host suites; the protocol suite asserts it | — |
| Loop-invariant CPI data (always reused) | P50 | 5 Mollusk tests fail | — |
| TypeScript operand table (`mulDiv`, `select`) | P42 | the replay accepts the miscompile (critic) | — |

The critic's tooling is on `claude/critic-tests` (`scripts/critic/`). This review's mutants are
the same kind of edit, applied and restored with `git checkout`; they were not committed.

## Next checks, by tool

These steer the fuzzing and verification work. The IDs are the properties each one covers.

- **cargo-fuzz:**
  - `parse` + `verify` (P39, P83);
  - `BallistaInstruction::parse` against an encoder (P14);
  - the executor on host, running generated programs, through a harness crate that compiles the
    program source as `certora/ballista-lib` does (P40, P49, P50);
  - u128 `mul_div` against a bignum (P77).
- **Kani:**
  - per-instruction typing preservation, `verify_single_instruction` then `execute_instruction`,
    the CB rule on host (P19, P20, P40);
  - `verify_cpi` keeps every record's flags within its slot's (P24);
  - `mul64`, `divlu` and `mul_div_u64` over every u64 (P77);
  - the cast, comparison and u128 cases that no rule covers (P71, P73, P74, P76, P78, P79);
  - `parse` and `parse_finalized` on payloads of up to 96 bytes (P10, P82, P84);
  - `validate_runtime_accounts` (P43);
  - `read_array` and `Preimage`'s offsets (P64, P65).
- **Mutation testing:** run the critic's `scripts/critic/run-mutant.sh` and this review's
  mutants in CI, so a check no test exercises shows up as a surviving mutant.
- **Protocol tests:** run at the mainnet log limit (P93), and add a Partial Pyth update (P94).
- **Proptest:**
  - extend `generate.rs` to INVOKE, PDA, data and byte reads, introspection and RETURN_DATA (P26,
    P40, P50);
  - input encode and decode round trips (P46);
  - mutating one header field at a time (P15);
  - a reference interpreter without the loop shortcuts (P49, P50).
- **Mollusk:**
  - a lifecycle state-machine fuzz: random create, begin, write, finalize, cancel and run, with
    random signers, offsets, accounts and payloads (P2, P4, P5, P11–P13, P69);
  - negative tests for the codes no test asserts: 6000–6007 and 6016 (P2, P4–P6, P14);
  - single-case tests for P53, the slot and row variants of P59, and P64;
  - exact codes instead of `is_err()` in M `one_shot_open_run_guards_and_privileges` and M
    chunked (F11).
- **Certora:**
  - fix F1;
  - widen the typing rules' error set (F2);
  - run `rule_u64_mul_div_is_exact`, and move it to `run.conf` once it proves.
- **TypeScript:**
  - a reject-side differential against the Rust verifier, and a register operand table checked
    against `verify.rs` rather than shared with the replay (P42);
  - the payload-hash check before signing (P87);
  - unskip F9's test once it's fixed (P91).
