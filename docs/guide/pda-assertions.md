# PDA and ATA assertions

These assertions check that an account the caller passed is the [PDA](/reference/glossary#pda) or
[ATA](/reference/glossary#ata) the template expects, and this page shows how to keep that cheap.
They prove how an address was derived; they don't let Ballista sign for it (see
[Signing](/guide/trust-model#signing)).

## Assert an associated token account

`assertAta` derives the ATA for `[owner, tokenProgram, mint]` with the canonical bump, and the run
fails unless `associatedTokenAccount` has that address. The Run tabs compute the same address off
chain.

::: code-group

<<< @/../clients/js/examples/docs/assert-recipient-ata.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/assert-recipient-ata.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#assert-recipient-ata [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#assert-recipient-ata [Rust · Run]

:::

## Assert an arbitrary PDA

`assertPda` fails the run unless `account` is the canonical PDA of `program`, which must have a
fixed `address`, for these seeds. Seeds are encoded by type, and a template can pass at most 15 of
up to 32 bytes each.

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

## Supply the bump

Searching for the canonical bump costs about 500 [compute units](/reference/glossary#compute-units)
per attempt. The caller can find it off chain for free and pass it to `assertPda` or `assertAta`:

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
  bump: expression.input('positionBump'), // computed off chain, 0 to 255
});
```

- **The trade-off.** A supplied bump proves only that the address comes from those seeds with that
  bump. A caller could pass a lower bump that also works, giving a valid but non-canonical PDA.
- **When that's fine.** For ATAs: the Associated Token Program only creates canonical ones.
  Otherwise leave `bump` out, or hard-code the canonical bump when every seed is a constant.
- **The saving.** With one constant seed, the search took 4,852 compute units and a supplied bump
  1,898. The search costs more the further the bump is below 255.
