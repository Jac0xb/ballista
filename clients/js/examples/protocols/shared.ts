/**
 * Constants shared by the live-protocol examples.
 *
 * Every program address and every account offset here was read from the protocol's own source at
 * the commit cited beside it. A protocol upgrade can move a field, and a moved field is a
 * silently wrong read, so re-derive these from the current IDL before you upload a template.
 */
import { sha256 } from '@noble/hashes/sha2.js';
import { address, getAddressEncoder, type Address } from '@solana/kit';

const encoder = getAddressEncoder();

/** A base58 program address as the 32 raw bytes a template schema pins. */
export function addressBytes(value: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(encoder.encode(address(value)));
}

/**
 * An Anchor instruction discriminator: the first eight bytes of `sha256("global:<name>")`, where
 * `<name>` is the handler's snake_case name in the `#[program]` module.
 */
export function anchorDiscriminator(name: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(sha256(new TextEncoder().encode(`global:${name}`)).slice(0, 8));
}

// ---------------------------------------------------------------- programs

/** Jupiter aggregator v6. */
export const JUPITER_V6 = 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4' as const;
/** Kamino Lend, the primary market program. */
export const KAMINO_LEND = 'KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD' as const;
/** Orca Whirlpools, from `declare_id!` in programs/whirlpool/src/lib.rs. */
export const ORCA_WHIRLPOOL = 'whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc' as const;
/** marginfi v2. */
export const MARGINFI_V2 = 'MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA' as const;
/** Pyth Solana receiver, the non-`pro-compatible` build. */
export const PYTH_RECEIVER = 'rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ' as const;
/** SPL Memo. Orca's v2 instructions take it, for Token-2022 transfers that require a memo. */
export const MEMO_PROGRAM = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr' as const;
/** The instructions sysvar. Every Kamino v2 lending instruction takes it as an account. */
export const SYSVAR_INSTRUCTIONS = 'Sysvar1nstructions1111111111111111111111111' as const;
/** Kamino Farms, which Kamino Lend invokes whenever a lending instruction touches a reserve with a farm. */
export const KAMINO_FARMS = 'FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr' as const;

/**
 * Jito's Tip Payment program. It owns all eight tip accounts, as program addresses holding eight
 * bytes of data, so none of them is a plain System account.
 */
export const JITO_TIP_PAYMENT = 'T1pyyaTNZsKv2WcRAB8oVnk93mLJw2XzjtVYqCsaHqt' as const;

/**
 * The eight Jito tip accounts. A tip is a plain SOL transfer to one of these and may be made by
 * CPI; the minimum is 1,000 lamports.
 */
export const JITO_TIP_ACCOUNTS: readonly Address[] = [
  address('96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5'),
  address('HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe'),
  address('Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY'),
  address('ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49'),
  address('DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh'),
  address('ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt'),
  address('DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL'),
  address('3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT'),
];

// ----------------------------------------------------------------- layouts

/** SPL Token account: `amount` is a little-endian u64 at offset 64 of the 165-byte layout. */
export const TOKEN_ACCOUNT_AMOUNT_OFFSET = 64;
export const TOKEN_ACCOUNT_LENGTH = 165;

/** SPL Token account: the mint is the first field. */
export const TOKEN_ACCOUNT_MINT_OFFSET = 0;

/** SPL Token account: the owner, the wallet the balance belongs to, follows the mint. */
export const TOKEN_ACCOUNT_OWNER_OFFSET = 32;

/** The wrapped SOL mint. Its token accounts count their balance in lamports. */
export const WRAPPED_SOL_MINT = 'So11111111111111111111111111111111111111112' as const;

/** Circle's USDC mint. */
export const USDC_MINT = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' as const;

/** A Pyth feed id, which Pyth publishes as 64 hex digits, as the 32 bytes a price account stores. */
export function pythFeedId(hex: string): Uint8Array<ArrayBuffer> {
  if (!/^[0-9a-f]{64}$/.test(hex)) throw new Error('A Pyth feed id is 64 lowercase hex digits');
  return Uint8Array.from({ length: 32 }, (_, index) => Number.parseInt(hex.slice(2 * index, 2 * index + 2), 16));
}

