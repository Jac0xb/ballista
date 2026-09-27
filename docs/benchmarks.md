# What a run costs

To run a template, a transaction sends one instruction to the Ballista program. Ballista reads the
template stored on chain and makes each program call in it, with the checks between them. This page
compares what that costs with sending the same calls yourself as **plain instructions**: ordinary
instructions listed directly in one transaction, with no Ballista in between.

Two costs are compared:

- **Compute units (CU).** Solana counts the work a program does in compute units. Every
  transaction has a compute limit, at most 1.4 million units, and a transaction that runs out
  fails.
- **Transaction bytes.** The size of the serialized transaction. A version 1 transaction can be at
  most 4,096 bytes; see [Transaction v1](/guide/transaction-v1).

## In short

- **Compute: a template always uses more.** A template that makes one SOL transfer uses 2,938
  compute units. The same transfer sent as a plain instruction uses 150. A 30-row payroll uses
  51,741 against 4,500.
- **Bytes: a template uses fewer once a batch is big enough.** Templates that make one or two calls
  are 54 to 107 bytes larger than the plain transaction. A simple batch of transfers is smaller
  from five or six rows, and a 30-row payroll is 430 bytes smaller.
- **Once per template: upload and rent.** Uploading a template is a separate step, done once, and
  every example here uploads in a single transaction. The account that stores the template must
  hold a minimum balance for its size, which Solana calls rent: 0.00183 to 0.00345 SOL for the
  examples here. A finished template cannot be closed, so that balance stays locked.

## Why a template uses more compute

In a plain transaction, each instruction goes straight to the program it names. With a template,
the transaction calls Ballista, and Ballista calls each program in turn. A call from one program to
another is a **cross-program invocation**, or CPI. Three costs come with that:

1. **Solana's CPI fee.** The runtime charges a flat 946 compute units for every CPI, before the
   called program does any work. (That is the figure in Agave 4.1, the validator version these
   measurements use.) Instructions sent directly in the transaction don't pay it.
2. **Ballista's work for each call.** Ballista reads the call's description from the template,
   looks up its accounts, assembles its data and hands it to the runtime. For a SOL transfer that
   is about 630 units.
3. **Fixed work for each run.** Before the first call, Ballista loads the template, checks the
   accounts you passed against what the template expects, and reads your inputs. That is 1,014
   units for the smallest possible template, plus a little for each account and input.

The called program's own work costs the same either way: the System Program spends 150 units on a
SOL transfer whether it arrives as a plain instruction or from Ballista. Put together, one SOL
transfer made from a template costs about 1,730 units: 946 for the CPI fee, 150 for the transfer
and about 630 for Ballista. Checks and arithmetic between calls add 80 to 210 units per step.

The table below shows whole runs, fixed cost included. A batch repeats the template's steps once for
each row of accounts, here once per recipient:

| Run | Template | Plain instructions |
| --- | ---: | ---: |
| Smallest possible template: one check, no calls | 1,014 | none |
| One SOL transfer | 2,938 | 150 |
| 8 SOL transfers in a batch | 14,989 | 1,200 |
| 30 SOL transfers in a batch | 51,741 | 4,500 |

Most of the difference is the CPI fee, and it is not specific to Ballista. Any program that calls
other programs pays it, including one you write yourself. In the 30-transfer batch the fee is
28,380 of the 47,241 extra units. What a template adds beyond the fee is the fixed cost of a run
and about 600 units per call. Templates that derive a PDA pay more. A PDA is an account address
computed from a program's ID and a few chosen values, called seeds, instead of from a key pair, and
finding one can take several attempts at 1,500 units each. [Where the compute goes](/cu-profile)
breaks down every step.

