# Formal verification

What Ballista's formal verification covers today: the rules the prover is set up to prove, the
rules that are written but blocked, what is out of scope, and how to run the prover yourself.

A test checks a program on the inputs someone chose. A formal verifier checks a stated property,
called a rule, against every possible input, and either proves it or finds a counterexample.
Ballista has rules for the Certora Solana Prover, which analyzes the compiled program: the SBF
bytecode that Solana runs. The rules live in `certora/`, a separate Cargo workspace, so they do not
affect the program's own build. `certora/README.md` covers installation.

## Where things stand

- **Set up to prove: 14 rules**, listed in `certora/ballista-specs/run.conf`. They cover `u64`
  and `i64` arithmetic, comparisons, casts, error codes, and the template parser.
- **Blocked: 13 rules**, listed in `certora/ballista-specs/run-blocked.conf`. They cover `u128`
  arithmetic, account constraints, the template lifecycle, and type preservation. Limits in how
  the prover models memory stop it from proving them, so they are expected to fail. They are not
  proved.
- **In CI:** the prover runs only when the repository has a `CERTORAKEY` secret, so it is skipped
  on forks and on pull requests from them. It runs the rules in `run.conf` only. Without the
  secret, CI only checks that the rules compile and runs the tests of the helper crate described
  below.

## Why Ballista suits a prover

Ballista runs each template on a small interpreter inside the program. When a template is
finalized, the program checks every instruction in it once: each operation is known, each value
is set before it is read, and each operation receives values of the type it expects. A template
has at most one loop, with a fixed maximum number of rows. The program has no recursion, and in the
release build Ballista's own code uses `unsafe` only in three short wrappers around Solana system
calls.

The most important property is one that tests cannot fully establish: whatever the finalize-time
check accepts, the interpreter runs without an error caused by the template's structure, such as a
value of the wrong type. That property can be stated one instruction at a time, which lets a
prover check it for every possible instruction at once. The rules that state it are among the
blocked ones.

## Rules the prover is set up to prove

| Area | What the rules state |
| --- | --- |
| `u64` arithmetic | Add, subtract, multiply, divide, min, and max give the same result as Rust's checked operations for every pair of `u64` values, and overflow and division by zero are reported as errors |
| `i64` arithmetic | The same for `i64`, except division. For signed division, only the error cases are proved: division by zero, and the one quotient that overflows (`i64::MIN / -1`). The prover cannot follow the compiler's signed-division routine, so unit tests check the quotient instead |
| Mixed types | Arithmetic on mixed number types or on values that are not numbers fails with a type error |
| Comparisons | All six comparisons match Rust for `u64` and `i64`. Less than, greater than, and their variants reject booleans, public keys, bytes, and mixed number types |
| Casts | Conversions between `i64`, `u64`, and `u128` succeed exactly when the value fits in the target type, and converting a boolean fails with a type error |
| Error codes | Every error code decodes back to the kind and context it was built from. Errors raised while a template runs and errors raised by the finalize-time check use separate ranges of codes, so a decoder can tell them apart |
| Parser | For every template payload of up to 96 bytes, the parser checks the format marker and then the version before anything else, reports a payload shorter than its header as truncated, and on success returns sections whose sizes match the header and exactly fill the payload |

## Blocked rules

These rules are written, but the prover cannot prove them yet because of how it models the
program's memory. They are expected to fail.

| Area | What the rules would state |
| --- | --- |
| `u128` arithmetic | The checked-arithmetic rule above, for `u128` values |
| Account constraints | The account rules a template declares are enforced exactly: that an account signed the transaction (signer), may be changed (writable), is a program (executable), has a fixed address or owner, and holds a minimum amount of data, and that the number of accounts matches. Reading an account's address, owner, lamports (SOL balance), data length, or emptiness returns the account's real values |
| Template lifecycle | A finalized template rejects further writes and cannot be cancelled, a template that is still uploading never runs, and a run never writes to the template account |
| Type preservation | If every value has the type the finalize-time check recorded, and that check accepts an instruction, then running the instruction either succeeds or fails only because of the values involved (an overflow, a division by zero, a failed `require`, or a failed derivation of a program derived address), and its result has the recorded type |

`run-blocked.conf` also holds three diagnostic rules that isolate the memory problem. One, for
example, checks that eight bytes written one at a time read back as the expected 64-bit number.
They describe the prover, not Ballista.

## What is not covered

- Everything in the blocked rules above.
- What an invoked program does. The prover treats every CPI (a call into another program) as an
  opaque call with an arbitrary result, so the rules stop where the other program's behavior
  begins.
- Solana system calls, such as hashing, finding a program derived address (PDA), and reading the
  clock. The prover models them; it does not verify them.
- The finalize-time check of a whole template. No rule covers it as a whole. The type-preservation
  rules are meant to cover it one instruction at a time, and they are blocked.
- Whole-template execution. Even once the type-preservation rules pass, extending them from one
  instruction to a whole template is an argument (induction over the instructions), not a
  machine-checked proof.
- Turning an error code into its name. Ordinary tests in `ballista-common` cover that lookup.
- The pinocchio library, which the program is written with, and the Solana runtime underneath it.
  Both are trusted.

## How the rules connect to the program

The program crate has a `spec-api` feature for the rules. It makes the interpreter module public
so rules can call its functions directly, compiles in one helper that is otherwise test-only, and
stops the compiler from inlining two small byte-copy functions so the prover can treat them as
opaque. The release build never enables the feature, so none of this reaches the deployed program.
`certora/ballista-lib` compiles the same program source as a Rust library for the rules, because
adding a library target to the program's own crate would change the deployed binary.

`certora/cvlr-pinocchio` is a helper crate that builds accounts with arbitrary contents, laid out
the way pinocchio programs read them, so rules can call the program with any possible account. It
does for pinocchio what Certora's `cvlr-solana` does for programs built on `solana-program`, and it
contains nothing specific to Ballista.

## What the prover has found

Building the program with Certora's tools found two stack overflows before any rule ran. Ballista
is compiled for version 0 of Solana's bytecode (SBPF v0), which gives each function a fixed 4 KiB
stack frame. Certora's build tools report any function that needs more, which the standard
`cargo build-sbf` does not. Two functions did: the CPI path used 4,736 bytes and the return-data
read used 7,552 bytes. Both overflowed into the calling function's frame on every run and happened
to work. Both are fixed.

To catch a regression, run `cargo certora-sbf` after changing the interpreter. In CI it runs only
in the prover job, so it also depends on the `CERTORAKEY` secret.

## Running the prover

Install `certora-cli`, which provides `certoraSolanaProver`, and `cargo-certora-sbf`, which builds
the program with Certora's platform tools. Then set `CERTORAKEY` to a key from Certora.
`certora/README.md` has the exact commands. To build the program and run the rules the prover is
set up to prove:

```bash
cd certora/ballista-specs
cargo certora-sbf --tools-version v1.53
certoraSolanaProver run.conf
```

Use platform tools v1.53: older versions cannot build the program. To see the blocked rules fail,
run `certoraSolanaProver run-blocked.conf` in the same directory.

Without a key, you can still check that the rules compile, as CI does. From `certora/`, run
`cargo check -p ballista-specs --features rt` and `cargo test -p cvlr-pinocchio --features rt`.
