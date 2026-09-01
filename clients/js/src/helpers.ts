import { data, expression, step, type AccountReference, type Expression, type Step } from './schema.js';

export function systemTransfer(input: {
  systemProgram: AccountReference;
  from: AccountReference;
  to: AccountReference;
  lamports: Expression;
  when?: Expression;
}): Step {
  return step.invoke({
    program: input.systemProgram,
    accounts: [
      { account: input.from, signer: true, writable: true },
      { account: input.to, signer: false, writable: true },
    ],
    data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', input.lamports)],
    ...(input.when ? { when: input.when } : {}),
  });
}

export function tokenTransfer(input: {
  tokenProgram: AccountReference;
  source: AccountReference;
  destination: AccountReference;
  authority: AccountReference;
  amount: Expression;
  when?: Expression;
}): Step {
  return step.invoke({
    program: input.tokenProgram,
    accounts: [
      { account: input.source, signer: false, writable: true },
      { account: input.destination, signer: false, writable: true },
      { account: input.authority, signer: true, writable: false },
    ],
    data: [data.literal(Uint8Array.of(3)), data.encode('u64', input.amount)],
    ...(input.when ? { when: input.when } : {}),
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
}): Step {
  return step.invoke({
    program: input.associatedTokenProgram,
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
}): Step {
  const isMissing = expression.accountField(input.associatedTokenAccount, 'isEmpty');
  const when = input.when ? expression.and(isMissing, input.when) : isMissing;
  return createAssociatedTokenAccount({ ...input, when });
}
