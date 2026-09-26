import {
  AccountRole,
  address,
  appendTransactionMessageInstruction,
  estimateResourceLimitsFactory,
  getAddressDecoder,
  getAddressEncoder,
  getProgramDerivedAddress,
  getTransactionMessageSize,
  getTransactionMessageSizeLimit,
  setTransactionMessageComputeUnitLimit,
  setTransactionMessageLoadedAccountsDataSizeLimit,
  type Address,
  type Instruction,
} from '@solana/kit';

import type { CompiledTemplate } from './compiler.js';
import {
  BALLISTA_PROGRAM_ADDRESS,
  buildRunInstruction,
  encodeCancelTemplate,
  encodeWriteTemplateChunk,
  planTemplateUpload,
  resumeTemplateUpload,
  type AccountBinding,
  type DecodedTemplateAccount,
  type InstructionDescriptor,
  type RunInputValue,
  type TemplateUploadPlan,
  type UploadInstruction,
} from './instructions.js';

export const BALLISTA_ADDRESS = address(BALLISTA_PROGRAM_ADDRESS);
export const SYSTEM_PROGRAM_ADDRESS = address('11111111111111111111111111111111');

export interface KitAccountBinding {
  address: Address;
  /** Account group members only: pass the account as writable. */
  writable?: boolean;
}

export interface KitUploadInstruction {
  kind: UploadInstruction['kind'];
  instruction: Instruction;
  offset?: number;
}

export interface KitTemplateUploadPlan {
  mode: TemplateUploadPlan['mode'];
  templateAddress: Address;
  bump: number;
  instructions: readonly KitUploadInstruction[];
  payloadLength: number;
  payloadHash: Uint8Array;
}

type SizeableTransactionMessage = Parameters<typeof getTransactionMessageSize>[0];
type ResourceLimitEstimator = ReturnType<typeof estimateResourceLimitsFactory>;
type EstimableTransactionMessage = Parameters<ResourceLimitEstimator>[0];
export type ComputeUnitEstimateConfig = NonNullable<Parameters<ResourceLimitEstimator>[1]>;
export type ComputeUnitRpc = Parameters<typeof estimateResourceLimitsFactory>[0]['rpc'];

export const MAX_TRANSACTION_COMPUTE_UNITS = 1_400_000;
export const MAX_LOADED_ACCOUNTS_DATA_SIZE = 64 * 1_024 * 1_024;
export const LOADED_ACCOUNTS_DATA_PAGE_SIZE = 32 * 1_024;

export interface ComputeUnitMarginConfig {
  /** Safety margin in basis points. Defaults to the Solana-recommended 10%. */
  marginBps?: number;
  /** Cannot exceed the SVM transaction maximum of 1,400,000 CUs. */
  maxComputeUnitLimit?: number;
}

export interface ComputeUnitEstimate {
  simulatedComputeUnits: number;
  computeUnitLimit: number;
  marginComputeUnits: number;
  marginBps: number;
  capped: boolean;
  simulatedLoadedAccountsDataSize?: number;
  loadedAccountsDataSizeLimit?: number;
}

export interface ComputeUnitProvider {
  estimate<TTransactionMessage extends EstimableTransactionMessage>(
    transactionMessage: TTransactionMessage,
    config?: ComputeUnitEstimateConfig,
  ): Promise<ComputeUnitEstimate>;
  estimateAndSet<TTransactionMessage extends EstimableTransactionMessage>(
    transactionMessage: TTransactionMessage,
    config?: ComputeUnitEstimateConfig,
  ): Promise<{ transactionMessage: TTransactionMessage; estimate: ComputeUnitEstimate }>;
}

