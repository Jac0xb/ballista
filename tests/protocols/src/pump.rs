//! pump.fun's bonding-curve program for the pump.fun templates' tests: the snapshot they run on,
//! pump.fun's own `buy` and `sell` and the accounts they take, and its curves as the SVM holds
//! them. Nothing here writes pump.fun state; wallets' balances go through [`crate::wallet`]
//! (write rule 1), and every other change is a real instruction: pump.fun's own `buy`, the
//! Associated Token program's `CreateIdempotent`, or the Address Lookup Table program's own.
//!
//! The snapshot (`scripts/snapshot/manifests/pump-fun.json`) holds three live curves of
//! Token-2022 coins, all SOL-paired: [`HJXC`] and [`AVYG`] are ordinary coins, and [`AVJ1`] is a
//! mayhem-mode coin, whose trades pay a reserved fee recipient instead. [`BFX4`]'s curve has
//! graduated.
//!
//! The instruction layouts are pump.fun's published IDL (`pump-fun/pump-public-docs`,
//! `idl/pump.json`) plus the two accounts its 2026 upgrade appended to `buy` and `sell`, as
//! `@pump-fun/pump-sdk` 2.0.0 passes them and mainnet transactions show them: the coin's
//! `bonding-curve-v2` PDA, then a buyback fee recipient.

use {
    crate::{
        snapshot::{warp, Snapshot},
        tx::{self, Failure},
        wallet::{fund, keypair, SOL},
    },
    ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID},
    litesvm::LiteSVM,
    sha2::{Digest, Sha256},
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_message::AddressLookupTableAccount,
    solana_sdk_ids::address_lookup_table,
    solana_signer::Signer,
    std::sync::OnceLock,
};

/// The pump.fun snapshot, written by `scripts/snapshot/snapshot.mjs` from
/// `scripts/snapshot/manifests/pump-fun.json`.
pub const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot-pump");

