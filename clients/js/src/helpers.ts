import { data, expression, step, type AccountReference, type Expression, type Step } from './schema.js';

/** `11111111111111111111111111111111` */
export const SYSTEM_PROGRAM_ADDRESS_BYTES = new Uint8Array(32);
/** `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` */
export const TOKEN_PROGRAM_ADDRESS_BYTES = Uint8Array.of(
  6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28, 180, 133, 237, 95, 91,
  55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
);
/** `ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL` */
export const ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES = Uint8Array.of(
  140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131, 11, 90, 19, 153, 218, 255, 16,
  132, 4, 142, 123, 216, 219, 233, 248, 89,
);

export function assertPda(input: {
  account: AccountReference;
  program: AccountReference;
  seeds: Expression[];
  label?: string;
}): Step {
  return step.require(
    expression.equal(
      expression.accountField(input.account, 'key'),
      expression.pda(input.program, input.seeds),
    ),
    input.label,
  );
}

export function assertAta(input: {
  associatedTokenAccount: AccountReference;
  owner: AccountReference;
  mint: AccountReference;
  tokenProgram: AccountReference;
  associatedTokenProgram: AccountReference;
  label?: string;
}): Step {
  return assertPda({
    account: input.associatedTokenAccount,
    program: input.associatedTokenProgram,
    seeds: [
      expression.accountField(input.owner, 'key'),
      expression.accountField(input.tokenProgram, 'key'),
      expression.accountField(input.mint, 'key'),
    ],
    ...(input.label ? { label: input.label } : {}),
  });
}

export const assertAssociatedTokenAccount = assertAta;

export function systemTransfer(input: {
  systemProgram: AccountReference;
  from: AccountReference;
  to: AccountReference;
  lamports: Expression;
  when?: Expression;
  label?: string;
}): Step {
  return step.invoke({
    program: input.systemProgram,
    programAddress: SYSTEM_PROGRAM_ADDRESS_BYTES,
    accounts: [
      { account: input.from, signer: true, writable: true },
      { account: input.to, signer: false, writable: true },
    ],
    data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', input.lamports)],
    ...(input.when ? { when: input.when } : {}),
    ...(input.label ? { label: input.label } : {}),
  });
}

export function tokenTransfer(input: {
  tokenProgram: AccountReference;
  source: AccountReference;
  destination: AccountReference;
  authority: AccountReference;
  amount: Expression;
  when?: Expression;
  label?: string;
}): Step {
  return step.invoke({
    program: input.tokenProgram,
    programAddress: TOKEN_PROGRAM_ADDRESS_BYTES,
    accounts: [
      { account: input.source, signer: false, writable: true },
      { account: input.destination, signer: false, writable: true },
      { account: input.authority, signer: true, writable: false },
    ],
    data: [data.literal(Uint8Array.of(3)), data.encode('u64', input.amount)],
    ...(input.when ? { when: input.when } : {}),
    ...(input.label ? { label: input.label } : {}),
  });
}

export function createAssociatedTokenAccount(input: {
  associatedTokenProgram: AccountReference;
  payer: AccountReference;
  associatedTokenAccount: AccountReference;
  owner: AccountReference;
  mint: AccountReference;
  systemProgram: AccountReference;
  tokenProgram: AccountReference;
  when?: Expression;
  label?: string;
}): Step {
  return step.invoke({
    program: input.associatedTokenProgram,
    programAddress: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
    accounts: [
      { account: input.payer, signer: true, writable: true },
      { account: input.associatedTokenAccount, signer: false, writable: true },
      { account: input.owner, signer: false, writable: false },
      { account: input.mint, signer: false, writable: false },
      { account: input.systemProgram, signer: false, writable: false },
      { account: input.tokenProgram, signer: false, writable: false },
    ],
    data: [],
    ...(input.when ? { when: input.when } : {}),
    ...(input.label ? { label: input.label } : {}),
  });
}

/** Uses ordinary ATA Create; Ballista's `isEmpty` guard provides the idempotent behavior. */
export function ensureAssociatedTokenAccount(input: {
  associatedTokenProgram: AccountReference;
  payer: AccountReference;
  associatedTokenAccount: AccountReference;
  owner: AccountReference;
  mint: AccountReference;
  systemProgram: AccountReference;
  tokenProgram: AccountReference;
  when?: Expression;
  label?: string;
}): Step {
  const isMissing = expression.accountField(input.associatedTokenAccount, 'isEmpty');
  const when = input.when ? expression.and(isMissing, input.when) : isMissing;
  return createAssociatedTokenAccount({ ...input, when });
}
