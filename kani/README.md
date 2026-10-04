# Kani proofs

Bounded model checking of the Ballista program with [Kani](https://model-checking.github.io/kani/).
Each harness states a property for every input up to a stated bound, and Kani either proves it for
all of them or returns a counterexample. Use it where sampling (`common/tests`, the Mollusk suites)
cannot reach every case.

This directory is its own Cargo workspace, as `certora/` is, so the release build, its lock file and
its dependency policy are untouched. `ballista-lib` compiles the program's sources as an `rlib` with
`spec-api`, and `proofs` holds the harnesses, all behind `cfg(kani)`.

## Run

Kani 0.67: `cargo install --locked kani-verifier --version 0.67.0 && cargo kani setup`.

```bash
cd kani
cargo kani -p ballista-kani -Z stubbing -j                     # every harness, in parallel
cargo kani -p ballista-kani -Z stubbing --harness wire::       # one file's harnesses
cargo kani -p ballista-kani -Z stubbing --exact --harness math::pow10_is_exact_up_to_38_and_overflows_above
cargo kani -p ballista-kani -Z stubbing -j -Z unstable-options --harness-timeout 20m   # cap each one
```

`-Z stubbing` is needed by the harnesses that replace SHA-256, the curve check, the PDA search or
the CPI syscall; the others ignore it. Each harness's doc comment states what it proves and its
bound. Every harness with a branch to reach carries `kani::cover!` statements (all but
`mul_div_rejects_a_zero_divisor`, which is two unconditional assertions), and the cover counts
below show those branches are reached.

## What is proved

The tables give each harness's state at `bcb85f8`, the code this README describes: 80 passed, 16
**timed out** and 1 ran **out of memory**; none failed a check or on unwinding. Every harness ran at
that commit with 30 minutes of CPU time, 12 at a time on a 16-core laptop that other work was also
using (20 in one run, which was then interrupted, and the other 77 in a second), so a result's time
(Kani's `Verification Time`, wall clock) can exceed the CPU it used. The harness that ran out of
memory did so again when rerun alone. A harness that timed out or ran out of memory proves nothing.
**Covers** counts the harness's `kani::cover!` statements that were satisfied. **Last passed** is
the commit of the latest run in which the harness passed, or `never`.

### Arithmetic (`math.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `arithmetic_rejects_mismatched_operands` | mixed or non-numeric operands are `TypeMismatch` | none; `bytes` ≤ 4 | passed, 2.5 s | 2/2 | `bcb85f8` |
| `bitwise_operations_and_integer_type_rules` | `BIT_*` are bitwise; integer opcodes' type rules | none; `bytes` ≤ 2 | passed, 14 s | 2/2 | `bcb85f8` |
| `casts_preserve_the_value_or_fail` | casts keep the value or overflow, never truncate | none; `bytes` ≤ 2 | passed, 3.5 s | 5/5 | `bcb85f8` |
| `division_and_remainder_fail_exactly_on_the_documented_inputs` | `DIV`/`REM` fail exactly on a zero divisor and `i64::MIN / -1` | none | passed, 3.5 s | 3/3 | `bcb85f8` |
| `equality_only_comparisons_and_type_rules` | `bool`/`pubkey`/`bytes` compare by value; no ordering | none; `bytes` ≤ 3 | passed, 29 s | 4/4 | `bcb85f8` |
| `i64_add_sub_min_max_are_exact` | the same on `i64` | none | passed, 1.8 s | 3/3 | `bcb85f8` |
| `i64_div_truncates_toward_zero` | `DIV` on `i64` truncates toward zero | 12 bits per operand | passed, 33 s | 1/1 | `bcb85f8` |
| `i64_mul_is_exact` | `MUL` on `i64` is exact or overflows | 24 bits per operand | passed, 172 s | 2/2 | `bcb85f8` |
| `i64_remainder_takes_the_dividends_sign` | `REM` on `i64` takes the dividend's sign | 12 bits per operand | passed, 96 s | 1/1 | `bcb85f8` |
| `numeric_comparisons_match_the_numbers` | comparisons order numbers correctly | none | passed, 0.9 s | 4/4 | `bcb85f8` |
| `pow10_is_exact_up_to_38_and_overflows_above` | `POW10` is 10^e up to 38, overflow above | none | passed, 1.3 s | 3/3 | `bcb85f8` |
| `shifts_never_drop_a_set_bit_silently` | `SHL` fails exactly when a set bit would drop; `SHR` floors | none | passed, 14 s | 5/5 | `bcb85f8` |
| `u128_add_sub_min_max_are_exact` | the same on `u128`, against a limb-wise spec | none | passed, 9.2 s | 2/2 | `bcb85f8` |
| `u128_div_is_the_floor_quotient` | `DIV` on `u128` is the floor quotient | 12 bits per operand | passed, 20 s | 1/1 | `bcb85f8` |
| `u128_mul_is_exact` | `MUL` on `u128` is exact or overflows | 24 bits per operand | passed, 278 s | 2/2 | `bcb85f8` |
| `u128_remainder_is_exact` | `REM` on `u128` is the remainder | 12 bits per operand | passed, 71 s | 1/1 | `bcb85f8` |
| `u64_add_sub_min_max_are_exact` | `ADD SUB MIN MAX` on `u64` are exact or overflow | none | passed, 1.9 s | 3/3 | `bcb85f8` |
| `u64_div_is_the_floor_quotient` | `DIV` on `u64` is the floor quotient | 12 bits per operand | passed, 10 s | 1/1 | `bcb85f8` |
| `u64_mul_is_exact` | `MUL` on `u64` is exact or overflows | 24 bits per operand | passed, 5.3 s | 2/2 | `bcb85f8` |
| `u64_remainder_is_exact` | `REM` on `u64` is the remainder | 12 bits per operand | passed, 20 s | 1/1 | `bcb85f8` |

### Multiply-divide (`muldiv.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `full_product_is_the_exact_product` | the 256-bit product is exact | 12 bits per operand | passed, 7.8 s | 1/1 | `bcb85f8` |
| `full_product_never_overflows` | no intermediate overflows | none | passed, 0.2 s | 1/1 | `bcb85f8` |
| `mul_div_dispatches_on_operand_types` | `u64`×3 and `u128`×3 dispatch; others `TypeMismatch` | operands < 16, `bytes` ≤ 2 | passed, 335 s | 3/3 | `bcb85f8` |
| `mul_div_rejects_a_zero_divisor` | a zero divisor is `DivisionByZero` | none | passed, 0.1 s | – | `bcb85f8` |
| `mul_div_u128_contract_divisor_1` | `mul_div_u128` meets the contract, divisor 1 | that divisor, 12 bits per factor | passed, 6.8 s | 1/4 | `bcb85f8` |
| `mul_div_u128_contract_divisor_101_bits` | `mul_div_u128` meets the contract, divisor 2^100 + 12,345 | that divisor, 12 bits per factor | passed, 66 s | 3/4 | `bcb85f8` |
| `mul_div_u128_contract_divisor_10_18` | `mul_div_u128` meets the contract, divisor 10^18 | that divisor, 12 bits per factor | passed, 1225 s | 3/4 | `bcb85f8` |
| `mul_div_u128_contract_divisor_127_bits` | `mul_div_u128` meets the contract, divisor 2^126 + 1 | that divisor, 12 bits per factor | passed, 361 s | 3/4 | `bcb85f8` |
| `mul_div_u128_contract_divisor_2_64_minus_1` | `mul_div_u128` meets the contract, divisor 2^64 − 1 | that divisor, 12 bits per factor | **timed out** (30 min of CPU) | – | never |
| `mul_div_u128_contract_divisor_2_64_plus_1` | `mul_div_u128` meets the contract, divisor 2^64 + 1 | that divisor, 12 bits per factor | passed, 128 s | 3/4 | `bcb85f8` |
| `mul_div_u128_contract_divisor_3` | `mul_div_u128` meets the contract, divisor 3 | that divisor, 12 bits per factor | passed, 40 s | 3/4 | `bcb85f8` |
| `mul_div_u128_contract_divisor_max` | `mul_div_u128` meets the contract, divisor `u128::MAX` | that divisor, 12 bits per factor | **timed out** (30 min of CPU) | – | never |
| `mul_div_u64_contract_divisor_1` | `mul_div_u64` meets the contract, divisor 1 | that divisor, 12 bits per factor | passed, 1.8 s | 1/4 | `bcb85f8` |
| `mul_div_u64_contract_divisor_10` | `mul_div_u64` meets the contract, divisor 10 | that divisor, 12 bits per factor | passed, 63 s | 3/4 | `bcb85f8` |
| `mul_div_u64_contract_divisor_2_32_minus_1` | `mul_div_u64` meets the contract, divisor 2^32 − 1 | that divisor, 12 bits per factor | **timed out** (30 min of CPU) | – | never |
| `mul_div_u64_contract_divisor_2_32_plus_1` | `mul_div_u64` meets the contract, divisor 2^32 + 1 | that divisor, 12 bits per factor | passed, 290 s | 3/4 | `bcb85f8` |
| `mul_div_u64_contract_divisor_2_63` | `mul_div_u64` meets the contract, divisor 2^63 | that divisor, 12 bits per factor | passed, 42 s | 3/4 | `bcb85f8` |
| `mul_div_u64_contract_divisor_3` | `mul_div_u64` meets the contract, divisor 3 | that divisor, 12 bits per factor | passed, 68 s | 3/4 | `bcb85f8` |
| `mul_div_u64_contract_divisor_63_bits` | `mul_div_u64` meets the contract, divisor 2^62 + 12,345 | that divisor, 12 bits per factor | passed, 385 s | 3/4 | `bcb85f8` |
| `mul_div_u64_contract_divisor_max` | `mul_div_u64` meets the contract, divisor `u64::MAX` | that divisor, 12 bits per factor | **timed out** (30 min of CPU) | – | never |
| `mul_div_u64_contract_divisor_prime` | `mul_div_u64` meets the contract, divisor 1,000,000,007 | that divisor, 12 bits per factor | **timed out** (30 min of CPU) | – | never |

### Wire format (`wire.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `finalized_fast_path_agrees_with_the_full_parse` | the run path's template read agrees with the full one | ≤ 10,320 bytes | passed, 1834 s | 2/2 | `bcb85f8` |
| `instruction_data_parses_exactly_its_layout` | instruction data re-encodes byte for byte | ≤ 48 bytes | passed, 28 s | 5/5 | `bcb85f8` |
| `parse_finalized_agrees_with_parse` | the run path's parser returns the same sections | ≤ 10,240 bytes | passed, 194 s | 3/3 | `bcb85f8` |
| `parse_never_panics_and_sections_tile_the_payload` | `parse` never panics; sections tile the payload | ≤ 10,240 bytes | passed, 153 s | 5/5 | `bcb85f8` |
| `parse_refuses_payloads_over_the_cap` | over the cap is `PayloadTooLarge` | 10,241–10,248 bytes | passed, 30 s | 1/1 | `bcb85f8` |
| `reference_accessors_name_exactly_their_record` | `account_constraint`/`input_descriptor` name the right record | 4 fixed + 4 row records | passed, 4.3 s | 4/4 | `bcb85f8` |
| `template_account_split_is_exact` | template readers split header and payload exactly | ≤ 10,320 bytes | passed, 47 s | 3/3 | `bcb85f8` |

### Records (`records.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `immediates_decode_little_endian_and_ranges_round_trip` | immediates are little-endian; ranges round-trip | none | passed, 0.3 s | 1/1 | `bcb85f8` |
| `read_array_is_bounds_checked` | `read_array` reads in range or fails | data ≤ 40 bytes | passed, 0.5 s | 2/2 | `bcb85f8` |
| `read_i32_sign_extends` | `READ_I32` sign-extends | none | passed, 0.5 s | 1/1 | `bcb85f8` |
| `record_accessors_are_little_endian` | record accessors are little-endian | none | passed, 0.5 s | 2/2 | `bcb85f8` |
| `registry_immediates_round_trip_and_refuse_spare_bits` | registry immediates round-trip, refuse spare bits | none | passed, 0.3 s | 3/3 | `bcb85f8` |
| `typed_reads_match_the_verifiers_widths_and_types` | reads match `read_width`/`read_type`, exact values | data ≤ 40 bytes | passed, 41 s | 3/3 | `bcb85f8` |

### Registry (`registry.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `entries_are_never_read_as_templates` | no template reader accepts an entry | ≤ 24 field bytes | passed, 20 s | 1/1 | `bcb85f8` |
| `entry_headers_follow_the_layout_and_identify_their_entry` | entry headers follow the layout and are injective | none | passed, 5.3 s | 2/2 | `bcb85f8` |
| `open_accepts_exactly_the_documented_header_and_size` | `check_entry` accepts exactly that header and size | ≤ 16 field bytes | passed, 6.4 s | 3/3 | `bcb85f8` |
| `open_refuses_foreign_accounts_before_creating` | foreign or non-empty System accounts are refused | concrete owners, data ≤ 8 bytes | passed, 2.3 s | 1/1 | `bcb85f8` |
| `write_field_writes_exactly_its_field` | field writes touch exactly their bytes | ≤ 16 field bytes | passed, 30 s | 4/4 | `bcb85f8` |

### Encoding and inputs (`encoding.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `account_byte_reads_lend_exactly_the_range` | `READ_ACCOUNT_BYTES` lends exactly its range | data ≤ 16 bytes | passed, 8.5 s | 3/3 | `bcb85f8` |
| `input_decoding_follows_the_documented_encoding` | `parse_inputs` follows the documented encoding | 2 inputs, 10 bytes | **out of memory** | – | never |
| `register_segments_encode_their_exact_width_or_fail` | segments encode exact widths; never truncate | none; `bytes` ≤ 4 | passed, 94 s | 4/4 | `bcb85f8` |
| `run_inputs_are_the_fixed_inputs_then_one_row_per_iteration` | run inputs are fixed, then one row per pass | 2 fixed + 1 row input, 2 rows, 10 bytes | **timed out** (30 min of CPU) | – | never |
| `seed_buffers_never_write_past_their_end` | `FixedSink` never writes past its buffer | 8-byte buffer | passed, 1.1 s | 2/2 | `bcb85f8` |

### Executor (`executor.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `account_checks_enforce_the_schema_exactly` | `validate_runtime_accounts` accepts exactly the schema | ≤ 4 accounts | passed, 3183 s | 2/2 | `bcb85f8` |
| `consecutive_loops_start_from_the_registers_the_last_one_left` | a second loop starts where the first left off | 3 registers, ≤ 2 passes each | **timed out** (30 min of CPU) | – | never |
| `count_loops_carry_exactly_the_masked_registers` | a `REPEAT` runs as the carry/restore model | 3 registers, ≤ 2 passes | **timed out** (30 min of CPU) | – | never |
| `frame_account_fields` | the frame with real accounts: account fields | `World` + 2 accounts, data ≤ 80 bytes | passed, 139 s | 5/5 | `bcb85f8` |
| `frame_account_reads` | the frame with real accounts: typed and byte reads | `World` + 2 accounts, data ≤ 80 bytes | passed, 689 s | 10/10 | `bcb85f8` |
| `frame_add_sub_and_logic` | the same for `ADD SUB MIN MAX`, boolean logic, `SELECT`, `REQUIRE` | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 123 s | 9/9 | `bcb85f8` |
| `frame_casts` | the same for the three casts | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 29 s | 3/3 | `bcb85f8` |
| `frame_comparisons` | the same for the six comparisons | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 84 s | 6/6 | `bcb85f8` |
| `frame_loads_and_constants` | loads, constants, moves, `LOOP_INDEX` write at most `dst`; a failure writes nothing | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 107 s | 9/9 | `bcb85f8` |
| `frame_math` | the same for the register-only opcodes from `MUL_DIV` up, but the multiply-divides | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 118 s | 8/8 | `bcb85f8` |
| `frame_multiplies_and_divides` | the same for `MUL`, `DIV`, `MUL_DIV`, `MUL_DIV_CEIL` | `World`, 12-bit numbers | **timed out** (30 min of CPU) | – | never |
| `frame_outputs_calls_and_clock` | the same for the outputs, `INVOKE`, `RETURN_DATA`, the clock | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | **timed out** (30 min of CPU) | – | never |
| `frame_pdas_and_introspection` | the frame for the PDA opcodes (stubbed) and introspection | `World` + 2 accounts, data ≤ 80 bytes | **timed out** (30 min of CPU) | – | never |
| `frame_registry_fields` | only `WRITE_REGISTRY` writes account data, only to an open entry | `World` + 2 accounts, data ≤ 80 bytes | passed, 151 s | 2/3 | `bcb85f8` |
| `frame_unknown_opcodes_128_to_191` | opcode values 128–191 fail and write nothing | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 1949 s | 1/1 | `bcb85f8` |
| `frame_unknown_opcodes_192_to_255` | opcode values 192–255 fail and write nothing | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 1932 s | 1/1 | `bcb85f8` |
| `frame_unknown_opcodes_78_to_127` | opcode values 78–127 fail and write nothing | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 801 s | 1/1 | `bcb85f8` |
| `frame_unknown_opcodes_below_78` | opcodes 0 and 39, and a loop opcode run as one step, fail and write nothing | `World`: 4 registers, 3 inputs, `bytes` ≤ 4 | passed, 23 s | 1/1 | `bcb85f8` |
| `row_loops_carry_exactly_the_masked_registers` | a `FOREACH` runs as the carry/restore model | 3 registers, ≤ 2 rows | **timed out** (30 min of CPU) | – | never |
| `row_references_resolve_to_the_validated_account` | row accounts resolve to the validated slot | ≤ 3 rows | passed, 11 s | 1/1 | `bcb85f8` |

### Invocations (`invoke.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `batch_invocations_match_a_fresh_build_every_row` | the CPI cache sends what a fresh build sends; privilege ceiling | 2 rows, 2 account records | **timed out** (30 min of CPU) | – | never |

### Verifier soundness, one opcode at a time (`typing.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `verified_arithmetic_and_logic_keep_their_types` | the same for arithmetic, comparisons, boolean logic, casts | 4 registers | passed, 1590 s | 18/18 | `bcb85f8` |
| `verified_integer_opcodes_keep_their_types` | the same for `POW10` on, `BYTES_LEN` and the clock | 4 registers | passed, 377 s | 8/10 | `bcb85f8` |
| `verified_loads_constants_and_outputs_keep_their_types` | a verified load, constant, move, `SELECT`, `REQUIRE` or output keeps types or fails on values only | 4 registers | **timed out** (30 min of CPU) | – | never |

### Template lifecycle (`lifecycle.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `cancels_return_every_lamport_to_the_creator` | a cancel returns every lamport to the creator | 12-byte payload | passed, 20 s | 2/2 | `bcb85f8` |
| `chunk_writes_append_exactly_their_bytes` | a chunk write appends exactly its bytes, only for the creator | 12-byte payload | **timed out** (30 min of CPU) | – | never |
| `finalized_templates_never_change` | chunk writes, finalize and cancel fail on a finalized template | 12-byte payload | **timed out** (30 min of CPU) | – | never |
| `uploading_templates_never_run` | the run path refuses any non-finalized template | ≤ 10,320 bytes | passed, 288 s | 1/1 | `bcb85f8` |

### PDAs (`pda.rs`)

| Harness | Proves | Bound | Result | Covers | Last passed |
| --- | --- | --- | --- | --- | --- |
| `create_hashes_the_documented_preimage` | creation hashes the documented preimage | 2 seed shapes | passed, 43 s | 2/2 | `bcb85f8` |
| `search_hashes_the_documented_preimage` | the search hashes the documented preimage per bump | 3 seed shapes, ≤ 4 bumps | passed, 91 s | 2/2 | `bcb85f8` |
| `search_tries_every_bump_from_255_to_1` | bumps tried from 255 to 1, never 0 | no seeds | passed, 28 s | 1/1 | `bcb85f8` |
| `seed_count_limits_hold_at_full_capacity` | seed-count limits; writes in bounds at capacity | 16 × 32 bytes | passed, 20 s | 1/1 | `bcb85f8` |
| `template_and_registry_addresses_hash_their_documented_seeds` | template and registry seeds are as documented | ≤ 4 bumps | passed, 68 s | 2/2 | `bcb85f8` |

236 of the 255 covers were satisfied. The 19 that were not:

- **Expected.** `OPEN_REGISTRY` succeeding (`frame_registry_fields`: no template exists outside
  `run`), a verified `CLOCK_SLOT` or `CLOCK_TIMESTAMP` running
  (`verified_integer_opcodes_keep_their_types`: no clock off-chain), and three per width for
  divisor 1, where nothing rounds and a product wider than the result always overflows.
- **Too narrow a bound.** "Only the rounding overflows" in the other 10 `mul_div` contract harnesses
  that passed. Within 12 bits per factor no product has a floor that fits and a ceiling that
  overflows, so no passing harness reaches that error path.

## How the bounds work

- **None** means every value of every input the harness takes: all `u64`s, all 256 opcode values,
  every operand byte.
- **Windowed operands.** A proof that compares two different 64- or 128-bit multipliers or
  dividers does not finish at full width: that is multiplier equivalence, the classic hard case for
  bit-level solvers (the full-width runs timed out at 15 minutes). Those harnesses draw operands
  from `util::windowed_u64`/`windowed_u128`: three narrow windows of symbolic bits (at the bottom,
  straddling the middle limb boundary, at the top) with zeros between, so carries, overflow and
  every code path stay reachable while the solver multiplies narrow values.
- **Divisor classes.** `mul_div`'s division normalizes by the divisor's leading zeros; a symbolic
  divisor makes that shift symbolic and every later step a full divider. The `mul_div` harnesses
  therefore fix the divisor, one call per normalization class, and keep the factors symbolic.
- **Small programs.** Executor and typing harnesses build `ProgramView`s directly from symbolic
  records, so the solver's effort goes to the code under test rather than the parser, which
  `wire.rs` covers on its own up to the 10,240-byte cap.
- **Concrete opcodes.** A harness with a symbolic opcode makes the solver follow every handler at
  once. The executor and typing harnesses run each opcode as its own call. Across the executor's
  frame harnesses that is all 256 values: the 74 single-step opcodes once each, and the 182 others
  (0, 39, 78 to 255, and the two loop opcodes taken as a single step) in `frame_unknown_opcodes_*`.

## Model limits

Each limit below makes a claim narrower than a harness's name might suggest.

- **Accounts never alias.** Every account a harness builds is its own memory at its own address.
  A transaction can pass one account in two slots, which the program allows (see the trust
  model's [Aliased accounts](../docs/guide/trust-model.md#aliased-accounts)). No harness here
  models that, so none proves what happens when, say, the creator and the template, or two
  registry slots, are the same account.
  `open_accepts_exactly_the_documented_header_and_size` does prove that an entry already marked
  open is refused, which is how the program rejects one entry passed in two slots.
- **Off-chain no-ops.** Kani runs the host build, where syscalls are no-ops:
  - `RETURN_DATA` reads `fetch_return_data`, which returns nothing off-chain, so only its failure
    path is proved; its cover is unsatisfiable by design. The same holds for the clock
    (`CLOCK_SLOT`, `CLOCK_TIMESTAMP`).
  - `EMIT` and `SET_RETURN_DATA` encode their bytes for real, but the log and the return-data
    syscall do nothing.
  - A CPI does nothing off-chain. `invoke.rs` replaces it with a recorder and checks what each
    call receives; the callee itself is out of scope.
- **Stubs.** `pda.rs` replaces SHA-256 and the curve check with stubs that assert the exact
  preimage of every attempt and answer the curve check from a symbolic oracle: the first 3 attempts
  are symbolic and every later one is off the curve, so a search ends by its fourth bump, except in
  `search_tries_every_bump_from_255_to_1`, where every attempt is on the curve.
  `lifecycle.rs` and `frame_pdas_and_introspection` stub the PDA search to return any address, so
  their address checks are explored both ways but never against a real derivation. `lifecycle.rs`
  also stubs SHA-256 to return any digest, so the payload-hash check is explored both ways but
  never against a real hash.
- **Introspection.** The Instructions sysvar is parsed by pinocchio's unchecked parser, which
  trusts the runtime's layout by design. The harnesses only show the opcodes refuse any other
  account.
- **Programs are built, not parsed.** Executor and typing harnesses build `ProgramView`s from
  symbolic records with the counts `parse` would produce; `wire.rs` proves `parse` produces
  exactly such views.

## The test-only hook

`execute_program`, at the end of `programs/ballista/src/processor/execute.rs` behind the
`spec-api` feature, runs a whole program (loops included) against a register file the caller owns.
The loop and invocation harnesses need it to read the registers after a run. No build without
`spec-api` compiles it, and `cargo build-sbf` never enables that feature. It sits at the end of the
file because the binary embeds panic locations, line numbers included: there it moves no compiled
line, and the release build stays byte-identical.

## Not covered

- **Harnesses that did not finish.** At `bcb85f8` these properties are unproved, because their
  harnesses timed out or ran out of memory: the `mul_div` contract for 5 divisors (`u64`:
  1,000,000,007, 2^32 − 1, `u64::MAX`; `u128`: 2^64 − 1, `u128::MAX`); the executor's count, row
  and consecutive loops, and its frames for multiplies and divides, for outputs, calls and the
  clock, and for PDAs and introspection; the CPI cache; input decoding and run inputs; the typing
  step for loads, constants and outputs; and the lifecycle's chunk writes and finalized templates.
- **Full-width multiply and divide.** The values of `MUL`, `DIV` and `REM` on `u64`, `i64` and
  `u128`, and of `mul_div`, are proved on windowed operands (12 or 24 symbolic bits each, as the
  tables say), and `mul_div` only for the listed divisors, one per normalization class (five of
  those did not finish, above). A proof that compares the code's multiplier or divider with another
  one does not finish at full width, and a symbolic divisor makes the normalization shift symbolic.
  Absence of panics in `mul_div` is proved within those same bounds. Within them no `mul_div`
  product has a floor that fits and a ceiling that overflows, so that error path is not exercised.
  Which `DIV` and `REM` inputs fail, and with which error, is proved at full width, and so are add,
  subtract, compare, cast, shift and the bitwise operations.
- **Whole-program verification.** `verify` over a symbolic program does not finish (one symbolic
  instruction ran past 20 minutes). `typing.rs` proves the verifier and executor agree one opcode at a
  time instead, for the opcodes that touch only registers, inputs and outputs; the account, PDA, CPI,
  registry and introspection opcodes have frame proofs but no typing step.
- **What the runtime provides.** The return-data success path, the clock, the Instructions sysvar's
  parser, SHA-256, the curve check, each CPI's callee, and the registry's creation path, which calls
  the System program.
- **Size.** Loops of at most 2 passes over 3 registers, 2 batch rows for the CPI cache, 10 bytes of
  run inputs, 80 bytes of account data. Longer runs repeat the same code; the properties hold per pass
  and per instruction.