/** SPL Token `Mint`: `decimals` is the u8 at offset 44 of the 82-byte layout. */
export const SPL_MINT = { length: 82, decimals: 44 } as const;

/**
 * Pyth `PriceUpdateV2`.
 *
 * The offsets depend on the account's verification level, which is why `verificationLevel` is
 * read first. `VerificationLevel` is a Borsh enum: `Full` serializes as one byte, `Partial {
 * num_signatures: u8 }` as two. Anchor writes the struct sequentially, so a `Full` account puts
 * every later field one byte earlier than a `Partial` one. The account is allocated at the
 * larger size either way, leaving a trailing byte unused.
 *
 * Deriving offsets from `PriceUpdateV2::LEN = 8 + 32 + 2 + 32 + 8 + …` therefore gives the
 * `Partial` layout, while most accounts on devnet are `Full`. Templates here require `Full` (it is
 * the stronger guarantee anyway, having all the signatures rather than some) and read at the
 * `Full` offsets.
 */
export const PYTH = {
  length: 134,
  /** 1 is `Full`, 0 is `Partial`. Read this before trusting any offset below. */
  verificationLevel: 40,
  verificationLevelFull: 1,
  /**
   * Offsets for a `Full` account. A `Partial` one shifts each by one.
   *
   * `feedId` is `price_message.feed_id`, the 32 bytes that say which feed the price belongs to.
   * The receiver owns every feed's account alike, so nothing else about the account says which
   * one it is.
   */
  feedId: 41,
  price: 73,
  confidence: 81,
  exponent: 89,
  publishTime: 93,
} as const;

/**
 * Orca `Position`, declared as `whirlpool, position_mint, liquidity, tick_lower_index,
 * tick_upper_index, fee_growth_checkpoint_a, fee_owed_a, fee_growth_checkpoint_b, fee_owed_b,
 * reward_infos` with `LEN = 8 + 136 + 72`.
 */
export const ORCA_POSITION = {
  length: 216,
  liquidity: 72,
  feeOwedA: 112,
  feeOwedB: 136,
} as const;

// ------------------------------------------------------------ instructions

/**
 * Jupiter v6 `route(route_plan: Vec<RoutePlanStep>, in_amount: u64, quoted_out_amount: u64,
 * slippage_bps: u16, platform_fee_bps: u8)`, from the published CPI IDL (`jup-ag/jupiter-cpi`).
 *
 * Its accounts start `token_program`, `user_transfer_authority` (the one signer),
 * `user_source_token_account` and `user_destination_token_account`; `destination_token_account`,
 * `destination_mint`, `platform_fee_account`, `event_authority`, `program` and the route's own
 * accounts follow. A template passes the first four itself and forwards the rest as an account
 * group. Ask the Swap API for `useSharedAccounts: false`: the default, `shared_accounts_route`,
 * orders its accounts differently.
 */
export const JUPITER_ROUTE = anchorDiscriminator('route');

/** The accounts at the head of `route`'s list that a template passes itself. */
export const JUPITER_ROUTE_FIXED_ACCOUNTS = 4;

/** `in_amount`, `quoted_out_amount`, `slippage_bps` and `platform_fee_bps`: the bytes after the plan. */
export const JUPITER_ROUTE_TAIL_LENGTH = 19;

