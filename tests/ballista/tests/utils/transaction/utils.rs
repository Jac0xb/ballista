use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    message::{v0, VersionedMessage},
    signature::Keypair,
    signer::EncodableKeypair,
    transaction::VersionedTransaction,
};

use crate::utils::test_context::TestContext;

pub async fn create_transaction(
    context: &mut TestContext,
    instructions: Vec<solana_sdk::instruction::Instruction>,
    keypairs: &[&Keypair],
) -> VersionedTransaction {
    VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &keypairs[0].encodable_pubkey(),
                &instructions,
                &[],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        keypairs,
    )
    .unwrap()
}

pub async fn create_transaction_with_lookup_table(
    context: &mut TestContext,
    instructions: Vec<solana_sdk::instruction::Instruction>,
    lookup_table: AddressLookupTableAccount,
    keypairs: &[&Keypair],
) -> VersionedTransaction {
    VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(
                &keypairs[0].encodable_pubkey(),
                &instructions,
                &[lookup_table],
                context.get_blockhash().await,
            )
            .unwrap(),
        ),
        keypairs,
    )
    .unwrap()
}