/// pump.fun's bonding-curve program.
pub const PUMP: Address = Address::from_str_const("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
/// The Pump Fees program, which pump.fun asks for its fee rates on every trade.
pub const PUMP_FEES: Address =
    Address::from_str_const("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const TOKEN_2022_PROGRAM: Address =
    Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// pump.fun's `Global`, the PDA `["global"]`.
pub const GLOBAL: Address = Address::from_str_const("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf");
/// The Pump Fees `FeeConfig` for pump.fun: `["fee_config", pump]` under [`PUMP_FEES`].
pub const FEE_CONFIG: Address =
    Address::from_str_const("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt");
/// `["global_volume_accumulator"]`.
pub const GLOBAL_VOLUME_ACCUMULATOR: Address =
    Address::from_str_const("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y");
/// Anchor's event authority, `["__event_authority"]`: pump.fun logs each trade by calling itself.
pub const EVENT_AUTHORITY: Address =
    Address::from_str_const("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");
/// `Global.fee_recipient`: an ordinary coin's trades may pay it.
pub const FEE_RECIPIENT: Address =
    Address::from_str_const("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");
/// `Global.reserved_fee_recipient`: a mayhem-mode coin's trades pay it or another reserved one.
pub const RESERVED_FEE_RECIPIENT: Address =
    Address::from_str_const("GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS");
/// One of `Global.buyback_fee_recipients`, the account the 2026 upgrade appended to every trade.
pub const BUYBACK_FEE_RECIPIENT: Address =
    Address::from_str_const("9M4giFFMxmFGXtc3feFzRai56WbBqehoSeRE5GK7gf7");

/// An ordinary coin, live on its curve.
pub const HJXC: Address = Address::from_str_const("HjXcr1A2k9mG2614UrCw5JnYnK7sAbe1y3EEeSmGmPUD");
/// A second ordinary coin, live on its curve.
pub const AVYG: Address = Address::from_str_const("AvygmeaEVDXFT8pUQdtr3pv6sUBf8CtMRsRtdzuPpump");
/// A mayhem-mode coin, live on its curve: its trades pay a reserved fee recipient.
pub const AVJ1: Address = Address::from_str_const("AVJ1akJaBGvMoCE7c39DhWGDL7Fb5eLtUgAw3Tfppump");
/// A coin whose curve has graduated to PumpSwap: `complete` is set.
pub const BFX4: Address = Address::from_str_const("BFX4LknRDLUiJvmAYNWWg8o6x5Wo21c2zXNxUDgipump");

/// Base units per whole token: every pump.fun coin has six decimals.
pub const TOKEN: u64 = 1_000_000;

/// `BondingCurve`, after Anchor's 8-byte discriminator: five `u64` reserves and the supply, then
/// `complete`, then the creator.
pub const CURVE_COMPLETE: usize = 48;
const CURVE_CREATOR: usize = 49;
const CURVE_MAYHEM: usize = 81;

/// pump.fun's own errors, from its IDL.
pub const TOO_MUCH_SOL_REQUIRED: u32 = 6002;
pub const BONDING_CURVE_COMPLETE: u32 = 6005;

/// Anchor's discriminator for the instruction handler `name`: `sha256("global:<name>")[..8]`.
pub fn discriminator(name: &str) -> [u8; 8] {
    Sha256::digest(format!("global:{name}"))[..8]
        .try_into()
        .unwrap()
}

/// A LiteSVM holding the pump.fun snapshot, with Ballista built from source. The snapshot is read
/// and checked once per test binary.
pub fn svm() -> LiteSVM {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| Snapshot::load(SNAPSHOT_DIR)).svm()
}

/// A fixed 32-byte key seed named by `label`.
pub fn seed(label: &str) -> [u8; 32] {
    let mut seed = [0; 32];
    seed.copy_from_slice(&Sha256::digest(label.as_bytes()));
    seed
}

fn pump_pda(seeds: &[&[u8]]) -> Address {
    Address::find_program_address(seeds, &PUMP).0
}

/// A coin and the per-coin accounts `buy` and `sell` take.
#[derive(Clone, Copy, Debug)]
pub struct Coin {
    pub mint: Address,
    /// `["bonding-curve", mint]`.
    pub bonding_curve: Address,
    /// The curve's own token account: its associated token account for the mint.
    pub curve_token_account: Address,
    /// `["creator-vault", curve.creator]`: the creator's fees accrue here.
    pub creator_vault: Address,
    /// `["bonding-curve-v2", mint]`. It does not exist for these coins; pump.fun takes it anyway.
    pub bonding_curve_v2: Address,
    /// Ordinary or reserved, as the coin's mode requires.
    pub fee_recipient: Address,
}

/// The coin `mint` as the SVM holds it now.
///
/// # Panics
///
/// If its curve is not in the SVM.
pub fn coin(svm: &LiteSVM, mint: Address) -> Coin {
    let bonding_curve = pump_pda(&[b"bonding-curve", mint.as_ref()]);
    let data = svm
        .get_account(&bonding_curve)
        .unwrap_or_else(|| panic!("the curve of {mint} is not in the SVM"))
        .data;
    let creator = Address::try_from(&data[CURVE_CREATOR..CURVE_CREATOR + 32]).unwrap();
    Coin {
        mint,
        bonding_curve,
        curve_token_account: associated_token_address(&bonding_curve, &mint),
        creator_vault: pump_pda(&[b"creator-vault", creator.as_ref()]),
        bonding_curve_v2: pump_pda(&[b"bonding-curve-v2", mint.as_ref()]),
        fee_recipient: if data[CURVE_MAYHEM] != 0 {
            RESERVED_FEE_RECIPIENT
        } else {
            FEE_RECIPIENT
        },
    }
}

/// A curve's state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Curve {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub complete: bool,
}

pub fn curve(svm: &LiteSVM, coin: &Coin) -> Curve {
    let data = svm.get_account(&coin.bonding_curve).unwrap().data;
    let u64_at = |offset: usize| u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
    Curve {
        virtual_token_reserves: u64_at(8),
        virtual_sol_reserves: u64_at(16),
        real_token_reserves: u64_at(24),
        real_sol_reserves: u64_at(32),
        complete: data[CURVE_COMPLETE] != 0,
    }
}

/// `["user_volume_accumulator", user]`: pump.fun creates it on a user's first buy.
pub fn user_volume_accumulator(user: &Address) -> Address {
    pump_pda(&[b"user_volume_accumulator", user.as_ref()])
}

