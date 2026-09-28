import {
  data,
  expression,
  readWidth,
  step,
  type AccountReference,
  type Expression,
  type ReadType,
  type Step,
} from './schema.js';

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
/**
 * `Sysvar1nstructions1111111111111111111111111`. Declare an account pinned to this address to read
 * the transaction's instructions; every introspection expression names that account.
 */
export const INSTRUCTIONS_SYSVAR_ADDRESS_BYTES = Uint8Array.of(
  6, 167, 213, 23, 24, 123, 209, 102, 53, 218, 212, 4, 85, 253, 194, 192, 193, 36, 198, 143, 33, 86, 117,
  165, 219, 186, 203, 95, 8, 0, 0, 0,
);
/** `Ed25519SigVerify111111111111111111111111111`, the Ed25519 signature-verification precompile. */
export const ED25519_PROGRAM_ADDRESS_BYTES = Uint8Array.of(
  3, 125, 70, 214, 124, 147, 251, 190, 18, 249, 66, 143, 131, 141, 64, 255, 5, 112, 116, 73, 39, 244, 138,
  100, 252, 202, 112, 68, 128, 0, 0, 0,
);

export function assertPda(input: {
  account: AccountReference;
  program: AccountReference;
  seeds: Expression[];
  /**
   * The bump to derive with, such as `expression.input('bump')`. Supplying it derives the address
   * once instead of searching down from 255: about 1,900 compute units instead of about 4,850 for a
   * search three bumps deep. The assertion then proves only that the account derives from these
   * seeds and this bump, which need not be the canonical one. Leave it out, or write the canonical
   * bump into the template, when only the canonical address will do.
   */
  bump?: Expression;
  label?: string;
}): Step {
  return step.require(
    expression.equal(
      expression.accountField(input.account, 'key'),
      expression.pda(input.program, input.seeds, input.bump),
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
  bump?: Expression;
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
    ...(input.bump ? { bump: input.bump } : {}),
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

/**
 * The first 16 bytes of an Ed25519 precompile instruction with one signature, from agave
 * `precompiles/src/ed25519.rs`: a `u8` signature count and a padding byte, then the signature's
 * seven little-endian `u16` offsets. Byte offsets.
 */
const ED25519_HEADER = {
  signatureCount: 0,
  signatureInstructionIndex: 4,
  publicKeyOffset: 6,
  publicKeyInstructionIndex: 8,
  messageDataOffset: 10,
  messageDataSize: 12,
  messageInstructionIndex: 14,
} as const;

/** An instruction-index field of `u16::MAX`: the Ed25519 instruction's own data. */
const ED25519_THIS_INSTRUCTION = 0xffffn;

/**
 * A mask and the value the masked header must equal, from `[byte offset, width, value]` fields of
 * a little-endian header.
 */
function headerMask(fields: readonly (readonly [offset: number, width: number, value: bigint])[]) {
  let mask = 0n;
  let expected = 0n;
  for (const [offset, width, value] of fields) {
    const shift = BigInt(offset * 8);
    mask |= ((1n << BigInt(width * 8)) - 1n) << shift;
    expected |= value << shift;
  }
  return { mask, expected };
}

export interface Ed25519Signature {
  /**
   * Requirements that bind the precompile's verified signature to `signer` and to a message of
   * `messageLength` bytes, and the bindings `field` reads through. Put them before any step that
   * uses `field`: without them `field` would read unverified bytes, so it does not compile.
   */
  steps: Step[];
  /** The value at `offset` in the signed message. The whole read must lie inside the message. */
  field(offset: number, type: ReadType): Expression;
}

/**
 * Binds an Ed25519 signature that the transaction's precompile instruction verified to this
 * template's inputs.
 *
 * The Ed25519 program verifies its signatures as part of the transaction, so a transaction whose
 * signature is invalid fails and nothing the template did survives. What the precompile does not
 * say is whose signature it checked, or over which bytes. The steps returned here require that
 * instruction `index` is the Ed25519 program, holds exactly one signature, takes the signature,
 * the key and the message from its own data, was signed by `signer`, and signed exactly
 * `messageLength` bytes. `field` then reads the signed message.
 *
 * The count, the three instruction indexes and the message size all sit in the instruction's
 * first 16 bytes, so one masked `u128` comparison checks them together: five separate
 * comparisons would take a dozen more of the template's 64 registers.
 *
 * `name` prefixes the step labels and the two variables the steps bind, `<name>Instruction` and
 * `<name>Message`, so one template can check more than one signature. Every step is labeled, so a
 * failed run names the step: with no instruction before the run, an `index` of the one before it
 * underflows at `<name>InstructionIndex`.
 */
export function ed25519Signature(input: {
  sysvar: AccountReference;
  /** The Ed25519 instruction's index in the transaction, as a `u64`. */
  index: Expression;
  /**
   * The public key the signature must be by, as a `pubkey`. It must be a key the transaction's
   * builder cannot choose, such as a pinned key or the key of an account that must sign. With an
   * input, or the key of an account nothing constrains, the builder can sign with a key of their
   * own and the check proves nothing. This helper refuses an input, but cannot see an account's
   * constraints: those are the caller's to get right.
   */
  signer: Expression;
  messageLength: number;
  name?: string;
}): Ed25519Signature {
  const name = input.name ?? 'signature';
  if (!Number.isInteger(input.messageLength) || input.messageLength < 1 || input.messageLength > 0xffff) {
    throw new RangeError('messageLength must be from 1 to 65535 bytes');
  }
  if (input.signer.kind === 'input' || input.signer.kind === 'rowInput') {
    throw new TypeError(
      `signer must be a key the transaction's builder cannot choose, not the ${input.signer.kind} ${input.signer.name}`,
    );
  }
  const instruction = expression.variable(`${name}Instruction`);
  const message = expression.variable(`${name}Message`);
  const offsetField = (offset: number) => expression.instructionData(input.sysvar, instruction, offset, 'u16');
  const header = headerMask([
    [ED25519_HEADER.signatureCount, 1, 1n],
    [ED25519_HEADER.signatureInstructionIndex, 2, ED25519_THIS_INSTRUCTION],
    [ED25519_HEADER.publicKeyInstructionIndex, 2, ED25519_THIS_INSTRUCTION],
    [ED25519_HEADER.messageDataSize, 2, BigInt(input.messageLength)],
    [ED25519_HEADER.messageInstructionIndex, 2, ED25519_THIS_INSTRUCTION],
  ]);
  return {
    steps: [
      step.let(`${name}Instruction`, input.index, `${name}InstructionIndex`),
      step.require(
        expression.equal(
          expression.instructionProgram(input.sysvar, instruction),
          expression.pubkey(ED25519_PROGRAM_ADDRESS_BYTES),
        ),
        `${name}IsEd25519`,
      ),
      step.require(
        expression.equal(
          expression.bitAnd(expression.instructionData(input.sysvar, instruction, 0, 'u128'), expression.u128(header.mask)),
          expression.u128(header.expected),
        ),
        `${name}IsOneSelfContainedSignature`,
      ),
      step.require(
        expression.equal(
          expression.instructionData(input.sysvar, instruction, offsetField(ED25519_HEADER.publicKeyOffset), 'pubkey'),
          input.signer,
        ),
        `${name}IsBySigner`,
      ),
      step.let(`${name}Message`, offsetField(ED25519_HEADER.messageDataOffset), `${name}MessageOffset`),
    ],
    field(offset, type) {
      if (!Number.isInteger(offset) || offset < 0 || offset + readWidth[type] > input.messageLength) {
        throw new RangeError(`${type} at ${offset} does not lie inside the ${input.messageLength}-byte signed message`);
      }
      const at = offset === 0 ? message : expression.add(message, expression.u64(offset));
      return expression.instructionData(input.sysvar, instruction, at, type);
    },
  };
}
