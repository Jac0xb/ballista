# Why Ballista?

Most Solana automation starts as transaction-builder code. As it grows, the same account ordering,
guards, encoding, and multi-program sequence gets copied into bots, frontends, scripts, and
backends. The next step is often a custom program—even when the desired logic is finite and does
not need private state.

Ballista makes that orchestration a reusable on-chain artifact.

## What it removes

- Rebuilding identical instruction sequences in every caller.
- Shipping protocol-specific orchestration programs for finite workflows.
- Trusting an RPC preflight for conditions that can be checked atomically at execution time.
- Sending the template bytes on every invocation.
- Client-side loops that create oversized sets of top-level instructions.

## What it deliberately does not replace

Ballista is not a general smart-contract language. Use a dedicated program when you need custody,
PDA signing, private mutable state, protocol-defined accounting, permission policy, replay state,
or control flow that cannot be statically bounded.

| Requirement | Ballista | Dedicated program |
| --- | --- | --- |
| Reusable finite CPI sequence | Excellent fit | Works, but more code |
| Runtime account and clock guards | Built in | Custom implementation |
| One bounded batch range | Built in | Custom implementation |
| Protocol-owned state machine | No | Yes |
| Sign as a program PDA | No | Yes |
| Unbounded/dynamic looping | No | Possible, within compute |

## The security bargain

The language is intentionally small enough that finalization can prove termination, initialized
register use, account-index validity, CPI privilege containment, and worst-case CPI/data expansion.
Every `Run` still relies on the transaction signers and downstream programs for authorization.

::: tip A good heuristic
If the workflow can be drawn as a short list of guarded CPIs plus one bounded table of rows, it is a
strong Ballista candidate.
:::
