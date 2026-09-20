/**
 * Benchmark inputs for the example cookbook.
 *
 * Every example in `docs/examples/` is written out here as a complete template, compiled, and
 * measured for transaction size. The result is `fixtures/benchmarks.json`, which the Mollusk
 * benchmark in `tests/ballista` reads to run each template and record compute units, and which
 * `scripts/benchmark-tables.mjs` turns into the tables under each example.
 *
 * Where an example names a third-party protocol, the benchmark substitutes the System Program with
 * a padded transfer as the callee: bincode ignores trailing bytes, so the CPI is real and its cost
 * is representative, while the protocol's own work is out of scope. Accounts that a template reads
 * as protocol state are System-owned accounts with the field under test written at the documented
 * offset.
 *
 * Regenerate with `pnpm benchmarks`.
 */
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import {
  AccountRole,
  address,
  appendTransactionMessageInstructions,
  blockhash,
  createTransactionMessage,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
  type Address,
  type Instruction,
} from '@solana/kit';
import { describe, expect, test } from 'vitest';

import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  assertAta,
  assertPda,
  compileTemplate,
  data,
  defineTemplate,
  encodeRunInputs,
  ensureAssociatedTokenAccount,
  expression,
  step,
  systemTransfer,
  tokenTransfer,
  type RunInputValue,
  type Step,
  type Template,
} from './index.js';
import { buildKitRunInstruction, measureTransactionMessage } from './kit.js';

const FIXTURE_PATH = fileURLToPath(new URL('../../../fixtures/benchmarks.json', import.meta.url));

/** Roles the Mollusk benchmark knows how to materialize. */
type Role =
  | 'system-program'
  | 'token-program'
  | 'ata-program'
  | 'signer'
  | 'wallet'
  | 'mint'
  | 'source-tokens'
  | 'recipient-tokens'
  | 'empty-tokens'
  | 'recipient'
  | 'recipient-ata'
  | 'state-account'
  | 'position-pda';

interface BaselineAccount {
  index: number;
  signer: boolean;
  writable: boolean;
}

interface BaselineInstruction {
  program: Role;
  accounts: BaselineAccount[];
  data: string;
}

interface Baseline {
  /** What a caller would send without Ballista, and what that sequence does or does not guarantee. */
  note: string;
  verdict: 'equivalent' | 'weaker' | 'impossible';
  instructions: BaselineInstruction[];
}

interface BenchmarkCase {
  name: string;
  page: string;
  anchor: string;
  /** True when the callee is a padded System transfer standing in for a third-party protocol. */
  standIn?: boolean;
  template: Template;
  accounts: Record<string, Role>;
  rows?: Record<string, Role>[];
  inputs?: Record<string, RunInputValue>;
  baseline: Baseline;
}

const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
const systemPrograms = { systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES } } as const;
const tokenPrograms = { tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES } } as const;
const ataPrograms = {
  associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
} as const;

/** A System transfer's instruction data: discriminator 2 then a little-endian u64. */
function transferData(lamports: bigint): string {
  const bytes = new Uint8Array(12);
  bytes.set([2, 0, 0, 0]);
  new DataView(bytes.buffer).setBigUint64(4, lamports, true);
  return hex(bytes);
}

/** An SPL Token Transfer: discriminator 3 then a little-endian u64. */
function tokenTransferData(amount: bigint): string {
  const bytes = new Uint8Array(9);
  bytes[0] = 3;
  new DataView(bytes.buffer).setBigUint64(1, amount, true);
  return hex(bytes);
}

/** The padded System transfer that stands in for a third-party protocol instruction. */
function standInData(lamports: bigint, padding: number): string {
  return transferData(lamports) + '00'.repeat(padding);
}

const ROW_COUNT = 8;
const AMOUNT = 10_000n;

const cases: BenchmarkCase[] = [];

// ---------------------------------------------------------------- payments

cases.push({
  name: 'bounded-sol-payroll',
  page: 'payments',
  anchor: 'bounded-sol-payroll',
  template: defineTemplate({
    inputs: { amount: { type: 'u64' } },
    accounts: { ...systemPrograms, treasury: { signer: true, writable: true } },
    batch: { maxIterations: 30, minIterations: 1, row: { recipient: { writable: true } } },
    steps: [
      step.forEach([
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('treasury'),
          to: account.iteration('recipient'),
          lamports: expression.input('amount'),
        }),
      ]),
    ],
  }),
  accounts: { systemProgram: 'system-program', treasury: 'signer' },
  rows: Array.from({ length: ROW_COUNT }, () => ({ recipient: 'recipient' as Role })),
  inputs: { amount: AMOUNT },
  baseline: {
    verdict: 'equivalent',
    note: 'One System transfer per recipient does the same work.',
    instructions: Array.from({ length: ROW_COUNT }, (_, index) => ({
      program: 'system-program' as Role,
      accounts: [
        { index: 1, signer: true, writable: true },
        { index: 2 + index, signer: false, writable: true },
      ],
      data: transferData(AMOUNT),
    })),
  },
});

