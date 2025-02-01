use super::{TestContext, TestContextAsync, TestContextSync};
use crate::utils::{clone_keypair, error::Error, process_transaction, Result};
use solana_program::{hash::Hash, pubkey::Pubkey};
use solana_program_test::ProgramTest;
use solana_program_test::{
    BanksTransactionResultWithMetadata, ProgramTestContext, ProgramTestError,
};
use solana_sdk::{
    account::AccountSharedData,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::{Transaction, VersionedTransaction},
};

pub fn create_program_test() -> ProgramTest {
    let mut test = ProgramTest::new("ballista", ballista_sdk::ID, None);

    // test.add_program(
    //     "spl_account_compression",
    //     spl_account_compression::id(),
    //     None,
    // );
    // test.add_program("spl_noop", spl_noop::id(), None);
    // test.add_program("mpl_token_metadata", mpl_token_metadata::ID, None);
    // test.add_program("mpl_bubblegum", mpl_bubblegum::ID, None);

    test.set_compute_max_units(1_400_000);

    test.prefer_bpf(true);

    test
}

pub struct BankClientTestContext {
    pub program_context: ProgramTestContext,
}

pub const DEFAULT_LAMPORTS_FUND_AMOUNT: u64 = 1_000_000_000;

impl TestContextSync for BankClientTestContext {
    fn warp_to_slot(&mut self, slot: u64) -> std::result::Result<(), ProgramTestError> {
        self.program_context.warp_to_slot(slot)
    }

    fn payer(&self) -> Keypair {
        clone_keypair(&self.program_context.payer)
    }
}

#[async_trait::async_trait]
impl TestContextAsync for BankClientTestContext {
    async fn get_blockhash(&mut self) -> Hash {
        self.program_context
            .get_new_latest_blockhash()
            .await
            .unwrap()
    }

    async fn set_account(&mut self, pubkey: &Pubkey, account: &AccountSharedData) {
        self.program_context.set_account(pubkey, account);
    }

    async fn get_account(&mut self, pubkey: Pubkey) -> Option<solana_sdk::account::Account> {
        self.program_context
            .banks_client
            .get_account(pubkey)
            .await
            .unwrap()
    }

    async fn fund_account(&mut self, address: Pubkey, lamports: u64) -> Result<()> {
        let payer = &self.program_context.payer;

        let tx = Transaction::new_signed_with_payer(
            &[system_instruction::transfer(
                &payer.pubkey(),
                &address,
                lamports,
            )],
            Some(&payer.pubkey()),
            &[payer],
            self.program_context.last_blockhash,
        );

        self.program_context
            .banks_client
            .process_transaction(tx)
            .await
            .map_err(|err| Box::new(Error::BanksClient(err)))
    }

    async fn get_minimum_balance_for_rent_exemption(&mut self, data_len: usize) -> u64 {
        let rent = self.program_context.banks_client.get_rent().await.unwrap();

        rent.minimum_balance(data_len)
    }

    async fn get_slot(&mut self) -> u64 {
        self.program_context
            .banks_client
            .get_root_slot()
            .await
            .unwrap()
    }

    async fn process_transaction(
        &mut self,
        tx: &VersionedTransaction,
    ) -> Result<BanksTransactionResultWithMetadata> {
        process_transaction(&mut self.program_context.banks_client, tx).await
    }
}

impl TestContext for BankClientTestContext {}

pub async fn new_bankclient_test_context() -> Result<Box<dyn TestContext>> {
    let program_context = create_program_test().start_with_context().await;

    Ok(Box::new(BankClientTestContext { program_context }))
}
