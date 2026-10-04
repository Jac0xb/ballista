# Testing group expressions

Plan for verifying the account-group expressions once the other session has committed them. They
were in progress, uncommitted, in the `claude/review-fixes` worktree on 2026-10-03. The plan reuses
the verification infrastructure on `claude/verification`.

## What is being added (as observed, may change)

- **New opcodes:**
  - `GROUP_LENGTH` (78) gives the member count, as a `u64`.
  - `GROUP_ANY` (79) gives a `bool`: does any member match the filter.
  - `GROUP_COUNT` (80) gives how many members match, as a `u64`.
- **The filter** lives in the immediate, as `GroupFilter`:
  - an owner program `b`, or either of `b` and `c`;
  - 1–4 match segments, each a typed value at a data offset;
  - 0–4 except pubkeys;
  - a data-length floor.

  Member data is read in place, for the duration of the opcode only.
- **Verifier:** new error `InvalidAccountGroup` (6133) for structural rules. Match registers follow
  the usual type and initialisation rules.
- **Rust template compiler:** `ballista_sdk::template`, about 6,100 lines, including `reuse.rs`. It
  claims to produce the same bytes as the TypeScript compiler.

## Questions for the feature's author (settle before testing)

1. Does `GROUP_COUNT` count a duplicate member twice? A group can hold the same account twice,
   which is the aliasing issue already raised for other accounts.
2. Is a member shorter than the floor skipped, or is it an error?
3. What happens when a member is the template account, an open registry entry (borrow mark set),
   or an account an earlier CPI reallocated or closed?
4. What does a group operation cost in compute units, per member and per match? What is the worst
   case, with about 61 members and 4 matches?
5. Do the unused operands (`b`, `c` and the immediate on `GROUP_LENGTH`) have to be canonical?
   Today the verifier rejects some unused fields and ignores others.

## Highest risks

1. **Register-reuse liveness.** Filter match and except segments read registers when the opcode
   runs. Both compilers' reuse passes must count them as reads, and so must the shared operand
   table that the replay check uses. A miss reuses a register that the filter still needs.
2. **Executor bounds.** A member's data must never be read past its length: compare only bytes
   under the floor, and handle members shorter than the floor.
3. **Owner and except logic.** `b` OR `c`. An except excludes by key. A wrong combination lets a
   look-alike account count.
4. **Agreement between the two compilers.** The new Rust compiler must match the TypeScript one on
   every document, and both may share the carried-alias bug that the compiler fuzzer found.

## Checks, by tool

Each check names its pass bar. Every new check must also catch a deliberate mutant.

| Tool | Check | Pass bar |
| --- | --- | --- |
| Property list | Add properties for the three opcodes, the filter rules and the read-only guarantee | Each one marked tested, sampled or unchecked, with mutation evidence |
| Verifier fuzzing (`fuzz/`) | Wire model and reference checker learn the group rules. The structured generator emits group operations with random filters. Negative breaks cover: 0 or 5 matches, 5 excepts, a segment out of range, a floor below offset + width, a wrong kind, a wrong register type, an undeclared group | No disagreement with `verify`. Each verifier group rule, when deleted, is caught |
| Executor fuzzing (`fuzz-executor/`, Mollusk) | Groups get random members (see the list below). `model.rs` computes length, any and count independently | Exact match with the model. `ANY == (COUNT > 0)`, `COUNT <= LENGTH`. Mutants caught: floor off by one, owner check skipped, except skipped, wrong compare width, `b`/`c` as AND |
| Compiler fuzzing (`compiler-fuzz.test.ts`) | Group expressions inside conditions, selects, CPI data, return data and loops | The verifier accepts everything. Forced reuse matches the natural numbering in Mollusk, with filter registers live |
| Cross-compiler (new) | The same documents compiled by TypeScript and by `ballista_sdk::template` | Byte-identical on every generated document, not just the docs examples. Needs a document format both read: the TypeScript generator writes documents as JSON, and a Rust test compiles them |
| Kani (`kani/`) | `GroupFilter` encode and decode for every `u64`. `segment_range` never overflows. `verify_group_filter` never panics within a bound. The match comparison against a byte-slice reference, per type | Proofs with `kani::cover!`, bounds stated |
| Certora | Group operations never write account data. Result typing | Probably blocked by the model's lack of aliasing; record why |
| Compute units | Per-member and per-match cost, and the worst-case group | Ceilings added. The limits page says compute grows with group size |
| Protocol test | One realistic template against the mainnet snapshot. For example, count a holder's token accounts for a mint, filtered by owner and mint | Exact counts against the snapshot's accounts |
| Docs | TypeScript and Rust tabs for the group examples | Byte-identical through the new Rust compiler, in the docs-examples harness |

**The random members for executor fuzzing:**
- owners equal to `b`, to `c`, or to neither;
- data lengths of 0, the floor minus 1, the floor, and the floor plus 1;
- match values present and absent;
- excepts that hit and that miss;
- duplicates of declared accounts;
- the template account;
- an open registry entry;
- writable members;
- a member that an earlier probe CPI reallocated;
- empty groups and maximum-size groups.

## Critics

Run the four critic angles on the feature once the checks exist:
- **Threat:** spoofed matches, duplicates inflating counts, templates that treat a count as
  uniqueness.
- **Invariants:** per-member semantics and canonical encodings.
- **Tests:** generator reach into group shapes.
- **Formal:** what the proofs actually cover.

## When to run

Start once the feature is committed. Then:
1. Merge the feature branch and `claude/verification` together.
2. Run the compiler-agreement and register-liveness checks first: they need no executor changes.
3. Run executor fuzzing once the opcodes run on chain.