export function getComputeUnitLimitWithMargin(
  simulatedComputeUnits: number,
  config: ComputeUnitMarginConfig = {},
): Omit<ComputeUnitEstimate, 'loadedAccountsDataSizeLimit'> {
  const marginBps = config.marginBps ?? 1_000;
  const maximum = config.maxComputeUnitLimit ?? MAX_TRANSACTION_COMPUTE_UNITS;
  if (!Number.isInteger(simulatedComputeUnits) || simulatedComputeUnits < 0) {
    throw new RangeError('Simulated compute units must be a non-negative integer');
  }
  if (!Number.isInteger(marginBps) || marginBps < 0 || marginBps > 100_000) {
    throw new RangeError('Compute unit margin must be an integer from 0 to 100,000 basis points');
  }
  if (!Number.isInteger(maximum) || maximum < 1 || maximum > MAX_TRANSACTION_COMPUTE_UNITS) {
    throw new RangeError('Maximum compute unit limit must be from 1 to 1,400,000');
  }
  if (simulatedComputeUnits > maximum) {
    throw new RangeError('Simulated compute units exceed the configured maximum');
  }

  const requested = Math.ceil((simulatedComputeUnits * (10_000 + marginBps)) / 10_000);
  const computeUnitLimit = Math.min(requested, maximum);
  return {
    simulatedComputeUnits,
    computeUnitLimit,
    marginComputeUnits: computeUnitLimit - simulatedComputeUnits,
    marginBps,
    capped: requested > maximum,
  };
}

/**
 * Simulates transaction messages with Solana Kit's resource estimator and applies a buffered CU
 * limit. Version-1 loaded-account data limits are preserved from the same simulation.
 */
export function createComputeUnitProvider(input: {
  rpc: ComputeUnitRpc;
} & ComputeUnitMarginConfig): ComputeUnitProvider {
  const estimateResourceLimits = estimateResourceLimitsFactory({ rpc: input.rpc });
  const marginConfig: ComputeUnitMarginConfig = {
    ...(input.marginBps === undefined ? {} : { marginBps: input.marginBps }),
    ...(input.maxComputeUnitLimit === undefined
      ? {}
      : { maxComputeUnitLimit: input.maxComputeUnitLimit }),
  };

  const estimate: ComputeUnitProvider['estimate'] = async (transactionMessage, config) => {
    const resources = await estimateResourceLimits(transactionMessage, config);
    const loadedAccountsDataSize =
      'loadedAccountsDataSizeLimit' in resources
        ? resources.loadedAccountsDataSizeLimit
        : undefined;
    return {
      ...getComputeUnitLimitWithMargin(resources.computeUnitLimit, marginConfig),
      ...(loadedAccountsDataSize !== undefined
        ? {
            simulatedLoadedAccountsDataSize: loadedAccountsDataSize,
            loadedAccountsDataSizeLimit:
              getLoadedAccountsDataSizeLimitWithHeadroom(loadedAccountsDataSize),
          }
        : {}),
    };
  };

  return {
    estimate,
    async estimateAndSet(transactionMessage, config) {
      const measurement = await estimate(transactionMessage, config);
      let transactionMessageWithLimits = setTransactionMessageComputeUnitLimit(
        measurement.computeUnitLimit,
        transactionMessage,
      );
      if (
        transactionMessage.version === 1 &&
        measurement.loadedAccountsDataSizeLimit !== undefined
      ) {
        transactionMessageWithLimits = setTransactionMessageLoadedAccountsDataSizeLimit(
          measurement.loadedAccountsDataSizeLimit,
          transactionMessageWithLimits,
        );
      }
      return { transactionMessage: transactionMessageWithLimits, estimate: measurement };
    },
  };
}

/** Rounds a simulation result to the next 32 KiB cost-model page for v1 headroom. */
export function getLoadedAccountsDataSizeLimitWithHeadroom(simulatedBytes: number): number {
  if (
    !Number.isInteger(simulatedBytes) ||
    simulatedBytes < 0 ||
    simulatedBytes > MAX_LOADED_ACCOUNTS_DATA_SIZE
  ) {
    throw new RangeError('Loaded account data size must be from 0 to 64 MiB');
  }
  if (simulatedBytes === 0) return LOADED_ACCOUNTS_DATA_PAGE_SIZE;
  return Math.min(
    Math.ceil(simulatedBytes / LOADED_ACCOUNTS_DATA_PAGE_SIZE) *
      LOADED_ACCOUNTS_DATA_PAGE_SIZE,
    MAX_LOADED_ACCOUNTS_DATA_SIZE,
  );
}

