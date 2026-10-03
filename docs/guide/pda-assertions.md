# PDA and ATA assertions

This page shows how a template checks that an account the caller passed is the PDA or associated
token account it expects, and how to keep that check cheap.

A [PDA](/reference/glossary#pda) is derived from a program's address, a list of seeds, and one
extra seed byte called the bump, chosen so the address falls off the ed25519 curve and has no
private key. The canonical bump is the first value, counting down from 255, that gives such an
address, and programs normally create their accounts at that canonical address. An
[ATA](/reference/glossary#ata) is the token account at the Associated Token Program's PDA for a
given wallet, token program, and mint.

These assertions prove how an address was derived. They do not let Ballista sign for the account;
see [Signing](/guide/trust-model#signing).

## Assert an associated token account

::: code-group

<<< @/../clients/js/examples/docs/assert-recipient-ata.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/assert-recipient-ata.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_security.rs#assert-ata-template [Rust · Template]

<<< @/../clients/rust/examples/docs_security.rs#assert-ata-run [Rust · Run]

:::

`assertAta` derives an address from the seeds `[owner, tokenProgram, mint]` under the Associated
Token Program, searching for the canonical bump. The run fails unless the result equals the address
of the account passed as `associatedTokenAccount`. The Run tabs compute the same address off chain
and pass the accounts in the order the template declares them. In Rust,
`get_associated_token_address_with_program_id` from the `spl-associated-token-account` crate gives
the same result as `Pubkey::find_program_address`.

## Assert an arbitrary PDA

```ts
import { account, assertPda, expression } from '@jac0xb/ballista';

assertPda({
  account: account.fixed('position'),
  program: account.fixed('protocolProgram'),
  seeds: [
    expression.bytes(new TextEncoder().encode('position')),
    expression.accountField(account.fixed('owner'), 'key'),
    expression.input('positionId'), // u64, encoded little-endian
  ],
});
```

The program account must be declared `executable: true`, and the compiler also requires it to have
a fixed `address`. Each seed is encoded according to its type: a `u64` as 8 little-endian bytes, a
`pubkey` as its 32 bytes, and `bytes` as its contents. A template may supply at most 15 seeds, and
both the compiler and finalization (the one-time check that locks a template on chain) check that no
seed can be longer than 32 bytes. Solana allows 16 seeds in a derivation, and the last one is kept
for the bump.

## Supply the bump

Finding the canonical bump on chain means hashing with 255, then 254, and so on until the result is
off the curve. Solana charges 1,500 [compute units](/reference/glossary#compute-units) for every
attempt, and the number of attempts depends on the seeds. The caller can compute the bump off
chain for free, so a template can take it as an input and derive the address once:

```ts
import { account, assertPda, expression } from '@jac0xb/ballista';

assertPda({
  account: account.fixed('position'),
  program: account.fixed('protocolProgram'),
  seeds: [
    expression.bytes(new TextEncoder().encode('position')),
    expression.accountField(account.fixed('owner'), 'key'),
    expression.input('positionId'),
  ],
  bump: expression.input('positionBump'), // u64, 0 to 255
});
```

A supplied bump changes what the check proves. Without one, the assertion proves that the account
is the canonical PDA for those seeds. With one, it proves only that the account is derived from
those seeds with that bump. A bump above 255, or one that lands on the curve, fails the run, and a
bump that derives a different address fails the comparison. But a caller who passes a lower bump
that is also off the curve, together with the address it derives, passes the check. That address is
a valid PDA, but not the canonical one.

Whether that matters depends on the program that owns the PDA. For associated token accounts it is
harmless: the Associated Token Program only ever creates accounts at the canonical address, so no
token account can exist at a non-canonical ATA address.

When a template must insist on the canonical address, do not take the bump from the caller. Either
leave out `bump`, so the program searches for the canonical one, or, when every seed is fixed as you
write the template, compute the canonical bump yourself and write it into the template as a
constant, such as `bump: expression.u64(254)`.

`assertAta` takes the same `bump` option, and `expression.pda(program, seeds, bump)` is the
expression both helpers use. Inside a batch, give each row's ATA its own bump with a row input:

```ts
import {
  account,
  assertAta,
  defineTemplate,
  expression,
  step,
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
} from '@jac0xb/ballista';

const template = defineTemplate({
  accounts: {
    associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
    tokenProgram: {},
    mint: {},
  },
  batch: {
    maxIterations: 32,
    row: { recipient: {}, ata: { writable: true } },
    rowInputs: { ataBump: { type: 'u64' } },
  },
  steps: [
    step.forEach([
      assertAta({
        associatedTokenAccount: account.iteration('ata'),
        owner: account.iteration('recipient'),
        mint: account.fixed('mint'),
        tokenProgram: account.fixed('tokenProgram'),
        associatedTokenProgram: account.fixed('associatedTokenProgram'),
        bump: expression.rowInput('ataBump'),
      }),
    ]),
  ],
});
```

| Derivation | Compute units |
| --- | ---: |
| Canonical search, one constant seed | 4,852 |
| Supplied bump, one constant seed | 1,898 |

The search costs more the further the canonical bump is below 255; a supplied bump costs the same
every time.

::: warning Variable compute
Without a supplied bump, a derivation's cost depends on how many bumps the search tries. Measure
templates that derive many PDAs, especially inside a batch loop.
:::