/** `route` instruction data as the Swap API returns it, split into the parts templates take. */
export function splitJupiterRoute(data: Uint8Array) {
  // Discriminator, the plan's u32 length prefix, and the tail.
  const isRoute =
    data.length >= 8 + 4 + JUPITER_ROUTE_TAIL_LENGTH &&
    JUPITER_ROUTE.every((byte, index) => data[index] === byte);
  if (!isRoute) {
    throw new Error('Not Jupiter `route` data; request the Swap API with useSharedAccounts: false');
  }
  const tailStart = data.length - JUPITER_ROUTE_TAIL_LENGTH;
  const tail = new DataView(data.buffer, data.byteOffset + tailStart, JUPITER_ROUTE_TAIL_LENGTH);
  return {
    /** Everything after the discriminator. */
    args: data.slice(8),
    /** The Borsh `route_plan` vector, length prefix included. */
    routePlan: data.slice(8, tailStart),
    inAmount: tail.getBigUint64(0, true),
    quotedOutAmount: tail.getBigUint64(8, true),
    slippageBps: tail.getUint16(16, true),
    platformFeeBps: tail.getUint8(18),
  };
}

/** The shape the Jupiter Swap API returns for `swapInstruction`. */
export interface JupiterSwapInstruction {
  programId: string;
  accounts: readonly { pubkey: string; isSigner: boolean; isWritable: boolean }[];
  /** Base64. */
  data: string;
}

/** One quoted leg: its quote's `inputMint` and `outputMint`, and the Swap API's `swapInstruction`. */
export interface JupiterLeg {
  inputMint: string;
  outputMint: string;
  swapInstruction: JupiterSwapInstruction;
}

/** `route`'s own accounts, before its steps': the four a template passes, then five more. */
const ROUTE_ACCOUNTS_BEFORE_STEPS = 9;

/**
 * Joins two quoted legs, wrapped SOL to another token and back, into one `route`: a round trip,
 * which the Swap API won't quote whole. The plan is the first leg's step, then the second's with
 * its input and output indices moved up by one, so that it spends exactly what the first produced.
 * The accounts are the second leg's own nine, then both legs' steps. It sells the first leg's input
 * for at least the second leg's quote less its slippage. `round_trip` in
 * `tests/protocols/tests/jito_tip.rs` is the same join in Rust.
 *
 * Each leg must be one step: a step ends in its two indices, but the `Swap` enum before them has
 * variants of different lengths, so a longer plan can't be renumbered without decoding it.
 *
 * Returns the joined `route` data, and its accounts from the fifth on, which a run passes as its
 * group: the template passes the token program, the signer and the wrapped SOL account twice.
 */
export function joinRoundTrip(
  first: JupiterLeg,
  second: JupiterLeg,
): { routeData: Uint8Array<ArrayBuffer>; strategyAccounts: { address: Address; writable: boolean }[] } {
  if (first.inputMint !== WRAPPED_SOL_MINT || second.outputMint !== WRAPPED_SOL_MINT) {
    throw new Error('A round trip starts and ends in wrapped SOL');
  }
  const routes = [first, second].map(({ swapInstruction }) => {
    if (swapInstruction.programId !== JUPITER_V6) throw new Error('Both legs must be Jupiter v6 `route`');
    return splitJupiterRoute(Uint8Array.from(atob(swapInstruction.data), (char) => char.charCodeAt(0)));
  }) as [ReturnType<typeof splitJupiterRoute>, ReturnType<typeof splitJupiterRoute>];
  const source = first.swapInstruction.accounts[2]?.pubkey;
  if (source === undefined || source !== second.swapInstruction.accounts[3]?.pubkey) {
    throw new Error('The legs must start and end in the same token account');
  }

  const steps = routes.flatMap(({ routePlan }, position) => {
    const [count, step] = [new DataView(routePlan.buffer, routePlan.byteOffset).getUint32(0, true), routePlan.slice(4)];
    if (count !== 1) throw new Error('Each leg must be a single step');
    if (step.at(-2) !== 0 || step.at(-1) !== 1) throw new Error('A single step reads index 0 and writes index 1');
    return [...step.slice(0, -2), position, position + 1];
  });
  const tail = new DataView(new ArrayBuffer(JUPITER_ROUTE_TAIL_LENGTH));
  tail.setBigUint64(0, routes[0].inAmount, true);
  tail.setBigUint64(8, routes[1].quotedOutAmount, true);
  tail.setUint16(16, routes[1].slippageBps, true);
  tail.setUint8(18, routes[1].platformFeeBps);
  const routeData = Uint8Array.from([...JUPITER_ROUTE, 2, 0, 0, 0, ...steps, ...new Uint8Array(tail.buffer)]);

  const accounts = [
    ...second.swapInstruction.accounts.slice(0, ROUTE_ACCOUNTS_BEFORE_STEPS),
    ...first.swapInstruction.accounts.slice(ROUTE_ACCOUNTS_BEFORE_STEPS),
    ...second.swapInstruction.accounts.slice(ROUTE_ACCOUNTS_BEFORE_STEPS),
  ];
  return {
    routeData,
    strategyAccounts: accounts
      .slice(JUPITER_ROUTE_FIXED_ACCOUNTS)
      .map((meta) => ({ address: address(meta.pubkey), writable: meta.isWritable })),
  };
}

