//! Kamino Lend (klend): instructions for setup and the contract tests, and the account fields the
//! scenarios read.
//!
//! Instructions come from `klend-interface`, pinned to the build deployed on mainnet.
//! - It speaks solana 2.x (`solana-pubkey` 2, `solana-instruction` 2), and this crate 3.x/4.x.
//!   [`address`], [`pubkey`] and [`instruction`] convert field by field; nothing outside this
//!   module sees a 2.x type.
//! - Anything that moves tokens uses its low-level `instructions::*` builders, with the vaults read
//!   from the `Reserve`. Its high-level helpers derive vaults from seeds, and the main market's SOL
//!   and USDC reserves predate those seeds. Refreshes use the helpers, which never touch vaults.
//! - klend is built with Anchor 0.29 without `allow-missing-optionals`, so every optional account
//!   must be present, with klend's own program ID meaning "none". klend-interface does that.

use {
    klend_interface::{self as klend, state},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey_v2::Pubkey,
};

pub const KLEND: Address = Address::from_str_const("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
/// Kamino Farms, which klend invokes whenever a deposit, repayment or liquidation touches a
/// reserve with a farm.
pub const FARMS: Address = Address::from_str_const("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
/// Every v2 lending instruction takes the instructions sysvar (only v1 reads it).
pub const INSTRUCTIONS_SYSVAR: Address =
    Address::from_str_const("Sysvar1nstructions1111111111111111111111111");

/// How many accounts each v2 instruction takes before its farm tail, which the templates forward
/// as `farmAccounts`.
pub const DEPOSIT_DECLARED: usize = 14;
pub const REPAY_DECLARED: usize = 9;
pub const LIQUIDATE_DECLARED: usize = 20;

/// `deposit_reserve_liquidity_and_obligation_collateral`, v1: its accounts are v2's first 14.
const DEPOSIT_V1: [u8; 8] = [0x81, 0xc7, 0x04, 0x02, 0xde, 0x27, 0x1a, 0x2e];

// ------------------------------------------------------------------------------- conversion

pub fn address(key: &Pubkey) -> Address {
    Address::new_from_array(key.to_bytes())
}

pub fn pubkey(address: &Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

pub fn instruction(instruction: solana_instruction_v2::Instruction) -> Instruction {
    Instruction {
        program_id: address(&instruction.program_id),
        accounts: instruction
            .accounts
            .into_iter()
            .map(|meta| AccountMeta {
                pubkey: address(&meta.pubkey),
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            })
            .collect(),
        data: instruction.data,
    }
}

// --------------------------------------------------------------------------------- accounts

fn account_data(svm: &LiteSVM, address: &Address) -> Vec<u8> {
    svm.get_account(address)
        .unwrap_or_else(|| panic!("{address} is not in the SVM"))
        .data
}

pub fn reserve(svm: &LiteSVM, address: &Address) -> state::Reserve {
    *klend::from_account_data::<state::Reserve>(&account_data(svm, address))
        .unwrap_or_else(|error| panic!("{address} is not a klend reserve: {error:?}"))
}

pub fn obligation(svm: &LiteSVM, address: &Address) -> state::Obligation {
    *klend::from_account_data::<state::Obligation>(&account_data(svm, address))
        .unwrap_or_else(|error| panic!("{address} is not a klend obligation: {error:?}"))
}

/// The accounts a reserve names, read from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReserveAccounts {
    pub lending_market: Address,
    pub liquidity_mint: Address,
    pub supply_vault: Address,
    pub fee_vault: Address,
    pub collateral_mint: Address,
    pub collateral_supply: Address,
    pub token_program: Address,
    pub farm_collateral: Option<Address>,
    pub farm_debt: Option<Address>,
    pub scope_prices: Option<Address>,
}

pub fn reserve_accounts(svm: &LiteSVM, reserve: &Address) -> ReserveAccounts {
    let stored = self::reserve(svm, reserve);
    let named = |key: Pubkey| (key != Pubkey::default()).then(|| address(&key));
    ReserveAccounts {
        lending_market: address(&stored.lending_market),
        liquidity_mint: address(&stored.liquidity.mint_pubkey),
        supply_vault: address(&stored.liquidity.supply_vault),
        fee_vault: address(&stored.liquidity.fee_vault),
        collateral_mint: address(&stored.collateral.mint_pubkey),
        collateral_supply: address(&stored.collateral.supply_vault),
        token_program: address(&stored.liquidity.token_program),
        farm_collateral: named(stored.farm_collateral),
        farm_debt: named(stored.farm_debt),
        scope_prices: named(stored.config.token_info.scope_configuration.price_feed),
    }
}

/// The cTokens `obligation` holds from `reserve`, or zero.
pub fn deposited(svm: &LiteSVM, obligation: &Address, reserve: &Address) -> u64 {
    self::obligation(svm, obligation)
        .deposits
        .iter()
        .find(|deposit| address(&deposit.deposit_reserve) == *reserve)
        .map_or(0, |deposit| deposit.deposited_amount)
}

/// What `obligation` owes `reserve`: a fraction with 60 fractional bits, or zero.
pub fn borrowed_sf(svm: &LiteSVM, obligation: &Address, reserve: &Address) -> u128 {
    self::obligation(svm, obligation)
        .borrows
        .iter()
        .find(|borrow| address(&borrow.borrow_reserve) == *reserve)
        .map_or(0, |borrow| u128::from(borrow.borrowed_amount_sf))
}

/// Whether klend would liquidate `obligation` as its last refresh left it: its debt, adjusted by
/// borrow factor, has reached its unhealthy borrow value.
pub fn is_liquidatable(svm: &LiteSVM, obligation: &Address) -> bool {
    let stored = self::obligation(svm, obligation);
    u128::from(stored.borrow_factor_adjusted_debt_value_sf)
        >= u128::from(stored.unhealthy_borrow_value_sf)
}

// ------------------------------------------------------------------------------------- PDAs

pub fn lending_market_authority(market: &Address) -> Address {
    address(&klend::pda::lending_market_authority(&klend::KLEND_PROGRAM_ID, &pubkey(market)).0)
}

pub fn user_metadata(owner: &Address) -> Address {
    address(&klend::pda::user_metadata(&klend::KLEND_PROGRAM_ID, &pubkey(owner)).0)
}

/// `owner`'s vanilla obligation in `market`: tag 0, id 0, no seed accounts.
pub fn vanilla_obligation(owner: &Address, market: &Address) -> Address {
    let none = Pubkey::default();
    address(
        &klend::pda::obligation(
            &klend::KLEND_PROGRAM_ID,
            0,
            0,
            &pubkey(owner),
            &pubkey(market),
            &none,
            &none,
        )
        .0,
    )
}

/// `obligation`'s user state in `farm`.
pub fn obligation_farm(farm: &Address, obligation: &Address) -> Address {
    address(&klend::pda::farms_user_state(&pubkey(farm), &pubkey(obligation)).0)
}

// ----------------------------------------------------------------------------- instructions

/// `init_user_metadata`, paid by `owner`, with no referrer and no lookup table.
pub fn init_user_metadata(owner: &Address) -> Instruction {
    instruction(klend::instructions::init_user_metadata(
        klend::instructions::InitUserMetadataAccounts {
            owner: pubkey(owner),
            fee_payer: pubkey(owner),
            user_metadata: pubkey(&user_metadata(owner)),
            referrer_user_metadata: None,
        },
        Pubkey::default(),
    ))
}

/// `init_obligation` for `owner`'s vanilla obligation in `market`, paid by `owner`. Tag 0 takes
/// the default key for both seed accounts, which is the System program's address.
pub fn init_obligation(owner: &Address, market: &Address) -> Instruction {
    instruction(klend::instructions::init_obligation(
        klend::instructions::InitObligationAccounts {
            obligation_owner: pubkey(owner),
            fee_payer: pubkey(owner),
            obligation: pubkey(&vanilla_obligation(owner, market)),
            lending_market: pubkey(market),
            seed1_account: Pubkey::default(),
            seed2_account: Pubkey::default(),
            owner_user_metadata: pubkey(&user_metadata(owner)),
        },
        klend::types::InitObligationArgs { tag: 0, id: 0 },
    ))
}

/// `init_obligation_farms_for_reserve` in collateral mode: `obligation`'s user state in `reserve`'s
/// collateral farm. klend needs it before the obligation's first deposit into a reserve with one.
/// `owner` is passed because the obligation may not exist yet when this is built.
pub fn init_obligation_farm(
    svm: &LiteSVM,
    payer: &Address,
    owner: &Address,
    obligation: &Address,
    reserve: &Address,
) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    let farm = r
        .farm_collateral
        .unwrap_or_else(|| panic!("reserve {reserve} has no collateral farm"));
    instruction(klend::instructions::init_obligation_farms_for_reserve(
        klend::instructions::InitObligationFarmsForReserveAccounts {
            payer: pubkey(payer),
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
            reserve: pubkey(reserve),
            reserve_farm_state: pubkey(&farm),
            obligation_farm: pubkey(&obligation_farm(&farm, obligation)),
            lending_market: pubkey(&r.lending_market),
        },
        0, // ReserveFarmKind::Collateral
    ))
}

/// What a transaction needs before a klend v2 call on `obligation`:
/// - `refresh_reserve` for each reserve the obligation holds, and for each of `also`;
/// - then `refresh_obligation`, with the held reserves writable: deposits, then borrows.
pub fn refreshes(svm: &LiteSVM, obligation: &Address, also: &[Address]) -> Vec<Instruction> {
    let held = klend::ObligationInfo::from_account_data(
        pubkey(obligation),
        &account_data(svm, obligation),
    )
    .unwrap_or_else(|error| panic!("{obligation} is not an obligation: {error:?}"));
    let market = self::obligation(svm, obligation).lending_market;
    let mut reserves: Vec<Pubkey> = Vec::new();
    for reserve in held
        .deposit_reserves
        .iter()
        .chain(&held.borrow_reserves)
        .copied()
        .chain(also.iter().map(pubkey))
    {
        if !reserves.contains(&reserve) {
            reserves.push(reserve);
        }
    }
    let infos: Vec<klend::ReserveInfo> = reserves
        .iter()
        .map(|reserve| {
            klend::ReserveInfo::from_account_data(*reserve, &account_data(svm, &address(reserve)))
                .expect("a reserve")
        })
        .collect();
    let mut instructions: Vec<Instruction> = infos
        .iter()
        .map(|info| instruction(klend::helpers::refresh_reserve(info)))
        .collect();
    instructions.push(instruction(klend::helpers::refresh_obligation(
        &market, &held, &infos,
    )));
    instructions
}

/// `deposit_reserve_liquidity_and_obligation_collateral_v2`: moves `amount` of `reserve`'s
/// liquidity from `source` into `obligation`. The farm accounts are included when the reserve has
/// a collateral farm.
pub fn deposit(
    svm: &LiteSVM,
    owner: &Address,
    obligation: &Address,
    reserve: &Address,
    source: &Address,
    amount: u64,
) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    instruction(
        klend::instructions::deposit_reserve_liquidity_and_obligation_collateral_v2(
            klend::instructions::DepositReserveLiquidityAndObligationCollateralV2Accounts {
                owner: pubkey(owner),
                obligation: pubkey(obligation),
                lending_market: pubkey(&r.lending_market),
                lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
                reserve: pubkey(reserve),
                reserve_liquidity_mint: pubkey(&r.liquidity_mint),
                reserve_liquidity_supply: pubkey(&r.supply_vault),
                reserve_collateral_mint: pubkey(&r.collateral_mint),
                reserve_destination_deposit_collateral: pubkey(&r.collateral_supply),
                user_source_liquidity: pubkey(source),
                placeholder_user_destination_collateral: None,
                liquidity_token_program: pubkey(&r.token_program),
                obligation_farm_user_state: r
                    .farm_collateral
                    .map(|farm| pubkey(&obligation_farm(&farm, obligation))),
                reserve_farm_state: r.farm_collateral.map(|farm| pubkey(&farm)),
            },
            amount,
        ),
    )
}