/** Reads the authoritative CU count returned in confirmed transaction metadata. */
export function getComputeUnitsConsumed(
  transaction: { meta?: { computeUnitsConsumed?: bigint | number | null } | null } | null,
): number | undefined {
  const value = transaction?.meta?.computeUnitsConsumed;
  if (value === undefined || value === null) return undefined;
  const units = Number(value);
  if (!Number.isSafeInteger(units) || units < 0) {
    throw new RangeError('Transaction compute units are not a safe non-negative integer');
  }
  return units;
}

export async function getTemplateAddress(
  creator: Address,
  templateId: number,
  programAddress: Address = BALLISTA_ADDRESS,
): Promise<readonly [Address, number]> {
  validateTemplateId(templateId);
  const [templateAddress, bump] = await getProgramDerivedAddress({
    programAddress,
    seeds: [
      new TextEncoder().encode('template'),
      getAddressEncoder().encode(creator),
      Uint8Array.of(templateId & 0xff, templateId >>> 8),
    ],
  });
  return [templateAddress, bump];
}

/** The subset of a Kit RPC client `findFreeTemplateId` needs. */
export interface TemplateProbeRpc {
  getMultipleAccounts(addresses: Address[]): { send(): Promise<{ value: readonly (unknown | null)[] }> };
}

export interface FreeTemplateId {
  templateId: number;
  templateAddress: Address;
  bump: number;
}

/**
 * Finds the lowest template ID at or above `start` whose address holds no account. Anyone can
 * dust a predictable template address, and creation tolerates that, but an address that already
 * holds a finalized or uploading template cannot be reused, so callers probe before uploading.
 */
export async function findFreeTemplateId(input: {
  rpc: TemplateProbeRpc;
  creator: Address;
  programAddress?: Address;
  start?: number;
  batchSize?: number;
}): Promise<FreeTemplateId> {
  const programAddress = input.programAddress ?? BALLISTA_ADDRESS;
  const batchSize = input.batchSize ?? 16;
  if (!Number.isInteger(batchSize) || batchSize < 1 || batchSize > 100) {
    throw new RangeError('Probe batch size must be from 1 to 100');
  }
  let templateId = input.start ?? 0;
  validateTemplateId(templateId);
  while (templateId <= 0xffff) {
    const count = Math.min(batchSize, 0x1_0000 - templateId);
    const candidates = await Promise.all(
      Array.from({ length: count }, (_, offset) => getTemplateAddress(input.creator, templateId + offset, programAddress)),
    );
    const { value } = await input.rpc.getMultipleAccounts(candidates.map(([address]) => address)).send();
    const free = value.findIndex((account) => account === null);
    if (free !== -1) {
      const [templateAddress, bump] = candidates[free]!;
      return { templateId: templateId + free, templateAddress, bump };
    }
    templateId += count;
  }
  throw new RangeError('Every template ID for this creator is taken');
}

export function toKitInstruction(descriptor: InstructionDescriptor): Instruction {
  const decoder = getAddressDecoder();
  return {
    programAddress: decoder.decode(descriptor.programAddress),
    accounts: descriptor.accounts.map((account) => ({
      address: decoder.decode(account.address),
      role: accountRole(account.signer, account.writable),
    })),
    data: descriptor.data,
  };
}