/// `owner`'s associated token account for the Token-2022 mint `mint`.
pub fn associated_token_address(owner: &Address, mint: &Address) -> Address {
    Address::find_program_address(
        &[owner.as_ref(), TOKEN_2022_PROGRAM.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}

/// The Associated Token program's `CreateIdempotent` for `owner`'s Token-2022 account of `mint`,
/// paid by `payer`.
pub fn create_associated_token_account(
    payer: &Address,
    owner: &Address,
    mint: &Address,
) -> Instruction {
    Instruction {
        program_id: ASSOCIATED_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(associated_token_address(owner, mint), false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),
        ],
        data: vec![1],
    }
}

/// A trader: a funded wallet with an empty associated token account for each of `mints`, made by
/// the Associated Token program.
pub struct Trader {
    pub keypair: Keypair,
}

impl Trader {
    pub fn address(&self) -> Address {
        self.keypair.pubkey()
    }

    pub fn token_account(&self, mint: &Address) -> Address {
        associated_token_address(&self.address(), mint)
    }
}

pub fn trader(svm: &mut LiteSVM, label: &str, sol: u64, mints: &[Address]) -> Trader {
    let keypair = keypair(&seed(label));
    fund(svm, &keypair.pubkey(), sol);
    let create: Vec<Instruction> = mints
        .iter()
        .map(|mint| create_associated_token_account(&keypair.pubkey(), &keypair.pubkey(), mint))
        .collect();
    if !create.is_empty() {
        tx::send(svm, &keypair, &[], &create, &[]).unwrap_or_else(|failure| {
            panic!("creating {label}'s token accounts failed: {failure:?}")
        });
    }
    Trader { keypair }
}

/// pump.fun's `buy(amount, max_sol_cost, track_volume)` of `amount` base units for at most
/// `max_sol_cost` lamports, fees included, paid into `token_account`.
pub fn buy(
    user: &Address,
    token_account: &Address,
    coin: &Coin,
    amount: u64,
    max_sol_cost: u64,
) -> Instruction {
    let mut data = discriminator("buy").to_vec();
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&max_sol_cost.to_le_bytes());
    data.push(1); // track_volume: OptionBool(true), as pump.fun's SDK and the template send it
    Instruction {
        program_id: PUMP,
        accounts: vec![
            AccountMeta::new_readonly(GLOBAL, false),
            AccountMeta::new(coin.fee_recipient, false),
            AccountMeta::new_readonly(coin.mint, false),
            AccountMeta::new(coin.bonding_curve, false),
            AccountMeta::new(coin.curve_token_account, false),
            AccountMeta::new(*token_account, false),
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),
            AccountMeta::new(coin.creator_vault, false),
            AccountMeta::new_readonly(EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(PUMP, false),
            AccountMeta::new(GLOBAL_VOLUME_ACCUMULATOR, false),
            AccountMeta::new(user_volume_accumulator(user), false),
            AccountMeta::new_readonly(FEE_CONFIG, false),
            AccountMeta::new_readonly(PUMP_FEES, false),
            AccountMeta::new_readonly(coin.bonding_curve_v2, false),
            AccountMeta::new(BUYBACK_FEE_RECIPIENT, false),
        ],
        data,
    }
}

/// pump.fun's `sell(amount, min_sol_output)` of `amount` base units from `token_account`.
pub fn sell(
    user: &Address,
    token_account: &Address,
    coin: &Coin,
    amount: u64,
    min_sol_output: u64,
) -> Instruction {
    let mut data = discriminator("sell").to_vec();
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&min_sol_output.to_le_bytes());
    Instruction {
        program_id: PUMP,
        accounts: vec![
            AccountMeta::new_readonly(GLOBAL, false),
            AccountMeta::new(coin.fee_recipient, false),
            AccountMeta::new_readonly(coin.mint, false),
            AccountMeta::new(coin.bonding_curve, false),
            AccountMeta::new(coin.curve_token_account, false),
            AccountMeta::new(*token_account, false),
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new(coin.creator_vault, false),
            AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),
            AccountMeta::new_readonly(EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(PUMP, false),
            AccountMeta::new_readonly(FEE_CONFIG, false),
            AccountMeta::new_readonly(PUMP_FEES, false),
            AccountMeta::new_readonly(coin.bonding_curve_v2, false),
            AccountMeta::new(BUYBACK_FEE_RECIPIENT, false),
        ],
        data,
    }
}