cases.push({
  name: 'basis-point-revenue-split',
  page: 'payments',
  anchor: 'basis-point-revenue-split',
  template: defineTemplate({
    inputs: { total: { type: 'u64' }, partnerBps: { type: 'u64' } },
    accounts: {
      ...systemPrograms,
      source: { signer: true, writable: true },
      partner: { writable: true },
      treasury: { writable: true },
    },
    steps: [
      step.require(expression.lessThanOrEqual(expression.input('partnerBps'), expression.u64(10_000))),
      step.let(
        'partnerAmount',
        expression.divide(
          expression.multiply(expression.input('total'), expression.input('partnerBps')),
          expression.u64(10_000),
        ),
      ),
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('source'),
        to: account.fixed('partner'),
        lamports: expression.variable('partnerAmount'),
      }),
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('source'),
        to: account.fixed('treasury'),
        lamports: expression.subtract(expression.input('total'), expression.variable('partnerAmount')),
      }),
    ],
  }),
  accounts: { systemProgram: 'system-program', source: 'signer', partner: 'recipient', treasury: 'recipient' },
  inputs: { total: 1_000_000n, partnerBps: 250n },
  baseline: {
    verdict: 'weaker',
    note: 'Two transfers with client-computed amounts settle the same way, but nothing on chain ties the two amounts to one total or bounds the share.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: transferData(25_000n),
      },
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: transferData(975_000n),
      },
    ],
  },
});

cases.push({
  name: 'index-weighted-rewards',
  page: 'payments',
  anchor: 'index-weighted-rewards',
  template: defineTemplate({
    inputs: { base: { type: 'u64' } },
    accounts: { ...systemPrograms, treasury: { signer: true, writable: true } },
    batch: { maxIterations: 30, minIterations: 1, row: { recipient: { writable: true } } },
    steps: [
      step.forEach([
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('treasury'),
          to: account.iteration('recipient'),
          lamports: expression.multiply(
            expression.add(expression.loopIndex(), expression.u64(1)),
            expression.input('base'),
          ),
        }),
      ]),
    ],
  }),
  accounts: { systemProgram: 'system-program', treasury: 'signer' },
  rows: Array.from({ length: ROW_COUNT }, () => ({ recipient: 'recipient' as Role })),
  inputs: { base: 1_000n },
  baseline: {
    verdict: 'weaker',
    note: 'Transfers with client-computed weights settle the same way; the weighting rule itself is not enforced on chain.',
    instructions: Array.from({ length: ROW_COUNT }, (_, index) => ({
      program: 'system-program' as Role,
      accounts: [
        { index: 1, signer: true, writable: true },
        { index: 2 + index, signer: false, writable: true },
      ],
      data: transferData(BigInt(index + 1) * 1_000n),
    })),
  },
});

cases.push({
  name: 'deadline-refund',
  page: 'payments',
  anchor: 'deadline-refund',
  template: defineTemplate({
    inputs: { refundAmount: { type: 'u64' }, deadline: { type: 'i64' } },
    accounts: {
      ...systemPrograms,
      escrowAuthority: { signer: true, writable: true },
      customer: { writable: true },
    },
    steps: [
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('escrowAuthority'),
        to: account.fixed('customer'),
        lamports: expression.input('refundAmount'),
        when: expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
      }),
    ],
  }),
  accounts: { systemProgram: 'system-program', escrowAuthority: 'signer', customer: 'recipient' },
  inputs: { refundAmount: AMOUNT, deadline: 9_000_000_000n },
  baseline: {
    verdict: 'weaker',
    note: 'A bare transfer refunds unconditionally; the deadline is only checked by whoever builds the transaction.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: transferData(AMOUNT),
      },
    ],
  },
});

cases.push({
  name: 'reserve-preserving-sweep',
  page: 'payments',
  anchor: 'reserve-preserving-sweep',
  template: defineTemplate({
    inputs: { reserve: { type: 'u64' }, cap: { type: 'u64' } },
    accounts: { ...systemPrograms, payer: { signer: true, writable: true }, vault: { writable: true } },
    steps: [
      step.snapshot('before', expression.accountField(account.fixed('payer'), 'lamports')),
      step.require(
        expression.greaterThanOrEqual(expression.snapshot('before'), expression.input('reserve')),
      ),
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('payer'),
        to: account.fixed('vault'),
        lamports: expression.min(
          expression.subtract(expression.snapshot('before'), expression.input('reserve')),
          expression.input('cap'),
        ),
      }),
      step.require(
        expression.greaterThanOrEqual(
          expression.accountField(account.fixed('payer'), 'lamports'),
          expression.input('reserve'),
        ),
      ),
    ],
  }),
  accounts: { systemProgram: 'system-program', payer: 'signer', vault: 'recipient' },
  inputs: { reserve: 1_000_000_000n, cap: 50_000n },
  baseline: {
    verdict: 'weaker',
    note: 'A transfer of a client-computed amount can be built, but no on-chain check proves the reserve survived.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: transferData(50_000n),
      },
    ],
  },
});

