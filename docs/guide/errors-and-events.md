# Errors and events

How to read a failed run's error code and find the step that failed, and how to read what a run
hands out: the run event, logs of the template's own, and return data.
[Error codes](/reference/errors) lists every code.

## Decoding

A failed run returns a custom error code. Its low 16 bits are the error kind, and its high 16 bits
are a context that locates the failure: for most kinds, the program counter, which is the index of
the failing bytecode instruction. [Error codes](/reference/errors) lists the kinds, and the four
whose context is something else.

::: code-group

<<< @/../clients/js/examples/sdk/decode-errors.ts#decode [TypeScript]

<<< @/../clients/rust/examples/docs_errors.rs#decode [Rust]

:::

`explainRunError` names the step that failed. It uses the compiled template's source map, which
records the step and optional label that produced each compiled instruction. For
`AccountConstraintFailed` it names the account instead, and for `InvalidRunInputs` the input. Give a
step a label when its position alone would not make a failure clear, as the
[budgeted payroll](/guide/batching#carry-a-total-across-rows) labels its `withinBudget` check.

Both TypeScript decoders take a `bigint` as well as a `number`. Kit's RPC returns a simulation's
custom code as a `bigint`, though its type says `number`.

## Which program failed

A program the run calls can fail with its own code, and Ballista passes that code through
unchanged. A code outside Ballista's ranges is always another program's, and both decoders return
nothing for it: `undefined` in TypeScript, `None` in Rust. But Anchor programs number their errors
from 6000 too, so Jupiter's slippage error, 6001, is also Ballista's `InvalidTemplateAccount`.

The logs say which program failed. The program that failed logs a `Program <address> failed:` line,
and every caller up the stack repeats the error after it, so the first such line names it.

- `failedProgram(logs)` returns that line's program.
- `decodeBallistaFailure(code, logs)` decodes the code only when that program is Ballista, and
  `explainRunError(code, compiled, { logs })` explains it only then. Both also take the address of
  another Ballista deployment.
- All three return `undefined` when the logs name no failed program, as when they were cut off at
  Solana's log limit.

This simulates a run before it is sent, which needs no signature, and says why it would fail:

<<< @/../clients/js/examples/sdk/simulate-run.ts#simulate [TypeScript]

## The failure log line

When a run fails inside Ballista, the program writes one line to the transaction log before it
returns, using `sol_log_64`, which logs five numbers. For a failure in a step, they are the program
counter (the index of the failing instruction), the instruction's opcode (the number that identifies
its operation), its two operands `a` and `b` (the registers, or numbered value slots, that it
reads), and its destination register (the one it writes).

With that line you can find the failing instruction in a simulation's logs without decoding the
error. A step failure that carries its own context, such as `CpiAccountLimitExceeded`, logs the
error kind and that context in place of the operands. A failure found while checking accounts and
inputs, before the first step runs, logs `u64::MAX` in place of the program counter, followed by the
error kind and its context. The line is written only when a run fails, so successful runs pay
nothing for it.

## Run events

Set `emitEvent: true` on a template, or `builder.flags(PROGRAM_FLAG_EMIT_EVENT)` in Rust, and every
successful run ends by logging one run event with `sol_log_data`. The event appears in the
transaction's logs as a base64-encoded `Program data:` line, where indexers can read it.
`decodeRunEvent` reads one, as `ballista_sdk::decode_run_event` does in Rust, into:

- `iterations`: the batch rows the run processed. Passes of a `repeat` loop are not counted;
- `expanded`: the calls the run reached, counting a loop's calls once per row or pass;
- `executed`: which reached calls ran, as a bit mask: bit `n` is set if reached call `n` ran;
- `templateAddress`: the template that ran;
- `version`: the template's bytecode version, currently 1.

A call skipped by its `when` condition counts as reached, with its bit clear. So the event can tell
"the ATA (associated token account) already existed" apart from "the ATA was created" without
inspecting inner instructions. The event is off by default because it costs a few hundred compute
units, Solana's measure of execution cost. [Wire format](/reference/wire-format#run-event) has its
byte layout.

## Logs and return data

The run event's fields are fixed. To hand out values it computes, a template uses `step.emit`, which
logs a `Program data:` line of its own, or `step.setReturnData`. Their rules are in
[Output](/reference/language#output).

A `Program data:` line does not name the program that logged it; the `invoke` and `success` lines
around it do. `parseProgramData(logs)` follows them and returns each line with its program. Among
Ballista's lines, `decodeRunEvent` recognizes the run event, and every other line is a template's
`emit`, starting with its tag. Lines from the same call share an `invocation` number, so a
template's `emit` lines can be paired with its run event, which names the template.

<<< @/../clients/js/examples/sdk/simulate-run.ts#events [TypeScript]

The return data that simulation or the transaction's metadata reports names the program that set
it, so it proves only that Ballista set it, not which template ran:

<<< @/../clients/js/examples/sdk/simulate-run.ts#return-data [TypeScript]

A `Program return:` log line names the program whose call ended instead: after a run that sets no
return data, it can show a called program's bytes under Ballista's name.