/// The same deposit under v1's discriminator with v1's 14 accounts. klend refuses it by CPI.
pub fn deposit_v1(
    svm: &LiteSVM,
    owner: &Address,
    obligation: &Address,
    reserve: &Address,
    source: &Address,
    amount: u64,
) -> Instruction {
    let mut v1 = deposit(svm, owner, obligation, reserve, source, amount);
    v1.accounts.truncate(DEPOSIT_DECLARED);
    v1.data[..8].copy_from_slice(&DEPOSIT_V1);
    v1
}

/// `borrow_obligation_liquidity_v2` into `destination`, with no referrer and the reserve's debt
/// farm if it has one. The obligation is in no elevation group, so there are no remaining accounts.
pub fn borrow(
    svm: &LiteSVM,
    owner: &Address,
    obligation: &Address,
    reserve: &Address,
    destination: &Address,
    amount: u64,
) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    instruction(klend::instructions::borrow_obligation_liquidity_v2(
        klend::instructions::BorrowObligationLiquidityV2Accounts {
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market: pubkey(&r.lending_market),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
            borrow_reserve: pubkey(reserve),
            borrow_reserve_liquidity_mint: pubkey(&r.liquidity_mint),
            reserve_source_liquidity: pubkey(&r.supply_vault),
            borrow_reserve_liquidity_fee_receiver: pubkey(&r.fee_vault),
            user_destination_liquidity: pubkey(destination),
            referrer_token_state: None,
            token_program: pubkey(&r.token_program),
            obligation_farm_user_state: r
                .farm_debt
                .map(|farm| pubkey(&obligation_farm(&farm, obligation))),
            reserve_farm_state: r.farm_debt.map(|farm| pubkey(&farm)),
        },
        amount,
        vec![],
    ))
}

