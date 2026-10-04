/** pump.fun's accounts, which both pump.fun runs derive from a coin's mint and its curve. */
// #region pump-accounts
import { address, getAddressDecoder, getAddressEncoder, getProgramDerivedAddress, type Address } from '@solana/kit';

import { ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES, TOKEN_2022_PROGRAM_ADDRESS_BYTES } from '@jac0xb/ballista';
import { PUMP_FUN } from '../shared.js';

const decoder = getAddressDecoder();
const encoder = getAddressEncoder();
export const TOKEN_2022_PROGRAM = decoder.decode(TOKEN_2022_PROGRAM_ADDRESS_BYTES);
const ASSOCIATED_TOKEN_PROGRAM = decoder.decode(ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES);

/** pump.fun's `Global`, `["global"]`. */
export const PUMP_GLOBAL = address('4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf');
/** Anchor's event authority, `["__event_authority"]`: pump.fun logs each trade by calling itself. */
export const PUMP_EVENT_AUTHORITY = address('Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1');
/** `["global_volume_accumulator"]`. */
export const PUMP_GLOBAL_VOLUME_ACCUMULATOR = address('Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y');
/** Pump Fees' `["fee_config", pump.fun]`. */
export const PUMP_FEE_CONFIG = address('8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt');
/** `Global.fee_recipient`, which an ordinary coin's trades may pay. `Global.fee_recipients` lists seven more. */
export const PUMP_FEE_RECIPIENT = address('62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV');
/** `Global.reserved_fee_recipient`, which a mayhem-mode coin's trades may pay instead. */
export const PUMP_RESERVED_FEE_RECIPIENT = address('GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS');
/** One of `Global.buyback_fee_recipients`, which every trade pays. */
export const PUMP_BUYBACK_FEE_RECIPIENT = address('9M4giFFMxmFGXtc3feFzRai56WbBqehoSeRE5GK7gf7');

/** A pump.fun coin, as its bonding curve describes it. */
export interface PumpCoin {
  mint: Address;
  /** `BondingCurve.creator`: the 32 bytes at offset 49 of the curve's data. */
  creator: Address;
  /** `BondingCurve.is_mayhem_mode`: the byte at offset 81. */
  mayhem: boolean;
}

/** Reads a coin's creator and mode from its curve's account data. */
export function pumpCoin(mint: Address, curveData: Uint8Array): PumpCoin {
  return { mint, creator: decoder.decode(curveData.subarray(49, 81)), mayhem: curveData[81] === 1 };
}

const pumpPda = async (seeds: Parameters<typeof getProgramDerivedAddress>[0]['seeds']) =>
  (await getProgramDerivedAddress({ programAddress: address(PUMP_FUN), seeds }))[0];

/** `owner`'s associated token account for the Token-2022 mint `mint`. */
export async function token2022Ata(owner: Address, mint: Address): Promise<Address> {
  const [ata] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM,
    seeds: [encoder.encode(owner), TOKEN_2022_PROGRAM_ADDRESS_BYTES, encoder.encode(mint)],
  });
  return ata;
}

/** `["user_volume_accumulator", user]`. pump.fun creates it on the user's first buy. */
export function pumpUserVolumeAccumulator(user: Address): Promise<Address> {
  return pumpPda(['user_volume_accumulator', encoder.encode(user)]);
}

/** The accounts pump.fun's `buy` and `sell` take for one coin. */
export async function pumpCoinAccounts(coin: PumpCoin) {
  const bondingCurve = await pumpPda(['bonding-curve', encoder.encode(coin.mint)]);
  return {
    mint: coin.mint,
    bondingCurve,
    /** The curve's own token account, which coins are bought from and sold into. */
    curveTokenAccount: await token2022Ata(bondingCurve, coin.mint),
    /** `["creator-vault", creator]`: the creator's fees accrue here. */
    creatorVault: await pumpPda(['creator-vault', encoder.encode(coin.creator)]),
    /** `["bonding-curve-v2", mint]`, which `buy` and `sell` take whether or not it exists. */
    bondingCurveV2: await pumpPda(['bonding-curve-v2', encoder.encode(coin.mint)]),
    /** A fee recipient of the kind the coin's mode needs. */
    feeRecipient: coin.mayhem ? PUMP_RESERVED_FEE_RECIPIENT : PUMP_FEE_RECIPIENT,
  };
}
// #endregion pump-accounts