// ---------------------------------------------------------- token accounts

cases.push({
  name: 'assert-create-then-transfer',
  page: 'token-accounts',
  anchor: 'assert-create-then-transfer',
  template: defineTemplate({
    inputs: { amount: { type: 'u64' } },
    accounts: {
      ...ataPrograms,
      ...tokenPrograms,
      ...systemPrograms,
      mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
      payer: { signer: true, writable: true },
      authority: { signer: true },
      source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    },
    batch: {
      maxIterations: 8,
      minIterations: 1,
      row: { recipient: {}, destinationAta: { writable: true } },
    },
    steps: [
      step.forEach([
        assertAta({
          associatedTokenAccount: account.iteration('destinationAta'),
          owner: account.iteration('recipient'),
          mint: account.fixed('mint'),
          tokenProgram: account.fixed('tokenProgram'),
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
        }),
        ensureAssociatedTokenAccount({
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
          payer: account.fixed('payer'),
          associatedTokenAccount: account.iteration('destinationAta'),
          owner: account.iteration('recipient'),
          mint: account.fixed('mint'),
          systemProgram: account.fixed('systemProgram'),
          tokenProgram: account.fixed('tokenProgram'),
        }),
        tokenTransfer({
          tokenProgram: account.fixed('tokenProgram'),
          source: account.fixed('source'),
          destination: account.iteration('destinationAta'),
          authority: account.fixed('authority'),
          amount: expression.input('amount'),
        }),
      ]),
    ],
  }),
  accounts: {
    associatedTokenProgram: 'ata-program',
    tokenProgram: 'token-program',
    systemProgram: 'system-program',
    mint: 'mint',
    payer: 'signer',
    authority: 'signer',
    source: 'source-tokens',
  },
  rows: Array.from({ length: 4 }, () => ({ recipient: 'recipient' as Role, destinationAta: 'recipient-ata' as Role })),
  inputs: { amount: 1_000n },
  baseline: {
    verdict: 'equivalent',
    note: 'ATA CreateIdempotent then Transfer per recipient. The ATA program derives the address itself, so the guarantee matches.',
    instructions: Array.from({ length: 4 }, (_, index) => [
      {
        program: 'ata-program' as Role,
        accounts: [
          { index: 4, signer: true, writable: true },
          { index: 8 + index * 2, signer: false, writable: true },
          { index: 7 + index * 2, signer: false, writable: false },
          { index: 3, signer: false, writable: false },
          { index: 2, signer: false, writable: false },
          { index: 1, signer: false, writable: false },
        ],
        data: '01',
      },
      {
        program: 'token-program' as Role,
        accounts: [
          { index: 6, signer: false, writable: true },
          { index: 8 + index * 2, signer: false, writable: true },
          { index: 5, signer: true, writable: false },
        ],
        data: tokenTransferData(1_000n),
      },
    ]).flat(),
  },
});

