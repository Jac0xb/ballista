use crate::utils::context::TestContext;
use crate::utils::schema::{
    build_mint_bubblegum_nft_task_execution, mint_bubblegum_nft_instruction_definition,
    mint_bubblegum_nft_task_definition,
};
use crate::utils::transaction::create_transaction;
use crate::utils::{create_user, process_transaction_assert_success};
use ballista_common::schema::Schema;
use ballista_sdk::generated::instructions::{CreateSchema, CreateSchemaInstructionArgs};
use ballista_sdk::{find_schema_pda, BALLISTA_ID};
use borsh::BorshSerialize;
use log::{debug, trace};
use mpl_bubblegum::state::metaplex_adapter::{MetadataArgs, TokenProgramVersion};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::EncodableKeypair;
use solana_sdk::{compute_budget, system_program};

use crate::utils::bubblegum::context::BubblegumTestContext;
use crate::utils::bubblegum::{LeafArgs, Tree};

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
    debug!("Commencing yak shaving");

    let context = &mut TestContext::new().await.unwrap();

    let (_, mut tree, mut leaves) = context_tree_and_leaves(&mut context.program_context)
        .await
        .unwrap();

    let tree_creator = &tree.tree_creator;

    let user = create_user(context).await.unwrap();
    let schema: Pubkey = find_schema_pda(user.encodable_pubkey(), 0).0;
    let tx = create_transaction(
        context,
        vec![
            ballista_sdk::generated::instructions::CreateSchema::instruction(
                &CreateSchema {
                    program_id: BALLISTA_ID,
                    system_program: system_program::ID,
                    payer: user.encodable_pubkey(),
                    schema,
                },
                CreateSchemaInstructionArgs {
                    schema_arg: Schema {
                        instructions: vec![mint_bubblegum_nft_instruction_definition()],
                        tasks: vec![mint_bubblegum_nft_task_definition()],
                    },
                },
            ),
        ],
        &[&user],
    )
    .await;

    process_transaction_assert_success(context, tx)
        .await
        .unwrap();

    let data = mpl_bubblegum::instruction::MintV1 {
        message: MetadataArgs {
            name: "".to_string(),
            symbol: "".to_string(),
            uri: "".to_string(),
            creators: vec![],
            collection: None,
            uses: None,
            seller_fee_basis_points: u16::MAX,
            primary_sale_happened: false,
            is_mutable: true,
            edition_nonce: None,
            token_standard: Some(
                mpl_bubblegum::state::metaplex_adapter::TokenStandard::NonFungible,
            ),
            token_program_version: TokenProgramVersion::Original,
        },
    };

    println!("borsh name {:?}", data.message.name.try_to_vec().unwrap());
    println!(
        "borsh symbol {:?}",
        data.message.symbol.try_to_vec().unwrap()
    );
    println!("borsh url {:?}", data.message.uri.try_to_vec().unwrap());
    println!(
        "borsh creators {:?}",
        data.message.creators.try_to_vec().unwrap()
    );
    println!(
        "borsh collection {:?}",
        data.message.collection.try_to_vec().unwrap()
    );
    println!("borsh uses {:?}", data.message.uses.try_to_vec().unwrap());
    println!(
        "borsh seller_fee_basis_points {:?}",
        data.message.seller_fee_basis_points.try_to_vec().unwrap()
    );
    println!(
        "borsh primary_sale_happened {:?}",
        data.message.primary_sale_happened.try_to_vec().unwrap()
    );
    println!(
        "borsh token_standard {:?}",
        data.message.token_standard.try_to_vec().unwrap()
    );
    println!(
        "borsh token_program_version {:?}",
        data.message.token_program_version.try_to_vec().unwrap()
    );

    // pub struct MetadataArgs {
    //     /// The name of the asset
    //     pub name: String,
    //     /// The symbol for the asset
    //     pub symbol: String,
    //     /// URI pointing to JSON representing the asset
    //     pub uri: String,
    //     /// Royalty basis points that goes to creators in secondary sales (0-10000)
    //     pub seller_fee_basis_points: u16,
    //     // Immutable, once flipped, all sales of this metadata are considered secondary.
    //     pub primary_sale_happened: bool,
    //     // Whether or not the data struct is mutable, default is not
    //     pub is_mutable: bool,
    //     /// nonce for easy calculation of editions, if present
    //     pub edition_nonce: Option<u8>,
    //     /// Since we cannot easily change Metadata, we add the new DataV2 fields here at the end.
    //     pub token_standard: Option<TokenStandard>,
    //     /// Collection
    //     pub collection: Option<Collection>,
    //     /// Uses
    //     pub uses: Option<Uses>,
    //     pub token_program_version: TokenProgramVersion,
    //     pub creators: Vec<Creator>,
    // }

    // Old [145, 98, 192, 118, 184, 147, 118, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]
    // New [145, 98, 192, 118, 184, 147, 118, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]

    println!("MINT DISCRIMINATOR: {:?}", data.try_to_vec().unwrap());

    let tx = create_transaction(
        context,
        vec![
            compute_budget::ComputeBudgetInstruction::request_heap_frame(128 * 1024),
            build_mint_bubblegum_nft_task_execution(
                &schema,
                &tree.authority(),
                &tree_creator.encodable_pubkey(),
                &tree_creator.encodable_pubkey(),
                &tree.tree_pubkey(),
                &tree_creator.encodable_pubkey(),
            ),
        ],
        &[tree_creator],
    )
    .await;

    process_transaction_assert_success(context, tx)
        .await
        .unwrap();

    panic!("GOEGKF")

    // let leaf = leaves.first_mut().unwrap();

    // let tree_pubkey = tree.tree_pubkey();
    // let tree_root = tree.decode_root().await.unwrap();

    // let proof_path = tree.proof_of_leaf(leaf.index);
    // let mut proof_path_metas: Vec<AccountMeta> = vec![];

    // for proof in proof_path.iter() {
    //     proof_path_metas.push(AccountMeta::new_readonly(
    //         Pubkey::new_from_array(*proof),
    //         false,
    //     ));
    // }
}

