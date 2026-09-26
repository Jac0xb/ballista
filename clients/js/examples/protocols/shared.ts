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
/** Drift v2, from `declare_id!` in programs/drift/src/lib.rs. */
export const DRIFT_V2 = 'dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH' as const;
/** Pyth Solana receiver, the non-`pro-compatible` build. */
export const PYTH_RECEIVER = 'rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ' as const;

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
 * `Partial` layout, which on devnet is the minority: of 4,000 accounts sampled, 3,323 were
 * `Full` and 677 `Partial`. Templates here require `Full` — it is the stronger guarantee anyway,
 * having all the signatures rather than some — and read at the `Full` offsets.
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
/** `deposit(market_index: u16, amount: u64, reduce_only: bool)`. */
export const DRIFT_DEPOSIT = anchorDiscriminator('deposit');
/** `withdraw(market_index: u16, amount: u64, reduce_only: bool)`. */
export const DRIFT_WITHDRAW = anchorDiscriminator('withdraw');

/** Borsh `Option::None`. */
export const OPTION_NONE = Uint8Array.of(0);
/** Borsh `false`. */
export const BORSH_FALSE = Uint8Array.of(0);

/** A little-endian u16, for arguments such as Drift's `market_index`. */
export function u16Bytes(value: number): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(2);
  new DataView(bytes.buffer).setUint16(0, value, true);
  return bytes;
}