cases.push({
  name: 'existing-account-token-payroll',
  page: 'token-accounts',
  anchor: 'existing-account-token-payroll',
  template: defineTemplate({
    inputs: { amount: { type: 'u64' } },
    accounts: {
      ...tokenPrograms,
      source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
      authority: { signer: true },
    },
    batch: {
      maxIterations: 32,
      minIterations: 1,
      row: { destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
    },
    steps: [
      step.forEach([
        tokenTransfer({
          tokenProgram: account.fixed('tokenProgram'),
          source: account.fixed('source'),
          destination: account.iteration('destination'),
          authority: account.fixed('authority'),
          amount: expression.input('amount'),
        }),
      ]),
    ],
  }),
  accounts: { tokenProgram: 'token-program', source: 'source-tokens', authority: 'signer' },
  rows: Array.from({ length: ROW_COUNT }, () => ({ destination: 'recipient-tokens' as Role })),
  inputs: { amount: 1_000n },
  baseline: {
    verdict: 'equivalent',
    note: 'One SPL Token transfer per destination does the same work.',
    instructions: Array.from({ length: ROW_COUNT }, (_, index) => ({
      program: 'token-program' as Role,
      accounts: [
        { index: 1, signer: false, writable: true },
        { index: 3 + index, signer: false, writable: true },
        { index: 2, signer: true, writable: false },
      ],
      data: tokenTransferData(1_000n),
    })),
  },
});

cases.push({
  name: 'conditional-ata-setup',
  page: 'token-accounts',
  anchor: 'conditional-ata-setup',
  template: defineTemplate({
    accounts: {
      ...ataPrograms,
      ...tokenPrograms,
      ...systemPrograms,
      mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
      payer: { signer: true, writable: true },
      wallet: {},
      ata: { writable: true },
    },
    steps: [
      ensureAssociatedTokenAccount({
        associatedTokenProgram: account.fixed('associatedTokenProgram'),
        payer: account.fixed('payer'),
        associatedTokenAccount: account.fixed('ata'),
        owner: account.fixed('wallet'),
        mint: account.fixed('mint'),
        systemProgram: account.fixed('systemProgram'),
        tokenProgram: account.fixed('tokenProgram'),
      }),
    ],
  }),
  accounts: {
    associatedTokenProgram: 'ata-program',
    tokenProgram: 'token-program',
    systemProgram: 'system-program',
    mint: 'mint',
    payer: 'signer',
    wallet: 'recipient',
    ata: 'recipient-ata',
  },
  baseline: {
    verdict: 'equivalent',
    note: 'ATA CreateIdempotent is the same behavior in one instruction.',
    instructions: [
      {
        program: 'ata-program',
        accounts: [
          { index: 4, signer: true, writable: true },
          { index: 6, signer: false, writable: true },
          { index: 5, signer: false, writable: false },
          { index: 3, signer: false, writable: false },
          { index: 2, signer: false, writable: false },
          { index: 1, signer: false, writable: false },
        ],
        data: '01',
      },
    ],
  },
});

cases.push({
  name: 'close-empty-token-accounts',
  page: 'token-accounts',
  anchor: 'close-empty-token-accounts',
  template: defineTemplate({
    accounts: {
      ...tokenPrograms,
      rentDestination: { writable: true },
      authority: { signer: true },
    },
    batch: {
      maxIterations: 16,
      minIterations: 1,
      row: { tokenAccount: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
    },
    steps: [
      step.forEach([
        step.invoke({
          program: account.fixed('tokenProgram'),
          programAddress: TOKEN_PROGRAM_ADDRESS_BYTES,
          accounts: [
            { account: account.iteration('tokenAccount'), writable: true, signer: false },
            { account: account.fixed('rentDestination'), writable: true, signer: false },
            { account: account.fixed('authority'), writable: false, signer: true },
          ],
          data: [data.literal(Uint8Array.of(9))],
          when: expression.equal(
            expression.accountData(account.iteration('tokenAccount'), 64, 'u64'),
            expression.u64(0),
          ),
        }),
      ]),
    ],
  }),
  accounts: { tokenProgram: 'token-program', rentDestination: 'recipient', authority: 'signer' },
  rows: Array.from({ length: 4 }, () => ({ tokenAccount: 'empty-tokens' as Role })),
  baseline: {
    verdict: 'weaker',
    note: 'CloseAccount per candidate works only while every candidate is empty: SPL Token rejects a funded account, which fails the whole transaction instead of skipping that row.',
    instructions: Array.from({ length: 4 }, (_, index) => ({
      program: 'token-program' as Role,
      accounts: [
        { index: 3 + index, signer: false, writable: true },
        { index: 1, signer: false, writable: true },
        { index: 2, signer: true, writable: false },
      ],
      data: '09',
    })),
  },
});

cases.push({
  name: 'exact-token-debit',
  page: 'token-accounts',
  anchor: 'exact-token-debit',
  template: defineTemplate({
    inputs: { amount: { type: 'u64' } },
    accounts: {
      ...tokenPrograms,
      source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
      destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
      authority: { signer: true },
    },
    steps: [
      step.snapshot('before', expression.accountData(account.fixed('source'), 64, 'u64')),
      tokenTransfer({
        tokenProgram: account.fixed('tokenProgram'),
        source: account.fixed('source'),
        destination: account.fixed('destination'),
        authority: account.fixed('authority'),
        amount: expression.input('amount'),
      }),
      step.require(
        expression.equal(
          expression.accountData(account.fixed('source'), 64, 'u64'),
          expression.subtract(expression.snapshot('before'), expression.input('amount')),
        ),
      ),
    ],
  }),
  accounts: {
    tokenProgram: 'token-program',
    source: 'source-tokens',
    destination: 'recipient-tokens',
    authority: 'signer',
  },
  inputs: { amount: 1_000n },
  baseline: {
    verdict: 'weaker',
    note: 'A bare transfer moves the tokens; nothing proves the source was debited by exactly that amount and no more.',
    instructions: [
      {
        program: 'token-program',
        accounts: [
          { index: 1, signer: false, writable: true },
          { index: 2, signer: false, writable: true },
          { index: 3, signer: true, writable: false },
        ],
        data: tokenTransferData(1_000n),
      },
    ],
  },
});

// -------------------------------------------------------------- guardrails

cases.push({
  name: 'deadline-and-minimum-output',
  standIn: true,
  page: 'guardrails',
  anchor: 'deadline-and-minimum-output',
  template: defineTemplate({
    inputs: {
      deadline: { type: 'i64' },
      quotedOut: { type: 'u64' },
      minimumOut: { type: 'u64' },
      routeData: { type: 'bytes', maxLength: 256 },
    },
    accounts: {
      swapProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      payer: { signer: true, writable: true },
      pool: { writable: true },
    },
    steps: [
      step.require(
        expression.and(
          expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
          expression.greaterThanOrEqual(expression.input('quotedOut'), expression.input('minimumOut')),
        ),
      ),
      step.invoke({
        program: account.fixed('swapProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.encode('bytes', expression.input('routeData'))],
      }),
    ],
  }),
  accounts: { swapProgram: 'system-program', payer: 'signer', pool: 'recipient' },
  inputs: {
    deadline: 9_000_000_000n,
    quotedOut: 1_000n,
    minimumOut: 900n,
    routeData: Uint8Array.from(Buffer.from(standInData(AMOUNT, 20), 'hex')),
  },
  baseline: {
    verdict: 'weaker',
    note: 'The route instruction alone. Deadline and minimum output are whatever the client checked before signing.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: standInData(AMOUNT, 20),
      },
    ],
  },
});

cases.push({
  name: 'pinned-program-and-owner',
  standIn: true,
  page: 'guardrails',
  anchor: 'pinned-program-and-owner',
  template: defineTemplate({
    inputs: { amount: { type: 'u64' } },
    accounts: {
      protocolProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      payer: { signer: true, writable: true },
      position: { writable: true, owner: SYSTEM_PROGRAM_ADDRESS_BYTES, minDataLength: 128 },
    },
    steps: [
      step.invoke({
        program: account.fixed('protocolProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('position'), signer: false, writable: true },
        ],
        data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.input('amount'))],
      }),
    ],
  }),
  accounts: { protocolProgram: 'system-program', payer: 'signer', position: 'state-account' },
  inputs: { amount: AMOUNT },
  baseline: {
    verdict: 'weaker',
    note: 'The same instruction with no schema: a substituted program ID or a wrong-owner account is accepted as far as the transaction is concerned.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: transferData(AMOUNT),
      },
    ],
  },
});

