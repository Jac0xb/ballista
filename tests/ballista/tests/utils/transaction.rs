use solana_sdk::{
    message::{v0, VersionedMessage},
    signature::Keypair,
    signer::EncodableKeypair,
    transaction::VersionedTransaction,
};

use super::context::TestContext;

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
