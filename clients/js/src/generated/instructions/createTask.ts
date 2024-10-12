/**
 * This code was AUTOGENERATED using the codama library.
 * Please DO NOT EDIT THIS FILE, instead use visitors
 * to add features, then rerun codama to update it.
 *
 * @see https://github.com/codama-idl/codama
 */

import {
  combineCodec,
  getStructDecoder,
  getStructEncoder,
  getU16Decoder,
  getU16Encoder,
  getU8Decoder,
  getU8Encoder,
  transformEncoder,
  type Address,
  type Codec,
  type Decoder,
  type Encoder,
  type IAccountMeta,
  type IAccountSignerMeta,
  type IInstruction,
  type IInstructionWithAccounts,
  type IInstructionWithData,
  type ReadonlyAccount,
  type TransactionSigner,
  type WritableAccount,
  type WritableSignerAccount,
} from '@solana/web3.js';
import { findTaskDefinitionPda } from '../pdas';
import { BALLISTA_PROGRAM_ADDRESS } from '../programs';
import {
  expectAddress,
  expectSome,
  getAccountMetaFactory,
  type ResolvedAccount,
} from '../shared';
import {
  getTaskDefinitionDecoder,
  getTaskDefinitionEncoder,
  type TaskDefinition,
  type TaskDefinitionArgs,
} from '../types';

export const CREATE_TASK_DISCRIMINATOR = 0;

export function getCreateTaskDiscriminatorBytes() {
  return getU8Encoder().encode(CREATE_TASK_DISCRIMINATOR);
}

export type CreateTaskInstruction<
  TProgram extends string = typeof BALLISTA_PROGRAM_ADDRESS,
  TAccountPayer extends string | IAccountMeta<string> = string,
  TAccountTaskDefinition extends string | IAccountMeta<string> = string,
  TAccountSystemProgram extends
    | string
    | IAccountMeta<string> = '11111111111111111111111111111111',
  TRemainingAccounts extends readonly IAccountMeta<string>[] = [],
> = IInstruction<TProgram> &
  IInstructionWithData<Uint8Array> &
  IInstructionWithAccounts<
    [
      TAccountPayer extends string
        ? WritableSignerAccount<TAccountPayer> &
            IAccountSignerMeta<TAccountPayer>
        : TAccountPayer,
      TAccountTaskDefinition extends string
        ? WritableAccount<TAccountTaskDefinition>
        : TAccountTaskDefinition,
      TAccountSystemProgram extends string
        ? ReadonlyAccount<TAccountSystemProgram>
        : TAccountSystemProgram,
      ...TRemainingAccounts,
    ]
  >;

export type CreateTaskInstructionData = {
  discriminator: number;
  task: TaskDefinition;
  taskId: number;
};

export type CreateTaskInstructionDataArgs = {
  task: TaskDefinitionArgs;
  taskId: number;
};

export function getCreateTaskInstructionDataEncoder(): Encoder<CreateTaskInstructionDataArgs> {
  return transformEncoder(
    getStructEncoder([
      ['discriminator', getU8Encoder()],
      ['task', getTaskDefinitionEncoder()],
      ['taskId', getU16Encoder()],
    ]),
    (value) => ({ ...value, discriminator: CREATE_TASK_DISCRIMINATOR })
  );
}

export function getCreateTaskInstructionDataDecoder(): Decoder<CreateTaskInstructionData> {
  return getStructDecoder([
    ['discriminator', getU8Decoder()],
    ['task', getTaskDefinitionDecoder()],
    ['taskId', getU16Decoder()],
  ]);
}

export function getCreateTaskInstructionDataCodec(): Codec<
  CreateTaskInstructionDataArgs,
  CreateTaskInstructionData
> {
  return combineCodec(
    getCreateTaskInstructionDataEncoder(),
    getCreateTaskInstructionDataDecoder()
  );
}

export type CreateTaskAsyncInput<
  TAccountPayer extends string = string,
  TAccountTaskDefinition extends string = string,
  TAccountSystemProgram extends string = string,
> = {
  /** Payer account */
  payer: TransactionSigner<TAccountPayer>;
  /** Task definition account */
  taskDefinition?: Address<TAccountTaskDefinition>;
  /** System program */
  systemProgram?: Address<TAccountSystemProgram>;
  task: CreateTaskInstructionDataArgs['task'];
  taskId: CreateTaskInstructionDataArgs['taskId'];
};

export async function getCreateTaskInstructionAsync<
  TAccountPayer extends string,
  TAccountTaskDefinition extends string,
  TAccountSystemProgram extends string,
  TProgramAddress extends Address = typeof BALLISTA_PROGRAM_ADDRESS,
>(
  input: CreateTaskAsyncInput<
    TAccountPayer,
    TAccountTaskDefinition,
    TAccountSystemProgram
  >,
  config?: { programAddress?: TProgramAddress }
): Promise<
  CreateTaskInstruction<
    TProgramAddress,
    TAccountPayer,
    TAccountTaskDefinition,
    TAccountSystemProgram
  >