cases.push({
  name: 'oracle-price-band',
  standIn: true,
  page: 'guardrails',
  anchor: 'oracle-price-band',
  template: defineTemplate({
    inputs: { minimumPrice: { type: 'i64' }, maximumPrice: { type: 'i64' } },
    accounts: {
      oracle: { owner: SYSTEM_PROGRAM_ADDRESS_BYTES, minDataLength: 128 },
      protocolProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      payer: { signer: true, writable: true },
      pool: { writable: true },
    },
    steps: [
      step.require(
        expression.and(
          expression.greaterThanOrEqual(
            expression.accountData(account.fixed('oracle'), 8, 'i64'),
            expression.input('minimumPrice'),
          ),
          expression.lessThanOrEqual(
            expression.accountData(account.fixed('oracle'), 8, 'i64'),
            expression.input('maximumPrice'),
          ),
        ),
      ),
      step.invoke({
        program: account.fixed('protocolProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.u64(AMOUNT))],
      }),
    ],
  }),
  accounts: {
    oracle: 'state-account',
    protocolProgram: 'system-program',
    payer: 'signer',
    pool: 'recipient',
  },
  inputs: { minimumPrice: 1n, maximumPrice: 1_000_000n },
  baseline: {
    verdict: 'impossible',
    note: 'No instruction sequence reads an oracle account and refuses to continue. Enforcing a band on chain needs a program.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 2, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: transferData(AMOUNT),
      },
    ],
  },
});

cases.push({
  name: 'maximum-lamport-spend',
  standIn: true,
  page: 'guardrails',
  anchor: 'maximum-lamport-spend',
  template: defineTemplate({
    inputs: { maximumSpend: { type: 'u64' } },
    accounts: {
      protocolProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      payer: { signer: true, writable: true },
      pool: { writable: true },
    },
    steps: [
      step.snapshot('before', expression.accountField(account.fixed('payer'), 'lamports')),
      step.invoke({
        program: account.fixed('protocolProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.u64(AMOUNT))],
      }),
      step.require(
        expression.lessThanOrEqual(
          expression.subtract(
            expression.snapshot('before'),
            expression.accountField(account.fixed('payer'), 'lamports'),
          ),
          expression.input('maximumSpend'),
        ),
      ),
    ],
  }),
  accounts: { protocolProgram: 'system-program', payer: 'signer', pool: 'recipient' },
  inputs: { maximumSpend: 1_000_000n },
  baseline: {
    verdict: 'impossible',
    note: 'A transaction cannot compare a balance before and after one of its own instructions. Capping the debit on chain needs a program.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: transferData(AMOUNT),
      },
    ],
  },
});