/// A Token-2022 account's balance: the `u64` at offset 64, as in SPL Token.
///
/// # Panics
///
/// If the account does not exist or is not a Token-2022 account.
pub fn token_balance(svm: &LiteSVM, account: &Address) -> u64 {
    let account = svm
        .get_account(account)
        .unwrap_or_else(|| panic!("token account {account} does not exist"));
    assert_eq!(
        account.owner, TOKEN_2022_PROGRAM,
        "not a Token-2022 account"
    );
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

pub fn lamports(svm: &LiteSVM, address: &Address) -> u64 {
    svm.get_account(address)
        .map_or(0, |account| account.lamports)
}

/// Buys `amount` of `coin` for `trader` with pump.fun's own `buy`, outside any template, and
/// returns the lamports it cost the trader.
pub fn buy_directly(svm: &mut LiteSVM, trader: &Trader, coin: &Coin, amount: u64) -> u64 {
    let before = lamports(svm, &trader.address());
    let instruction = buy(
        &trader.address(),
        &trader.token_account(&coin.mint),
        coin,
        amount,
        100 * SOL,
    );
    let outcome = tx::send(svm, &trader.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("buying {} failed: {failure:?}", coin.mint));
    before - lamports(svm, &trader.address()) - outcome.fee
}

/// Asserts that pump.fun itself refused with its error `code`.
#[track_caller]
pub fn assert_pump_error(failure: &Failure, code: u32) {
    assert!(
        failure.program == PUMP && failure.code == Some(code),
        "expected pump.fun's error {code}, but {failure:?}"
    );
}

/// The keys `extend_lookup_table` adds per transaction here: 20 keep it well under the packet size.
const LOOKUP_TABLE_CHUNK: usize = 20;

/// An address lookup table of `addresses`, made by the Address Lookup Table program's own
/// `create_lookup_table` and `extend_lookup_table`, signed and paid for by `authority`. The clock
/// then moves one slot on (write rule 3), since a table's newest entries resolve only in a later
/// slot.
///
/// The table is derived from slot 0, the one slot LiteSVM's `SlotHashes` holds.
pub fn create_lookup_table(
    svm: &mut LiteSVM,
    authority: &Keypair,
    addresses: &[Address],
) -> AddressLookupTableAccount {
    let recent_slot = 0u64;
    let (table, bump) = Address::find_program_address(
        &[authority.pubkey().as_ref(), &recent_slot.to_le_bytes()],
        &address_lookup_table::ID,
    );
    let accounts = |authority_signs: bool| {
        vec![
            AccountMeta::new(table, false),
            AccountMeta::new_readonly(authority.pubkey(), authority_signs),
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ]
    };
    // `ProgramInstruction::CreateLookupTable { recent_slot, bump_seed }`, bincode-encoded.
    let mut create = 0u32.to_le_bytes().to_vec();
    create.extend_from_slice(&recent_slot.to_le_bytes());
    create.push(bump);
    let mut instructions = vec![Instruction {
        program_id: address_lookup_table::ID,
        accounts: accounts(true),
        data: create,
    }];
    for chunk in addresses.chunks(LOOKUP_TABLE_CHUNK) {
        // `ProgramInstruction::ExtendLookupTable { new_addresses }`.
        let mut extend = 2u32.to_le_bytes().to_vec();
        extend.extend_from_slice(&(chunk.len() as u64).to_le_bytes());
        for address in chunk {
            extend.extend_from_slice(address.as_ref());
        }
        instructions.push(Instruction {
            program_id: address_lookup_table::ID,
            accounts: accounts(true),
            data: extend,
        });
    }
    for instruction in instructions {
        tx::send(svm, authority, &[], &[instruction], &[])
            .unwrap_or_else(|failure| panic!("building the lookup table failed: {failure:?}"));
    }
    warp(svm, 1, 0);
    AddressLookupTableAccount {
        key: table,
        addresses: addresses.to_vec(),
    }
}
