# Scope and design choices

What Ballista does and does not do, and the design choices behind that scope.

## What Ballista is {#product-statement}

Ballista stores a template on chain once and lets anyone run it later with new inputs. A template
is a bounded sequence of Solana program calls with checks between them, and a run either completes
every step or changes nothing. Ballista is not an automation network, a custody service that holds
funds, or a general-purpose programming language.

What it offers: callers stop rebuilding the same instructions for every transaction, and every run
applies the same checks. [What it costs](/guide/why-ballista#cost) gives the rent and the
[compute units](/reference/glossary#compute-units) it adds.

Every maximum on a template and a run is listed on [Limits](/reference/limits).

## Design choices {#deliberate-scope-cuts}

Terms such as register, CPI and PDA are defined in the [Glossary](/reference/glossary).

| Choice | Instead of | Why |
| --- | --- | --- |
| Flat tables of fixed-size records | A nested tree of commands | The program reads records in place and can check exact size bounds |
| Up to eight loops, over the rows the caller supplies or a counted number of passes, each with a maximum set by the template | Nested or unbounded loops | Batches and repeated steps stay possible, and every run is known to finish |
| Registers that exist only during one run, plus the registry entries a template declares | Arbitrary state, or storage shared between templates | Templates stay immutable, and state that outlives a run is limited to entries only its own template can change |
| SDK helpers that compile to ordinary CPIs | Protocol-specific logic in the program | The program does not depend on any particular protocol |
| Only the signatures the transaction already carries | Ballista signing a template's calls as its own PDA | Ballista holds no authority of its own; see [when it signs](/guide/trust-model#signing) |
| No scheduler or keeper rules | Built-in automation | Deciding who may run a template, and preventing repeat runs, are left to the programs it calls, or to the template itself through a registry: an allowlist, a counter or a nonce |
| Templates checked by the SDK as you write them, plus shared encoders and decoders | Generated clients as the main API | Named inputs and accounts, and clear errors before a transaction is built |

Named bindings (`let` and `snapshot`) are names for registers, not stored variables. They make
before-and-after checks around a CPI readable without adding state. A carried binding keeps its
value from one pass of a loop to the next, so a batch can enforce a total, but it never outlives
the transaction.

`assertPda` and `assertAta` derive a [PDA](/reference/glossary#pda) on chain, within the seed
limits on [Limits](/reference/limits). Without a bump, the derivation searches for the canonical
bump, so its compute cost varies. With a bump supplied, it derives once. Either way these checks
only compare addresses; see [when Ballista signs](/guide/trust-model#signing).

## Dependency policy

- The JavaScript package pins its dependencies to exact versions, recorded in `pnpm-lock.yaml`.
  Its Solana Kit peer dependency accepts versions from 8.2.0 up to, but not including, 9.
- Dependency install scripts are blocked by default; only `esbuild` is allowed to run one.
- A change to the wire format regenerates the shared example payloads in `fixtures/` with
  `pnpm fixtures`. The Rust verifier tests, the Mollusk tests (which run the program in a simulated
  Solana runtime), and the TypeScript tests all read the same files.
- Each release will be deployed at its own address with no upgrade authority; see
  [Deployments](/guide/trust-model#deployments). Stored templates cannot be upgraded; moving to a
  new version means uploading them again under the new deployment.
- The program's Rust dependencies are pinned to exact versions, built on Pinocchio 0.11 (a
  lightweight library for Solana programs), and checked with `cargo build-sbf`, the Solana program
  build tool.

## What the examples do not imply {#product-boundary-for-use-cases}

The [examples](/examples/) show what a template can express when a suitable program instruction and
permission model already exist. They do not mean Ballista can create a protocol operation that does
not exist, find a swap route, keep oracle prices fresh, sign for a user, or run itself later.