cases.push({
  name: 'canonical-position-account',
  standIn: true,
  page: 'guardrails',
  anchor: 'canonical-position-account',
  template: defineTemplate({
    inputs: { positionId: { type: 'bytes', maxLength: 8 } },
    accounts: {
      protocolProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      owner: { signer: true, writable: true },
      position: { writable: true, owner: SYSTEM_PROGRAM_ADDRESS_BYTES, minDataLength: 128 },
    },
    steps: [
      assertPda({
        account: account.fixed('position'),
        program: account.fixed('protocolProgram'),
        seeds: [
          expression.bytes(new TextEncoder().encode('position')),
          expression.accountField(account.fixed('owner'), 'key'),
          expression.input('positionId'),
        ],
      }),
      step.invoke({
        program: account.fixed('protocolProgram'),
        accounts: [
          { account: account.fixed('owner'), signer: true, writable: true },
          { account: account.fixed('position'), signer: false, writable: true },
        ],
        data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.u64(AMOUNT))],
      }),
    ],
  }),
  accounts: { protocolProgram: 'system-program', owner: 'signer', position: 'position-pda' },
  inputs: { positionId: Uint8Array.of(7, 0, 0, 0, 0, 0, 0, 0) },
  baseline: {
    verdict: 'impossible',
    note: 'A transaction cannot derive a PDA and compare it to a supplied account. Rejecting a substituted account on chain needs a program.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 1, signer: true, writable: true },
          { index: 2, signer: false, writable: true },
        ],
        data: transferData(AMOUNT),
      },
    ],
  },
});

// ------------------------------------------------------------- composition

