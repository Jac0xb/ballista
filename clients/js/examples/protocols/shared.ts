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
/** The instructions sysvar. Every Kamino v2 lending instruction takes it as an account. */
export const SYSVAR_INSTRUCTIONS = 'Sysvar1nstructions1111111111111111111111111' as const;
/** Kamino Farms, which Kamino Lend invokes whenever a lending instruction touches a reserve with a farm. */
export const KAMINO_FARMS = 'FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr' as const;

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

/** SPL Token `Mint`: `decimals` is the u8 at offset 44 of the 82-byte layout. */
export const SPL_MINT = { length: 82, decimals: 44 } as const;

/**
 * Pyth `PriceUpdateV2`.
 *
 * The offsets depend on the account's verification level, which is why `verificationLevel` is
 * read first. `VerificationLevel` is a Borsh enum: `Full` serializes as one byte, `Partial {
 * num_signatures: u8 }` as two. Anchor writes the struct sequentially, so a `Full` account puts
 * every later field one byte earlier than a `Partial` one — and the account is allocated at the
 * larger size either way, leaving a trailing byte unused.
 *
 * Deriving offsets from `PriceUpdateV2::LEN = 8 + 32 + 2 + 32 + 8 + …` therefore gives the
 * `Partial` layout, while most accounts on devnet are `Full`. Templates here require `Full` — it is
 * the stronger guarantee anyway, having all the signatures rather than some — and read at the
 * `Full` offsets.
 */
export const PYTH = {
  length: 134,
  /** 1 is `Full`, 0 is `Partial`. Read this before trusting any offset below. */
  verificationLevel: 40,
  verificationLevelFull: 1,
  /** Offsets for a `Full` account. A `Partial` one shifts each by one. */
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
/** `increase_liquidity(liquidity_amount: u128, token_max_a: u64, token_max_b: u64)`. */
export const ORCA_INCREASE_LIQUIDITY = anchorDiscriminator('increase_liquidity');
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