/// `repay_obligation_liquidity_v2` from `source`, with the reserve's debt farm if it has one.
pub fn repay(
    svm: &LiteSVM,
    owner: &Address,
    obligation: &Address,
    reserve: &Address,
    source: &Address,
    amount: u64,
) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    instruction(klend::instructions::repay_obligation_liquidity_v2(
        klend::instructions::RepayObligationLiquidityV2Accounts {
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market: pubkey(&r.lending_market),
            repay_reserve: pubkey(reserve),
            reserve_liquidity_mint: pubkey(&r.liquidity_mint),
            reserve_destination_liquidity: pubkey(&r.supply_vault),
            user_source_liquidity: pubkey(source),
            token_program: pubkey(&r.token_program),
            obligation_farm_user_state: r
                .farm_debt
                .map(|farm| pubkey(&obligation_farm(&farm, obligation))),
            reserve_farm_state: r.farm_debt.map(|farm| pubkey(&farm)),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
        },
        amount,
        vec![],
    ))
}

/// The liquidator's side of a liquidation.
#[derive(Clone, Copy, Debug)]
pub struct Liquidator {
    pub signer: Address,
    /// Pays the repayment: a token account for the repaid reserve's mint.
    pub source_liquidity: Address,
    /// A token account for the withdrawn reserve's cToken mint.
    pub destination_collateral: Address,
    /// Where klend pays the seized collateral, redeemed: a token account for its liquidity mint.
    pub destination_liquidity: Address,
}