// let user_ata =
//     get_associated_token_address(&user.encodable_pubkey(), &mint_keypair.encodable_pubkey());

// let mint_tx = VersionedTransaction::try_new(
//     VersionedMessage::V0(
//         v0::Message::try_compile(
//             &user.encodable_pubkey(),
//             &[
//                 system_instruction::create_account(
//                     &user.encodable_pubkey(),
//                     &mint_keypair.encodable_pubkey(),
//                     context
//                         .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)
//                         .await,
//                     spl_token::state::Mint::LEN as u64,
//                     &spl_token::ID,
//                 ),
//                 spl_token::instruction::initialize_mint2(
//                     &spl_token::ID,
//                     &mint_keypair.encodable_pubkey(),
//                     &user.encodable_pubkey(),
//                     None,
//                     6,
//                 )
//                 .unwrap(),
//                 spl_associated_token_account::instruction::create_associated_token_account(
//                     &user.encodable_pubkey(),
//                     &user.encodable_pubkey(),
//                     &mint_keypair.encodable_pubkey(),
//                     &spl_token::ID,
//                 ),
//                 spl_token::instruction::mint_to(
//                     &spl_token::ID,
//                     &mint_keypair.encodable_pubkey(),
//                     &user_ata,
//                     &user.encodable_pubkey(),
//                     &[],
//                     100_000_000,
//                 )
//                 .unwrap(),
//             ],
//             &[],
//             context.get_blockhash().await,
//         )
//         .unwrap(),
//     ),
//     &[&user, &mint_keypair],
// )
// .unwrap();

// process_transaction_assert_success(context, mint_tx)
//     .await
//     .unwrap();

// let user_ata_info = context
//     .client()
//     .get_account(user_ata)
//     .await
//     .unwrap()
//     .unwrap();

// println!("user ata info: {:?}", user_ata_info);

// let tasks = vec![create_batch_token_transfer_def(1_000_000)];

// let schema: Pubkey = find_schema_pda(user.encodable_pubkey(), 0).0;
// let tx = create_transaction(
//     context,
//     vec![
//         ballista_sdk::generated::instructions::CreateSchema::instruction(
//             &CreateSchema {
//                 program_id: BALLISTA_ID,
//                 system_program: system_program::ID,
//                 payer: user.encodable_pubkey(),
//                 schema,
//             },
//             CreateSchemaInstructionArgs {
//                 schema_arg: Schema {
//                     instructions: vec![],
//                     tasks,
//                 },
//             },
//         ),
//     ],
//     &[&user],
// )
// .await;

// process_transaction_assert_success(context, tx)
//     .await
//     .unwrap();

// let mut dest_accounts_batch_one = vec![];
// for _ in 0..batch_size {
//     dest_accounts_batch_one.push(Keypair::new().encodable_pubkey());
// }

// let tx = create_transaction(
//     context,
//     vec![
//         create_token_transfer(
//             schema,
//             &user,
//             user_ata,
//             &mint_keypair.encodable_pubkey(),
//             &dest_accounts_batch_one,
//         ),
//         // create_token_transfer(
//         //     schema,
//         //     &user,
//         //     user_ata,
//         //     &mint_keypair.encodable_pubkey(),
//         //     &dest_accounts_batch_two,
//         // ),
//         // create_token_transfer(
//         //     schema,
//         //     &user,
//         //     user_ata,
//         //     &mint_keypair.encodable_pubkey(),
//         //     &dest_accounts_batch_three,
//         // ),
//     ],
//     &[&user],
// )
// .await;

// process_transaction_assert_success(context, tx)
//     .await
//     .unwrap();

// for account in dest_accounts_batch_one {
//     let account_info = context
//         .client()
//         .get_account(get_associated_token_address(
//             &account,
//             &mint_keypair.encodable_pubkey(),
//         ))
//         .await
//         .unwrap()
//         .unwrap();

//     let token_account = spl_token::state::Account::unpack(&account_info.data).unwrap();

//     assert_eq!(
//         1_000_000, token_account.amount,
//         "Token account amount was {} expected {}",
//         token_account.amount, 1_000_000
//     );
// }

// panic!("done");