/** `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount: u64)`. */
export const KAMINO_DEPOSIT = anchorDiscriminator('deposit_reserve_liquidity_and_obligation_collateral_v2');
/** `repay_obligation_liquidity_v2(liquidity_amount: u64)`. */
export const KAMINO_REPAY = anchorDiscriminator('repay_obligation_liquidity_v2');
/** `refresh_reserve()`. */
export const KAMINO_REFRESH_RESERVE = anchorDiscriminator('refresh_reserve');
/** `refresh_obligation()`. */
export const KAMINO_REFRESH_OBLIGATION = anchorDiscriminator('refresh_obligation');
/**
 * `liquidate_obligation_and_redeem_reserve_collateral_v2(liquidity_amount,
 * min_acceptable_received_liquidity_amount, max_allowed_ltv_override_percent)`.
 */
export const KAMINO_LIQUIDATE = anchorDiscriminator('liquidate_obligation_and_redeem_reserve_collateral_v2');
/** `collect_fees()`. */
export const ORCA_COLLECT_FEES = anchorDiscriminator('collect_fees');
/**
 * `update_fees_and_rewards()`: folds the pool's fee growth into a position's `fee_owed_*`. Needs
 * no signature; fails with `LiquidityZero` (6012) on a position without liquidity.
 */
export const ORCA_UPDATE_FEES_AND_REWARDS = anchorDiscriminator('update_fees_and_rewards');
/**
 * `increase_liquidity_by_token_amounts_v2(method: IncreaseLiquidityMethod, remaining_accounts_info:
 * Option<RemainingAccountsInfo>)`. It takes `increase_liquidity_v2`'s accounts: whirlpool,
 * token_program_a, token_program_b, memo_program, position_authority, position,
 * position_token_account, token_mint_a, token_mint_b, token_owner_account_a,
 * token_owner_account_b, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper.
 */
export const ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2 = anchorDiscriminator(
  'increase_liquidity_by_token_amounts_v2',
);
/**
 * `IncreaseLiquidityMethod::ByTokenAmounts { token_max_a: u64, token_max_b: u64, min_sqrt_price:
 * u128, max_sqrt_price: u128 }`, the enum's only variant, as its one-byte Borsh tag.
 */
export const ORCA_BY_TOKEN_AMOUNTS = Uint8Array.of(0);
/** `lending_account_withdraw(amount: u64, withdraw_all: Option<bool>)`. */
export const MARGINFI_WITHDRAW = anchorDiscriminator('lending_account_withdraw');
/** `lending_account_deposit(amount: u64, ...)`. */
export const MARGINFI_DEPOSIT = anchorDiscriminator('lending_account_deposit');

/** Borsh `Option::None`. */
export const OPTION_NONE = Uint8Array.of(0);
/** Borsh `false`. */
export const BORSH_FALSE = Uint8Array.of(0);
/** Borsh `true`. */
export const BORSH_TRUE = Uint8Array.of(1);