/// `liquidate_obligation_and_redeem_reserve_collateral_v2`. It repays up to `amount` of
/// `repay_reserve`'s debt and seizes `withdraw_reserve`'s collateral, with the withdrawn reserve's
/// collateral farm and the repaid reserve's debt farm when they exist. No LTV override.
pub fn liquidate(
    svm: &LiteSVM,
    liquidator: &Liquidator,
    obligation: &Address,
    repay_reserve: &Address,
    withdraw_reserve: &Address,
    amount: u64,
    min_received: u64,
) -> Instruction {
    let repay = reserve_accounts(svm, repay_reserve);
    let withdraw = reserve_accounts(svm, withdraw_reserve);
    let user_state = |farm: Address| pubkey(&obligation_farm(&farm, obligation));
    instruction(
        klend::instructions::liquidate_obligation_and_redeem_reserve_collateral_v2(
            klend::instructions::LiquidateObligationAndRedeemReserveCollateralV2Accounts {
                liquidator: pubkey(&liquidator.signer),
                obligation: pubkey(obligation),
                lending_market: pubkey(&repay.lending_market),
                lending_market_authority: pubkey(&lending_market_authority(&repay.lending_market)),
                repay_reserve: pubkey(repay_reserve),
                repay_reserve_liquidity_mint: pubkey(&repay.liquidity_mint),
                repay_reserve_liquidity_supply: pubkey(&repay.supply_vault),
                withdraw_reserve: pubkey(withdraw_reserve),
                withdraw_reserve_liquidity_mint: pubkey(&withdraw.liquidity_mint),
                withdraw_reserve_collateral_mint: pubkey(&withdraw.collateral_mint),
                withdraw_reserve_collateral_supply: pubkey(&withdraw.collateral_supply),
                withdraw_reserve_liquidity_supply: pubkey(&withdraw.supply_vault),
                withdraw_reserve_liquidity_fee_receiver: pubkey(&withdraw.fee_vault),
                user_source_liquidity: pubkey(&liquidator.source_liquidity),
                user_destination_collateral: pubkey(&liquidator.destination_collateral),
                user_destination_liquidity: pubkey(&liquidator.destination_liquidity),
                repay_liquidity_token_program: pubkey(&repay.token_program),
                withdraw_liquidity_token_program: pubkey(&withdraw.token_program),
                collateral_obligation_farm_user_state: withdraw.farm_collateral.map(user_state),
                collateral_reserve_farm_state: withdraw.farm_collateral.map(|farm| pubkey(&farm)),
                debt_obligation_farm_user_state: repay.farm_debt.map(user_state),
                debt_reserve_farm_state: repay.farm_debt.map(|farm| pubkey(&farm)),
            },
            amount,
            min_received,
            0,
            vec![],
        ),
    )
}

