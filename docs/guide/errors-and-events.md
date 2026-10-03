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

A called program's error code passes through unchanged, and Anchor programs also count from 6000,
so read the logs to tell who failed: the first `Program <address> failed:` line names it.
`failedProgram(logs)` returns that program, and `decodeBallistaFailure` and `explainRunError` decode
the code only when it is Ballista's. This simulates a run and says why it would fail:

<<< @/../clients/js/examples/sdk/simulate-run.ts#simulate [TypeScript]

A failed run also writes one log line of five numbers: the program counter, the opcode, its two
operands and its destination register. That finds the failing instruction in a simulation's logs
without decoding the code.

## Run events and output {#run-events}

Set `emitEvent: true` and every successful run logs one run event: the template, the rows it
processed, and which calls ran (a call skipped by `when` shows as not run). For values the template
computes, use `step.emit`, which logs a tagged `Program data:` line, or `step.setReturnData`. Read
them back with `parseProgramData` and `decodeRunEvent`, or from the simulation's return data:

::: code-group

<<< @/../clients/js/examples/sdk/report-outputs.ts#template [Template]

<<< @/../clients/js/examples/sdk/simulate-run.ts#events [Read the logs]

<<< @/../clients/js/examples/sdk/simulate-run.ts#return-data [Read the return data]

:::

[Output](/reference/language#output) and [Wire format](/reference/wire-format#run-event) have the
details.