export function buildKitRunInstruction(input: {
  compiled: CompiledTemplate;
  templateAddress: Address;
  programAddress?: Address;
  inputs?: Readonly<Record<string, RunInputValue>>;
  accounts: Readonly<Record<string, KitAccountBinding>>;
  batchRows?: readonly Readonly<Record<string, KitAccountBinding>>[];
  batchInputs?: readonly Readonly<Record<string, RunInputValue>>[];
  accountGroups?: Readonly<Record<string, readonly KitAccountBinding[]>>;
}): Instruction {
  const encoder = getAddressEncoder();
  const accountGroups = input.accountGroups
    ? (Object.fromEntries(
        Object.entries(input.accountGroups).map(([name, members]) => [
          name,
          members.map((member) => ({
            address: Uint8Array.from(encoder.encode(member.address)),
            ...(member.writable !== undefined ? { writable: member.writable } : {}),
          })),
        ]),
      ) as Readonly<Record<string, readonly AccountBinding[]>>)
    : undefined;
  const accounts = Object.fromEntries(
    Object.entries(input.accounts).map(([name, binding]) => [
      name,
      { address: Uint8Array.from(encoder.encode(binding.address)) },
    ]),
  ) as Readonly<Record<string, AccountBinding>>;
  const batchRows = input.batchRows?.map(
    (row) =>
      Object.fromEntries(
        Object.entries(row).map(([name, binding]) => [
          name,
          { address: Uint8Array.from(encoder.encode(binding.address)) },
        ]),
      ) as Readonly<Record<string, AccountBinding>>,
  );
  return toKitInstruction(
    buildRunInstruction({
      compiled: input.compiled,
      programAddress: Uint8Array.from(encoder.encode(input.programAddress ?? BALLISTA_ADDRESS)),
      templateAddress: Uint8Array.from(encoder.encode(input.templateAddress)),
      inputs: input.inputs,
      accounts,
      batchRows,
      ...(input.batchInputs ? { batchInputs: input.batchInputs } : {}),
      ...(accountGroups ? { accountGroups } : {}),
    }),
  );
}

export async function buildKitTemplateUploadPlan(input: {
  compiled: CompiledTemplate;
  creator: Address;
  templateId: number;
  programAddress?: Address;
  maxInstructionDataBytes?: number;
  transactionMessage?: SizeableTransactionMessage;
}): Promise<KitTemplateUploadPlan> {
  const programAddress = input.programAddress ?? BALLISTA_ADDRESS;
  const [templateAddress, bump] = await getTemplateAddress(input.creator, input.templateId, programAddress);
  let plan: TemplateUploadPlan;

  if (input.transactionMessage) {
    const oneShot = planTemplateUpload(input.compiled, input.templateId, {
      maxInstructionDataBytes: input.compiled.bytes.length + 35,
    });
    const createInstruction = buildUploadInstruction(
      oneShot.instructions[0]!,
      programAddress,
      input.creator,
      templateAddress,
    );
    if (measureInstructionInTransaction(input.transactionMessage, createInstruction).fits) {
      plan = oneShot;
    } else {
      const maxInstructionDataBytes = findLargestWriteInstructionData(
        input.transactionMessage,
        programAddress,
        input.creator,
        templateAddress,
      );
      plan = planTemplateUpload(input.compiled, input.templateId, { maxInstructionDataBytes });
    }
  } else {
    plan = planTemplateUpload(input.compiled, input.templateId, {
      maxInstructionDataBytes: input.maxInstructionDataBytes,
    });
  }

  return mapUploadPlan(plan, programAddress, input.creator, templateAddress, bump);
}

export async function buildKitResumeTemplateUploadPlan(input: {
  compiled: CompiledTemplate;
  account: DecodedTemplateAccount | Uint8Array;
  creator: Address;
  templateId: number;
  programAddress?: Address;
  maxInstructionDataBytes?: number;
  transactionMessage?: SizeableTransactionMessage;
}): Promise<KitTemplateUploadPlan> {
  const programAddress = input.programAddress ?? BALLISTA_ADDRESS;
  const [templateAddress, bump] = await getTemplateAddress(input.creator, input.templateId, programAddress);
  const maxInstructionDataBytes = input.transactionMessage
    ? findLargestWriteInstructionData(
        input.transactionMessage,
        programAddress,
        input.creator,
        templateAddress,
      )
    : input.maxInstructionDataBytes;
  const plan = resumeTemplateUpload(input.compiled, input.account, { maxInstructionDataBytes });
  return mapUploadPlan(plan, programAddress, input.creator, templateAddress, bump);
}