// ---------------------------------------------------------------- the templates' farm groups

/// A deposit's `farmAccounts`: v2's accounts after the 14 the templates declare.
pub fn deposit_farm_accounts(
    svm: &LiteSVM,
    obligation: &Address,
    reserve: &Address,
) -> Vec<AccountMeta> {
    let any = Address::default();
    let mut all = deposit(svm, &any, obligation, reserve, &any, 0).accounts;
    all.split_off(DEPOSIT_DECLARED)
}

/// A repayment's `farmAccounts`: v2's accounts after the 9 declared. That is the debt farm pair,
/// the lending market authority and Farms.
pub fn repay_farm_accounts(
    svm: &LiteSVM,
    obligation: &Address,
    reserve: &Address,
) -> Vec<AccountMeta> {
    let any = Address::default();
    let mut all = repay(svm, &any, obligation, reserve, &any, 0).accounts;
    all.split_off(REPAY_DECLARED)
}

/// A liquidation's `farmAccounts`: v2's accounts after the 20 declared. That is the collateral farm
/// pair, the debt farm pair and Farms.
pub fn liquidation_farm_accounts(
    svm: &LiteSVM,
    obligation: &Address,
    repay_reserve: &Address,
    withdraw_reserve: &Address,
) -> Vec<AccountMeta> {
    let any = Address::default();
    let nobody = Liquidator {
        signer: any,
        source_liquidity: any,
        destination_collateral: any,
        destination_liquidity: any,
    };
    let mut all = liquidate(
        svm,
        &nobody,
        obligation,
        repay_reserve,
        withdraw_reserve,
        0,
        0,
    )
    .accounts;
    all.split_off(LIQUIDATE_DECLARED)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::lending::{self, MARKET, SOL_RESERVE, USDC_RESERVE},
    };

    const USDC_SUPPLY_VAULT: Address =
        Address::from_str_const("Bgq7trRgVMeq33yt235zM2onQ4bRDBsY5EWiTetF4qw6");
    const SOL_FARM: Address =
        Address::from_str_const("955xWFhSDcDiUgUr4sBRtCpTLiMd4H5uZLAmgtP3R3sX");
    const USDC_FARM: Address =
        Address::from_str_const("JAvnB9AKtgPsTEoKmn24Bq64UMoYcrtWtq42HHBdsPkh");

    /// Why nothing here derives a vault: these reserves predate the seeds klend-interface uses.
    #[test]
    fn the_reserves_vaults_are_stored_not_derived() {
        let svm = lending::svm();
        assert_eq!(
            reserve_accounts(&svm, &USDC_RESERVE).supply_vault,
            USDC_SUPPLY_VAULT
        );
        let derived =
            klend::pda::ReservePdas::derive(&klend::KLEND_PROGRAM_ID, &pubkey(&USDC_RESERVE));
        assert_ne!(address(&derived.liquidity_supply_vault), USDC_SUPPLY_VAULT);
    }

    /// The groups are the tails of klend-interface's own v2 instructions: the farm pair writable
    /// when the reserve has the farm, klend's ID read-only when it does not.
    #[test]
    fn the_farm_groups_are_klends_v2_tails() {
        let svm = lending::svm();
        let ob = Address::new_from_array([7; 32]);
        assert_eq!(
            deposit_farm_accounts(&svm, &ob, &USDC_RESERVE),
            [
                AccountMeta::new(obligation_farm(&USDC_FARM, &ob), false),
                AccountMeta::new(USDC_FARM, false),
                AccountMeta::new_readonly(FARMS, false),
            ]
        );
        assert_eq!(
            repay_farm_accounts(&svm, &ob, &USDC_RESERVE),
            [
                AccountMeta::new_readonly(KLEND, false),
                AccountMeta::new_readonly(KLEND, false),
                AccountMeta::new_readonly(lending_market_authority(&MARKET), false),
                AccountMeta::new_readonly(FARMS, false),
            ]
        );
        assert_eq!(
            liquidation_farm_accounts(&svm, &ob, &USDC_RESERVE, &SOL_RESERVE),
            [
                AccountMeta::new(obligation_farm(&SOL_FARM, &ob), false),
                AccountMeta::new(SOL_FARM, false),
                AccountMeta::new_readonly(KLEND, false),
                AccountMeta::new_readonly(KLEND, false),
                AccountMeta::new_readonly(FARMS, false),
            ]
        );
    }
}