cases.push({
  name: 'swap-then-deposit',
  standIn: true,
  page: 'composition',
  anchor: 'swap-then-deposit',
  template: defineTemplate({
    inputs: {
      minimumOut: { type: 'u64' },
      swapData: { type: 'bytes', maxLength: 256 },
      depositData: { type: 'bytes', maxLength: 256 },
    },
    accounts: {
      swapProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      vaultProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      payer: { signer: true, writable: true },
      pool: { writable: true },
      receivedTokens: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    },
    steps: [
      step.snapshot('before', expression.accountData(account.fixed('receivedTokens'), 64, 'u64')),
      step.invoke({
        program: account.fixed('swapProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.encode('bytes', expression.input('swapData'))],
      }),
      step.require(
        expression.greaterThanOrEqual(
          expression.subtract(
            expression.accountData(account.fixed('receivedTokens'), 64, 'u64'),
            expression.snapshot('before'),
          ),
          expression.input('minimumOut'),
        ),
      ),
      step.invoke({
        program: account.fixed('vaultProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.encode('bytes', expression.input('depositData'))],
      }),
    ],
  }),
  accounts: {
    swapProgram: 'system-program',
    vaultProgram: 'system-program',
    payer: 'signer',
    pool: 'recipient',
    receivedTokens: 'source-tokens',
  },
  inputs: {
    minimumOut: 0n,
    swapData: Uint8Array.from(Buffer.from(standInData(AMOUNT, 20), 'hex')),
    depositData: Uint8Array.from(Buffer.from(standInData(AMOUNT, 20), 'hex')),
  },
  baseline: {
    verdict: 'weaker',
    note: 'Both instructions can be sent back to back, but the intermediate token delta is never checked, so a bad fill still deposits.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 2, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: standInData(AMOUNT, 20),
      },
      {
        program: 'system-program',
        accounts: [
          { index: 2, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: standInData(AMOUNT, 20),
      },
    ],
  },
});

cases.push({
  name: 'claim-then-distribute',
  standIn: true,
  page: 'composition',
  anchor: 'claim-then-distribute',
  template: defineTemplate({
    inputs: { amountPerRecipient: { type: 'u64' } },
    accounts: {
      rewardsProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      ...tokenPrograms,
      claimer: { signer: true, writable: true },
      pool: { writable: true },
      treasuryTokens: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
      authority: { signer: true },
    },
    batch: {
      maxIterations: 16,
      minIterations: 1,
      row: { recipientTokens: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
    },
    steps: [
      step.invoke({
        program: account.fixed('rewardsProgram'),
        accounts: [
          { account: account.fixed('claimer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.u64(AMOUNT))],
      }),
      step.forEach([
        tokenTransfer({
          tokenProgram: account.fixed('tokenProgram'),
          source: account.fixed('treasuryTokens'),
          destination: account.iteration('recipientTokens'),
          authority: account.fixed('authority'),
          amount: expression.input('amountPerRecipient'),
        }),
      ]),
    ],
  }),
  accounts: {
    rewardsProgram: 'system-program',
    tokenProgram: 'token-program',
    claimer: 'signer',
    pool: 'recipient',
    treasuryTokens: 'source-tokens',
    authority: 'signer',
  },
  rows: Array.from({ length: ROW_COUNT }, () => ({ recipientTokens: 'recipient-tokens' as Role })),
  inputs: { amountPerRecipient: 1_000n },
  baseline: {
    verdict: 'equivalent',
    note: 'A claim instruction followed by one token transfer per recipient does the same work.',
    instructions: [
      {
        program: 'system-program' as Role,
        accounts: [
          { index: 2, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: transferData(AMOUNT),
      },
      ...Array.from({ length: ROW_COUNT }, (_, index) => ({
        program: 'token-program' as Role,
        accounts: [
          { index: 4, signer: false, writable: true },
          { index: 6 + index, signer: false, writable: true },
          { index: 5, signer: true, writable: false },
        ],
        data: tokenTransferData(1_000n),
      })),
    ],
  },
});

cases.push({
  name: 'primary-or-fallback-route',
  standIn: true,
  page: 'composition',
  anchor: 'primary-or-fallback-route',
  template: defineTemplate({
    inputs: {
      usePrimary: { type: 'bool' },
      primaryData: { type: 'bytes', maxLength: 256 },
      fallbackData: { type: 'bytes', maxLength: 256 },
    },
    accounts: {
      primaryProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      fallbackProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      payer: { signer: true, writable: true },
      pool: { writable: true },
    },
    steps: [
      step.invoke({
        program: account.fixed('primaryProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.encode('bytes', expression.input('primaryData'))],
        when: expression.input('usePrimary'),
      }),
      step.invoke({
        program: account.fixed('fallbackProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('pool'), signer: false, writable: true },
        ],
        data: [data.encode('bytes', expression.input('fallbackData'))],
        when: expression.not(expression.input('usePrimary')),
      }),
    ],
  }),
  accounts: {
    primaryProgram: 'system-program',
    fallbackProgram: 'system-program',
    payer: 'signer',
    pool: 'recipient',
  },
  inputs: {
    usePrimary: true,
    primaryData: Uint8Array.from(Buffer.from(standInData(AMOUNT, 20), 'hex')),
    fallbackData: Uint8Array.from(Buffer.from(standInData(AMOUNT, 20), 'hex')),
  },
  baseline: {
    verdict: 'weaker',
    note: 'The client picks a route and sends that one instruction. The choice is made before signing, not from state at execution time.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 2, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: standInData(AMOUNT, 20),
      },
    ],
  },
});

cases.push({
  name: 'time-gated-governance-execution',
  standIn: true,
  page: 'composition',
  anchor: 'time-gated-governance-execution',
  template: defineTemplate({
    inputs: { executeData: { type: 'bytes', maxLength: 256 } },
    accounts: {
      governanceProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      proposal: { owner: SYSTEM_PROGRAM_ADDRESS_BYTES, minDataLength: 128 },
      payer: { signer: true, writable: true },
      target: { writable: true },
    },
    steps: [
      step.require(
        expression.and(
          expression.accountData(account.fixed('proposal'), 0, 'bool'),
          expression.greaterThanOrEqual(
            expression.clockUnixTimestamp(),
            expression.accountData(account.fixed('proposal'), 8, 'i64'),
          ),
        ),
      ),
      step.invoke({
        program: account.fixed('governanceProgram'),
        accounts: [
          { account: account.fixed('payer'), signer: true, writable: true },
          { account: account.fixed('target'), signer: false, writable: true },
        ],
        data: [data.encode('bytes', expression.input('executeData'))],
      }),
    ],
  }),
  accounts: {
    governanceProgram: 'system-program',
    proposal: 'state-account',
    payer: 'signer',
    target: 'recipient',
  },
  inputs: { executeData: Uint8Array.from(Buffer.from(standInData(AMOUNT, 20), 'hex')) },
  baseline: {
    verdict: 'impossible',
    note: 'A transaction cannot read a proposal flag and a timestamp and refuse to execute. Gating on chain needs a program.',
    instructions: [
      {
        program: 'system-program',
        accounts: [
          { index: 2, signer: true, writable: true },
          { index: 3, signer: false, writable: true },
        ],
        data: standInData(AMOUNT, 20),
      },
    ],
  },
});

cases.push({
  name: 'bounded-keeper-crank',
  standIn: true,
  page: 'composition',
  anchor: 'bounded-keeper-crank',
  template: defineTemplate({
    accounts: {
      protocolProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      keeper: { signer: true, writable: true },
    },
    batch: {
      maxIterations: 24,
      minIterations: 1,
      row: { market: { writable: true }, queue: { writable: true } },
    },
    steps: [
      step.forEach([
        step.invoke({
          program: account.fixed('protocolProgram'),
          accounts: [
            { account: account.fixed('keeper'), signer: true, writable: true },
            { account: account.iteration('market'), writable: true, signer: false },
          ],
          data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.u64(1_000n))],
        }),
      ]),
    ],
  }),
  accounts: { protocolProgram: 'system-program', keeper: 'signer' },
  rows: Array.from({ length: 4 }, () => ({ market: 'recipient' as Role, queue: 'recipient' as Role })),
  baseline: {
    verdict: 'equivalent',
    note: 'One crank instruction per row does the same work.',
    instructions: Array.from({ length: 4 }, (_, index) => ({
      program: 'system-program' as Role,
      accounts: [
        { index: 1, signer: true, writable: true },
        { index: 2 + index * 2, signer: false, writable: true },
      ],
      data: transferData(1_000n),
    })),
  },
});

// ------------------------------------------------------------- measurement

const PLACEHOLDER = (index: number): Address => {
  const bytes = new Uint8Array(32);
  bytes[0] = 1 + (index % 200);
  bytes[1] = Math.floor(index / 200);
  return addressFromBytes(bytes);
};

function addressFromBytes(bytes: Uint8Array): Address {
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  let out = '';
  while (value > 0n) {
    out = alphabet[Number(value % 58n)] + out;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    out = '1' + out;
  }
  return address(out);
}

function measure(instructions: Instruction[], feePayer: Address): number {
  const message = pipe(
    createTransactionMessage({ version: 1 }),
    (value) => setTransactionMessageFeePayer(feePayer, value),
    (value) =>
      setTransactionMessageLifetimeUsingBlockhash(
        { blockhash: blockhash('11111111111111111111111111111111'), lastValidBlockHeight: 1n },
        value,
      ),
    (value) => appendTransactionMessageInstructions(instructions, value),
  );
  return measureTransactionMessage(message).size;
}

function accountRole(signer: boolean, writable: boolean): AccountRole {
  if (signer) return writable ? AccountRole.WRITABLE_SIGNER : AccountRole.READONLY_SIGNER;
  return writable ? AccountRole.WRITABLE : AccountRole.READONLY;
}

const programAddress: Record<string, Address> = {
  'system-program': addressFromBytes(SYSTEM_PROGRAM_ADDRESS_BYTES),
  'token-program': addressFromBytes(TOKEN_PROGRAM_ADDRESS_BYTES),
  'ata-program': addressFromBytes(ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES),
};

describe('example benchmarks', () => {
  test('compiles every example and measures its transaction size', () => {
    const output: Record<string, unknown> = {};
    for (const item of cases) {
      const compiled = compileTemplate(item.template);
      const fixedRoles = compiled.fixedAccountOrder.map((name) => {
        const role = item.accounts[name];
        expect(role, `${item.name}: no role for account ${name}`).toBeDefined();
        return role!;
      });
      const rowRoles = (item.rows ?? []).flatMap((row) =>
        compiled.batchAccountOrder.map((name) => {
          const role = row[name];
          expect(role, `${item.name}: no role for row account ${name}`).toBeDefined();
          return role!;
        }),
      );
      const roles = [...fixedRoles, ...rowRoles];

      // Placeholder addresses: sizes depend on the count and privileges, not the bytes.
      const addresses = roles.map((role, index) => programAddress[role] ?? PLACEHOLDER(index + 1));
      const accounts = Object.fromEntries(
        compiled.fixedAccountOrder.map((name, index) => [name, { address: addresses[index]! }]),
      );
      const batchRows = (item.rows ?? []).map((_, rowIndex) =>
        Object.fromEntries(
          compiled.batchAccountOrder.map((name, column) => [
            name,
            { address: addresses[fixedRoles.length + rowIndex * compiled.batchAccountOrder.length + column]! },
          ]),
        ),
      );
      const templateAddress = PLACEHOLDER(250);
      const feePayer = addresses.find((_, index) => roles[index] === 'signer') ?? PLACEHOLDER(251);
      const runInstruction = buildKitRunInstruction({
        compiled,
        templateAddress,
        ...(item.inputs ? { inputs: item.inputs } : {}),
        accounts,
        ...(batchRows.length > 0 ? { batchRows } : {}),
      });

      const baselineInstructions: Instruction[] = item.baseline.instructions.map((instruction) => ({
        programAddress: programAddress[instruction.program]!,
        accounts: instruction.accounts.map((entry) => ({
          address: addresses[entry.index]!,
          role: accountRole(entry.signer, entry.writable),
        })),
        data: Uint8Array.from(Buffer.from(instruction.data, 'hex')),
      }));

      output[item.name] = {
        page: item.page,
        anchor: item.anchor,
        standIn: item.standIn === true,
        rowCount: item.rows?.length ?? 0,
        templateHex: hex(compiled.bytes),
        payloadBytes: compiled.bytes.length,
        runtimeAccounts: roles,
        runData: hex(encodeRunInputs(compiled, item.inputs ?? {})),
        ballistaTransactionBytes: measure([runInstruction], feePayer),
        ballistaAccountKeys: (runInstruction.accounts ?? []).length + 1,
        // Privileges the schema requires of each runtime account, template account excluded.
        runtimeAccountFlags: (runInstruction.accounts ?? []).slice(1).map((entry) => ({
          signer: (Number(entry.role) & 2) !== 0,
          writable: (Number(entry.role) & 1) !== 0,
        })),
        baseline: {
          verdict: item.baseline.verdict,
          note: item.baseline.note,
          instructions: item.baseline.instructions,
          transactionBytes: measure(baselineInstructions, feePayer),
          instructionCount: item.baseline.instructions.length,
        },
        stats: compiled.stats,
      };
    }
    writeFileSync(FIXTURE_PATH, `${JSON.stringify(output, null, 2)}\n`);
    expect(Object.keys(output)).toHaveLength(cases.length);
  });
});
