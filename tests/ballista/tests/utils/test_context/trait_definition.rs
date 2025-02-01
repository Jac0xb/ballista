use crate::utils::Result;
use solana_program::{hash::Hash, pubkey::Pubkey};
use solana_program_test::{BanksTransactionResultWithMetadata, ProgramTestError};
use solana_sdk::{
    account::AccountSharedData, signature::Keypair, transaction::VersionedTransaction,
};

pub trait TestContextSync {
    fn warp_to_slot(&mut self, slot: u64) -> std::result::Result<(), ProgramTestError>;
    fn payer(&self) -> Keypair;
}

#[async_trait::async_trait]
pub trait TestContextAsync {
    async fn get_blockhash(&mut self) -> Hash;
    async fn set_account(&mut self, pubkey: &Pubkey, account: &AccountSharedData);
    async fn get_account(&mut self, pubkey: Pubkey) -> Option<solana_sdk::account::Account>;
    async fn fund_account(&mut self, address: Pubkey, lamports: u64) -> Result<()>;
    async fn get_minimum_balance_for_rent_exemption(&mut self, data_len: usize) -> u64;
    async fn process_transaction(
        &mut self,
        tx: &VersionedTransaction,
    ) -> Result<BanksTransactionResultWithMetadata>;
    async fn get_slot(&mut self) -> u64;
}

pub trait TestContext: TestContextSync + TestContextAsync {}
