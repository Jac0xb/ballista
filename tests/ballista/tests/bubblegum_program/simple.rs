use crate::utils::ballista::task_definitions::schema::{
    build_mint_bubblegum_nft_task_execution, mint_bubblegum_nft_task_definition,
};
use crate::utils::jupiter::transaction_cloning::print_transaction_info;
use crate::utils::process_transaction_assert_success;
use crate::utils::record::TestLogger;
use crate::utils::test_context::TestContext;
use crate::utils::transaction::utils::create_transaction;
use ballista_sdk::find_task_definition_pda;
use ballista_sdk::generated::instructions::{CreateTask, CreateTaskInstructionArgs};
use mpl_bubblegum::state::metaplex_adapter::{MetadataArgs, TokenProgramVersion};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::entrypoint::HEAP_LENGTH;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::EncodableKeypair;
use solana_sdk::{compute_budget, system_program};

use crate::utils::setup::bubblegum::context::BubblegumTestContext;
use crate::utils::setup::bubblegum::{LeafArgs, Tree};

const MAX_DEPTH: usize = 14;
const MAX_BUF_SIZE: usize = 64;
const DEFAULT_NUM_MINTS: u64 = 10;

pub async fn context_tree_and_leaves(
    program_context: &mut ProgramTestContext,
) -> Result<
    (
        BubblegumTestContext,
        Tree<MAX_DEPTH, MAX_BUF_SIZE>,
        Vec<LeafArgs>,
    ),
    String,
> {
    let context = BubblegumTestContext::new(program_context).await.unwrap();

    let (tree, leaves) = context
        .default_create_and_mint::<MAX_DEPTH, MAX_BUF_SIZE>(DEFAULT_NUM_MINTS)
        .await
        .unwrap();

    Ok((context, tree, leaves))
}

///
/// Tests all data types using the `AccountData` assertion.
///
#[tokio::test]
async fn simple() {
    let mut logger = TestLogger::new("bubblgum_program", "simple").unwrap();
    let context = &mut TestContext::new().await.unwrap();

    let (_bubblegum_context, mut tree, mut leaves) =
        context_tree_and_leaves(&mut context.program_context)
            .await
            .unwrap();

    let tree_creator = tree.tree_creator.insecure_clone();

    let mint_tx = tree
        .mint_v1_tx(
            &tree_creator,
            &mut LeafArgs::new(
                &tree_creator,
                MetadataArgs {
                    name: "".to_string(),
                    symbol: "".to_string(),
                    uri: "".to_string(),
                    seller_fee_basis_points: 10_000,
                    primary_sale_happened: false,
                    is_mutable: true,
                    edition_nonce: None,
                    token_standard: Some(
                        mpl_bubblegum::state::metaplex_adapter::TokenStandard::NonFungible,
                    ),
                    collection: None,
                    uses: None,
                    token_program_version: TokenProgramVersion::Original,
                    creators: vec![],
                },
            ),
        )
        .to_transaction(&vec![])
        .await
        .unwrap();

    let task_definition: Pubkey = find_task_definition_pda(tree_creator.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateTask::instruction(
                &CreateTask {
                    system_program: system_program::ID,
                    payer: tree_creator.encodable_pubkey(),
                    task_definition,
                },
                CreateTaskInstructionArgs {
                    task: mint_bubblegum_nft_task_definition(15),
                    task_id: 0,
                },
            ),
        ],
        &[&tree_creator],
    )
    .await;

    process_transaction_assert_success(context, tx.clone(), &mut logger)
        .await
        .unwrap();

    let tx = create_transaction(
        context,
        vec![
            compute_budget::ComputeBudgetInstruction::request_heap_frame(HEAP_LENGTH as u32 * 8),
            build_mint_bubblegum_nft_task_execution(
                &task_definition,
                &tree.authority(),
                &tree_creator.encodable_pubkey(),
                &tree_creator.encodable_pubkey(),
                &tree.tree_pubkey(),
                &tree_creator.encodable_pubkey(),
            ),
        ],
        &[&tree_creator],
    )
    .await;

    print_transaction_info(context, &mint_tx).await.unwrap();
    print_transaction_info(context, &tx).await.unwrap();

    process_transaction_assert_success(context, tx.clone(), &mut logger)
        .await
        .unwrap();
}