> {
  // Program address.
  const programAddress = config?.programAddress ?? BALLISTA_PROGRAM_ADDRESS;

  // Original accounts.
  const originalAccounts = {
    payer: { value: input.payer ?? null, isWritable: true },
    taskDefinition: { value: input.taskDefinition ?? null, isWritable: true },
    systemProgram: { value: input.systemProgram ?? null, isWritable: false },
  };
  const accounts = originalAccounts as Record<
    keyof typeof originalAccounts,
    ResolvedAccount
  >;

  // Original args.
  const args = { ...input };

  // Resolve default values.
  if (!accounts.taskDefinition.value) {
    accounts.taskDefinition.value = await findTaskDefinitionPda({
      payer: expectAddress(accounts.payer.value),
      taskId: expectSome(args.taskId),
    });
  }
  if (!accounts.systemProgram.value) {
    accounts.systemProgram.value =
      '11111111111111111111111111111111' as Address<'11111111111111111111111111111111'>;
  }

  const getAccountMeta = getAccountMetaFactory(programAddress, 'programId');
  const instruction = {
    accounts: [
      getAccountMeta(accounts.payer),
      getAccountMeta(accounts.taskDefinition),
      getAccountMeta(accounts.systemProgram),
    ],
    programAddress,
    data: getCreateTaskInstructionDataEncoder().encode(
      args as CreateTaskInstructionDataArgs
    ),
  } as CreateTaskInstruction<
    TProgramAddress,
    TAccountPayer,
    TAccountTaskDefinition,
    TAccountSystemProgram
  >;

  return instruction;
}

export type CreateTaskInput<
  TAccountPayer extends string = string,
  TAccountTaskDefinition extends string = string,
  TAccountSystemProgram extends string = string,
> = {
  /** Payer account */
  payer: TransactionSigner<TAccountPayer>;
  /** Task definition account */
  taskDefinition: Address<TAccountTaskDefinition>;
  /** System program */
  systemProgram?: Address<TAccountSystemProgram>;
  task: CreateTaskInstructionDataArgs['task'];
  taskId: CreateTaskInstructionDataArgs['taskId'];
};

export function getCreateTaskInstruction<
  TAccountPayer extends string,
  TAccountTaskDefinition extends string,
  TAccountSystemProgram extends string,
  TProgramAddress extends Address = typeof BALLISTA_PROGRAM_ADDRESS,
>(
  input: CreateTaskInput<
    TAccountPayer,
    TAccountTaskDefinition,
    TAccountSystemProgram
  >,
  config?: { programAddress?: TProgramAddress }
): CreateTaskInstruction<
  TProgramAddress,
  TAccountPayer,
  TAccountTaskDefinition,
  TAccountSystemProgram
> {
  // Program address.
  const programAddress = config?.programAddress ?? BALLISTA_PROGRAM_ADDRESS;

  // Original accounts.
  const originalAccounts = {
    payer: { value: input.payer ?? null, isWritable: true },
    taskDefinition: { value: input.taskDefinition ?? null, isWritable: true },
    systemProgram: { value: input.systemProgram ?? null, isWritable: false },
  };
  const accounts = originalAccounts as Record<
    keyof typeof originalAccounts,
    ResolvedAccount
  >;

  // Original args.
  const args = { ...input };

  // Resolve default values.
  if (!accounts.systemProgram.value) {
    accounts.systemProgram.value =
      '11111111111111111111111111111111' as Address<'11111111111111111111111111111111'>;
  }

  const getAccountMeta = getAccountMetaFactory(programAddress, 'programId');
  const instruction = {
    accounts: [
      getAccountMeta(accounts.payer),
      getAccountMeta(accounts.taskDefinition),
      getAccountMeta(accounts.systemProgram),
    ],
    programAddress,
    data: getCreateTaskInstructionDataEncoder().encode(
      args as CreateTaskInstructionDataArgs
    ),
  } as CreateTaskInstruction<
    TProgramAddress,
    TAccountPayer,
    TAccountTaskDefinition,
    TAccountSystemProgram
  >;

  return instruction;
}

export type ParsedCreateTaskInstruction<
  TProgram extends string = typeof BALLISTA_PROGRAM_ADDRESS,
  TAccountMetas extends readonly IAccountMeta[] = readonly IAccountMeta[],
> = {
  programAddress: Address<TProgram>;
  accounts: {
    /** Payer account */
    payer: TAccountMetas[0];
    /** Task definition account */
    taskDefinition: TAccountMetas[1];
    /** System program */
    systemProgram: TAccountMetas[2];
  };
  data: CreateTaskInstructionData;
};

export function parseCreateTaskInstruction<
  TProgram extends string,
  TAccountMetas extends readonly IAccountMeta[],
>(
  instruction: IInstruction<TProgram> &
    IInstructionWithAccounts<TAccountMetas> &
    IInstructionWithData<Uint8Array>
): ParsedCreateTaskInstruction<TProgram, TAccountMetas> {
  if (instruction.accounts.length < 3) {
    // TODO: Coded error.
    throw new Error('Not enough accounts');
  }
  let accountIndex = 0;
  const getNextAccount = () => {
    const accountMeta = instruction.accounts![accountIndex]!;
    accountIndex += 1;
    return accountMeta;
  };
  return {
    programAddress: instruction.programAddress,
    accounts: {
      payer: getNextAccount(),
      taskDefinition: getNextAccount(),
      systemProgram: getNextAccount(),
    },
    data: getCreateTaskInstructionDataDecoder().decode(instruction.data),
  };
}