export async function buildKitCancelTemplateInstruction(input: {
  creator: Address;
  templateId: number;
  programAddress?: Address;
}): Promise<Instruction> {
  const programAddress = input.programAddress ?? BALLISTA_ADDRESS;
  const [templateAddress] = await getTemplateAddress(input.creator, input.templateId, programAddress);
  return {
    programAddress,
    accounts: [
      { address: input.creator, role: AccountRole.WRITABLE_SIGNER },
      { address: templateAddress, role: AccountRole.WRITABLE },
    ],
    data: encodeCancelTemplate(),
  };
}

export function measureTransactionMessage(transactionMessage: SizeableTransactionMessage): {
  size: number;
  limit: number;
  fits: boolean;
} {
  const size = getTransactionMessageSize(transactionMessage);
  const limit = getTransactionMessageSizeLimit(transactionMessage);
  return { size, limit, fits: size <= limit };
}

export function measureInstructionInTransaction(
  transactionMessage: SizeableTransactionMessage,
  instruction: Instruction,
): { size: number; limit: number; fits: boolean } {
  return measureTransactionMessage(appendTransactionMessageInstruction(instruction, transactionMessage));
}

function mapUploadPlan(
  plan: TemplateUploadPlan,
  programAddress: Address,
  creator: Address,
  templateAddress: Address,
  bump: number,
): KitTemplateUploadPlan {
  return {
    mode: plan.mode,
    templateAddress,
    bump,
    instructions: plan.instructions.map((upload) => ({
      kind: upload.kind,
      offset: upload.offset,
      instruction: buildUploadInstruction(upload, programAddress, creator, templateAddress),
    })),
    payloadLength: plan.payloadLength,
    payloadHash: plan.payloadHash,
  };
}

function buildUploadInstruction(
  upload: UploadInstruction,
  programAddress: Address,
  creator: Address,
  templateAddress: Address,
): Instruction {
  const accounts =
    upload.kind === 'create' || upload.kind === 'begin'
      ? [
          { address: creator, role: AccountRole.WRITABLE_SIGNER },
          { address: templateAddress, role: AccountRole.WRITABLE },
          { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ]
      : [
          { address: creator, role: AccountRole.WRITABLE_SIGNER },
          { address: templateAddress, role: AccountRole.WRITABLE },
        ];
  return { programAddress, accounts, data: upload.data };
}

function findLargestWriteInstructionData(
  transactionMessage: SizeableTransactionMessage,
  programAddress: Address,
  creator: Address,
  templateAddress: Address,
): number {
  let low = 64;
  let high = getTransactionMessageSizeLimit(transactionMessage);
  let best = 0;
  while (low <= high) {
    const middle = Math.floor((low + high) / 2);
    const upload: UploadInstruction = {
      kind: 'write',
      offset: 0,
      data: encodeWriteTemplateChunk(0, new Uint8Array(middle - 5)),
    };
    const instruction = buildUploadInstruction(upload, programAddress, creator, templateAddress);
    if (measureInstructionInTransaction(transactionMessage, instruction).fits) {
      best = middle;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  if (best < 64) throw new RangeError('The base transaction message leaves no room for an upload chunk');
  return best;
}

function accountRole(signer: boolean, writable: boolean): AccountRole {
  if (signer) return writable ? AccountRole.WRITABLE_SIGNER : AccountRole.READONLY_SIGNER;
  return writable ? AccountRole.WRITABLE : AccountRole.READONLY;
}

function validateTemplateId(templateId: number): void {
  if (!Number.isInteger(templateId) || templateId < 0 || templateId > 0xffff) {
    throw new RangeError('Template ID must be an unsigned 16-bit integer');
  }
}
