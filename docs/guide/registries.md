# Remember state between runs

A run's values are gone when it ends. To keep something between runs, such as what a caller has
spent or who may run the template, a template declares a **registry**: a named set of fields. Each
**entry** is an account holding one copy of those fields, picked by a 32-byte **key** the template
computes; key it by the caller's address and each caller gets their own.

The first run to open an entry creates it, and the payer the template names pays its
[rent](/reference/glossary#rent), never returned: 1,097,280 lamports for 16 bytes of fields. Only
runs of its template can change an entry, and anyone can read it. This page builds a run counter, a
daily spending limit and an allowlist; the rules are under
[Registries](/reference/language#registries).

## Declare a registry

This template counts each caller's runs. It declares a registry, `runs`, with one field, `count`.
Every run opens the caller's own entry, creating it on the first run, and adds one to `count`.

::: code-group

<<< @/../clients/js/examples/docs/count-runs.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/docs_templates.rs#count-runs [Rust · Template]

:::

Two entries of one registry can't share a key in a run; see
[Opening an entry](/reference/language#opening-an-entry).

::: warning Don't let the caller choose the key
The caller sets every input. An entry keyed by an input is one the caller chooses, so a caller
could open a fresh entry, with a fresh limit, on every run. Key an entry that limits callers by a
signer's address, or leave the key out.
:::

## A daily limit per caller

Let each caller send at most 1 SOL at once, with the allowance refilling over about a day. The
`rateLimit` helper (`rate_limit` in Rust) returns the steps.

::: code-group

<<< @/../clients/js/examples/docs/daily-limit-per-caller.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/daily-limit-per-caller.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#daily-limit-per-caller [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#daily-limit-per-caller [Rust · Run]

:::

`rateLimit` refuses a `cap` or `refillPerSecond` that the caller could set, such as an input. It
can't see the key, so key the entry by a signer's address, as here, or leave the key out for one
limit that every caller shares. Give each `rateLimit` in a template its own `name`: it names the
requirement, `within<Name>`, so a failure says which limit was hit. The full rules are under
[Spending limits](/reference/language#spending-limits).

## An allowlist

This template lets only listed callers make a call. Its author keeps the list: each member has an
entry with an `ok` flag, keyed by the member's address. The author's runs add or remove members,
and anyone else's run fails unless their own flag is set. The guarded call is a System program
transfer, standing in for the call you'd protect, so the example runs as written.

::: code-group

<<< @/../clients/js/examples/docs/listed-callers-only.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/listed-callers-only.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#listed-callers-only [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#listed-callers-only [Rust · Run]

:::