The overhead matters less when the called programs do more work. Creating an associated token
account costs 13,518 units as a plain instruction and 16,658 from the template in
[Conditional ATA setup](/examples/token-accounts#conditional-ata-setup): about 3,000 units more, on a
much larger total.

## Cost of each example {#example-cost-tables}

Each measured example in the guide and the [examples](/examples/) has a cost table on its own page.
The table below collects them, grouped by the last column. The columns:

- **CU per run** and **Plain CU**: compute units for one run of the template, and for the same
  calls sent as plain instructions.
- **Bytes per run** and **Plain bytes**: the size of each transaction.
- **Template rent**: the balance locked in the account that stores the template.
- **Without a program?**: whether plain instructions can get the same result without you
  deploying a program of your own.
  - **Yes, same guarantees**: plain instructions already do the job. A template gives you one
    instruction instead of many, and a fixed sequence, stored on chain and checked when it was
    uploaded, that anyone can run again.
  - **Yes, weaker guarantees**: you can send the instructions, but a check the template makes on
    chain, such as a minimum amount or an expected account owner, is left to whoever builds the
    transaction.
  - **No, needs a program**: no fixed list of instructions can do it, because the right action
    depends on something read while the transaction runs, such as a balance, a price or the time.
    For these rows the plain columns show the closest plain transaction: the same calls, with
    amounts fixed at signing and without the checks. It does not do the same job; it shows what
    the calls alone cost.

<!-- benchmark:summary -->

| Pattern | CU per run | Plain CU | Bytes per run | Plain bytes | Template rent | Without a program? |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| [Sweep above a reserve](/guide/runtime-values#sweep-above-a-reserve) | 3,437 | 150 | 283 | 220 | 0.00215 SOL | No, needs a program |
| [Forward the whole token balance](/guide/runtime-values#forward-the-whole-token-balance) | 3,350 | 76 | 308 | 250 | 0.00209 SOL | No, needs a program |
| [Repay exactly what is owed](/guide/runtime-values#repay-exactly-what-is-owed) | 3,252 | 150 | 308 | 220 | 0.00201 SOL | No, needs a program |
| [Split what arrived](/guide/runtime-values#split-what-arrived) | 5,754 | 300 | 324 | 270 | 0.00272 SOL | No, needs a program |
| [Claim only when there is something](/guide/conditional#claim-only-when-there-is-something) | 3,336 | 150 | 308 | 220 | 0.00209 SOL | No, needs a program |
| [Liquidate only when unhealthy](/guide/conditional#liquidate-only-when-unhealthy) | 3,435 | 150 | 316 | 220 | 0.00211 SOL | No, needs a program |
| [Top up only when low](/guide/conditional#top-up-only-when-low) | 3,376 | 150 | 291 | 220 | 0.00209 SOL | No, needs a program |
| [Initialize only if missing](/guide/conditional#initialize-only-if-missing) | 2,956 | 150 | 275 | 220 | 0.00189 SOL | No, needs a program |
| [Waterfall until the money runs out](/guide/loops#waterfall-until-the-money-runs-out) | 23,452 | 1,200 | 578 | 570 | 0.00258 SOL | No, needs a program |
| [Consolidate only the funded accounts](/guide/loops#consolidate-only-the-funded-accounts) | 18,611 | 608 | 539 | 586 | 0.00209 SOL | No, needs a program |
| [Crank only the ripe entries](/guide/loops#crank-only-the-ripe-entries) | 19,209 | 1,200 | 506 | 570 | 0.00213 SOL | No, needs a program |
| [Distribute a runtime pot pro rata](/guide/loops#distribute-a-runtime-pot-pro-rata) | 21,022 | 1,200 | 578 | 570 | 0.00242 SOL | No, needs a program |
| [Oracle price band](/guide/guardrails#oracle-price-band) | 4,081 | 150 | 324 | 220 | 0.00254 SOL | No, needs a program |
| [Maximum lamport spend](/guide/guardrails#maximum-lamport-spend) | 3,624 | 150 | 283 | 220 | 0.00232 SOL | No, needs a program |
| [Canonical position account](/guide/guardrails#canonical-position-account) | 5,630 | 150 | 285 | 220 | 0.00256 SOL | No, needs a program |
| [Time-gated governance execution](/examples/composition#time-gated-governance-execution) | 3,828 | 150 | 342 | 240 | 0.0023 SOL | No, needs a program |
| [Basis-point revenue split](/examples/payments#basis-point-revenue-split) | 5,729 | 300 | 324 | 270 | 0.00272 SOL | Yes, weaker guarantees |
| [Index-weighted rewards](/examples/payments#index-weighted-rewards) | 68,356 | 4,500 | 1,240 | 1,670 | 0.00224 SOL | Yes, weaker guarantees |
| [Deadline refund](/examples/payments#deadline-refund) | 3,480 | 150 | 291 | 220 | 0.00209 SOL | Yes, weaker guarantees |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | 4,097 | 150 | 291 | 220 | 0.00258 SOL | Yes, weaker guarantees |
| [Close empty token accounts](/examples/token-accounts#close-empty-token-accounts) | 34,064 | 1,888 | 803 | 842 | 0.00205 SOL | Yes, weaker guarantees |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | 3,780 | 76 | 316 | 250 | 0.00227 SOL | Yes, weaker guarantees |
| [Deadline and minimum output](/guide/guardrails#deadline-and-minimum-output) | 4,064 | 150 | 333 | 240 | 0.00248 SOL | Yes, weaker guarantees |
| [Pinned program and owner](/guide/guardrails#pinned-program-and-owner) | 2,959 | 150 | 283 | 220 | 0.00183 SOL | Yes, weaker guarantees |
| [Swap then deposit](/examples/composition#swap-then-deposit) | 5,783 | 300 | 385 | 278 | 0.00282 SOL | Yes, weaker guarantees |
| [Primary or fallback route](/examples/composition#primary-or-fallback-route) | 3,512 | 150 | 345 | 240 | 0.0023 SOL | Yes, weaker guarantees |
| [Bounded SOL payroll](/examples/payments#bounded-sol-payroll) | 51,741 | 4,500 | 1,240 | 1,670 | 0.00191 SOL | Yes, same guarantees |
| [Assert, create, then transfer](/examples/token-accounts#assert-create-then-transfer) | 191,260 | 123,752 | 1,007 | 1,122 | 0.00345 SOL | Yes, same guarantees |
| [Existing-account token payroll](/examples/token-accounts#existing-account-token-payroll) | 55,183 | 2,432 | 1,339 | 1,738 | 0.00195 SOL | Yes, same guarantees |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | 16,658 | 13,518 | 407 | 341 | 0.00224 SOL | Yes, same guarantees |
| [Claim then distribute](/examples/composition#claim-then-distribute) | 30,432 | 1,366 | 974 | 1,148 | 0.00258 SOL | Yes, same guarantees |
| [Bounded keeper crank](/examples/composition#bounded-keeper-crank) | 45,396 | 3,600 | 1,826 | 2,162 | 0.00194 SOL | Yes, same guarantees |

<!-- /benchmark -->

In the example names, a *lamport* is the smallest unit of SOL (one billionth of a SOL), and an
*ATA* (associated token account) is the standard token account for a given wallet and token.

## Where the bytes go

Transaction size rarely decides whether to use a template. The plain version of a 30-row payroll
is 1,670 bytes, well under the 4,096-byte limit. The template itself is stored on chain, so its
bytes are never part of a run's transaction. What changes is how each row is encoded.

A plain transaction repeats a whole instruction for every row: which program to call, which
accounts it uses, and its data. A template describes the call once, so each extra row adds only
the row's address and a one-byte reference to it, 33 bytes. The address has to be in the
transaction either way.

A run also names two accounts that a plain transaction does not need, the template account and
the Ballista program, at 32 bytes each. So with one row the template's transaction is larger: by
63 bytes for a SOL transfer and 66 for a token transfer. The savings start a few rows in.

<!-- benchmark:chart -->

<svg viewBox="0 0 720 320" role="img" aria-label="Transaction bytes saved against the number of batch rows" style="width:100%;height:auto;max-width:720px">
    <line x1="56" y1="241.4" x2="704" y2="241.4" stroke="currentColor" stroke-opacity="0.45" /><text x="48" y="245.4" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">0</text>
    <line x1="56" y1="189.0" x2="704" y2="189.0" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="193.0" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">100</text>
    <line x1="56" y1="136.6" x2="704" y2="136.6" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="140.6" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">200</text>
    <line x1="56" y1="84.1" x2="704" y2="84.1" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="88.1" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">300</text>
    <line x1="56" y1="31.7" x2="704" y2="31.7" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="35.7" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">400</text>
    <line x1="56" y1="16" x2="56" y2="276" stroke="currentColor" stroke-opacity="0.35" />
    <path d="M 56.0 274.4 L 76.9 265.5 L 97.8 256.6 L 118.7 247.7 L 139.6 238.8 L 160.5 229.9 L 181.4 221.0 L 202.3 212.0 L 223.2 203.1 L 244.1 194.2 L 265.0 185.3 L 285.9 176.4 L 306.8 167.5 L 327.7 158.6 L 348.6 149.7 L 369.5 140.8 L 390.5 131.8 L 411.4 122.9 L 432.3 114.0 L 453.2 105.1 L 474.1 96.2 L 495.0 87.3 L 515.9 78.4 L 536.8 69.5 L 557.7 60.6 L 578.6 51.6 L 599.5 42.7 L 620.4 33.8 L 641.3 24.9 L 662.2 16.0" fill="none" stroke="#2f6f4f" stroke-width="2.5" />
    <path d="M 56.0 276.0 L 76.9 268.1 L 97.8 260.3 L 118.7 252.4 L 139.6 244.5 L 160.5 236.7 L 181.4 228.8 L 202.3 221.0 L 223.2 213.1 L 244.1 205.2 L 265.0 197.4 L 285.9 189.5 L 306.8 181.6 L 327.7 173.8 L 348.6 165.9 L 369.5 158.1 L 390.5 150.2 L 411.4 142.3 L 432.3 134.5 L 453.2 126.6 L 474.1 118.7 L 495.0 110.9 L 515.9 103.0 L 536.8 95.2 L 557.7 87.3 L 578.6 79.4 L 599.5 71.6 L 620.4 63.7 L 641.3 55.8 L 662.2 48.0 L 683.1 40.1 L 704.0 32.3" fill="none" stroke="#8a5a2b" stroke-width="2.5" />
    <text x="56.0" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">1</text>
    <text x="139.6" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">5</text>
    <text x="244.1" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">10</text>
    <text x="348.6" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">15</text>
    <text x="453.2" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">20</text>
    <text x="557.7" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">25</text>
    <text x="662.2" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">30</text>
    <text x="380" y="314" text-anchor="middle" font-size="13" fill="currentColor" fill-opacity="0.8">Rows in the batch</text>
    <text x="14" y="146" text-anchor="middle" font-size="13" fill="currentColor" fill-opacity="0.8" transform="rotate(-90 14 146)">Transaction bytes saved</text>
    <line x1="68" y1="28" x2="92" y2="28" stroke="#2f6f4f" stroke-width="2.5" /><text x="100" y="32" font-size="13" fill="currentColor">SOL transfer</text>
    <line x1="68" y1="48" x2="92" y2="48" stroke="#8a5a2b" stroke-width="2.5" /><text x="100" y="52" font-size="13" fill="currentColor">Token transfer</text>
</svg>

Measured: a SOL transfer costs 33 bytes per row through Ballista against 50 plain, breaking even at 5 rows; a token transfer costs 33 bytes per row through Ballista against 48 plain, breaking even at 6 rows.

<!-- /benchmark -->

In a large batch, the number of accounts runs out before the bytes do. A version 1 transaction can
name at most 64 account addresses. A 30-row payroll run already names 34 of them, but uses only
1,240 of its 4,096 bytes.

## How the numbers are measured

Both versions run in [Mollusk](https://github.com/anza-xyz/mollusk), Anza's harness for testing
Solana programs, which executes them with the Agave 4.1 runtime. Each starts from the same accounts.
The template side runs the compiled Ballista program. The plain side runs each plain instruction
and adds up their compute units. The System Program is the runtime's built-in version, and the
Token and Associated Token programs are copies of the mainnet programs that come with Mollusk.
Transaction bytes come from building both transactions as version 1 transactions with Solana Kit
8.2, the JavaScript SDK.

Where an example calls another protocol, such as a swap or a lending market, the benchmark calls
the System Program instead: a transfer with extra bytes at the end of its data, which the System
Program ignores. The call from Ballista is real and costs what any call costs, but the protocol's
own work is missing from both columns.

`pnpm benchmarks` compiles every example, measures both transactions, runs both versions and
rewrites the tables. The results describe this program on this runtime version. They are not fee
quotes: simulate the exact transaction you plan to send.

## Memory

A run's memory use does not grow with the number of calls. Solana gives a program 32 KiB of heap
(memory it can reserve while it runs) by default, and the standard allocator never frees any of it
during an instruction. So Ballista reserves what it needs once per run and reuses it for every
call: room for the template's working values (at most 64, of up to 40 bytes each, plus one saved
copy while a batch runs) and one set of buffers for building calls. PDA seeds are assembled on the
stack, not the heap. The template is read where it is stored, not copied.

The test suite checks this with two templates that would run out of heap if each call reserved
its own memory: one makes 58 transfers with 1,000 bytes of data each, and one derives two PDAs on
each of 59 rows. Both fit in the default heap. The
[formal verification](/guide/formal-verification) build also checks that every function fits in
the 4 KiB stack frame Solana gives it. [Limits](/reference/limits) lists the hard limits.
