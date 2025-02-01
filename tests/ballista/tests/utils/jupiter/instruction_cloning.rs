use std::str::FromStr;

use crate::utils::{
    cloning::copy_accounts_from_transaction,
    jupiter::types::{SwapInstructionsResponse, SwapRequest},
    test_context::TestContext,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    instruction::{AccountMeta, Instruction},
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::Keypair,
    transaction::VersionedTransaction,
};

use super::{
    quote::get_jupiter_quote,
    types::{JupiterInstruction, SwapArgs},
};

pub struct SwapInstructions {
    pub setup_instructions: Vec<Instruction>,
    pub swap_instruction: Instruction,
}

pub async fn clone_swap_instructions(
    context: &mut Box<dyn TestContext>,
    user: &Pubkey,
    client: &reqwest::Client,
    rpc_client: &RpcClient,
    args: SwapArgs,
) -> Result<SwapInstructions, Box<dyn std::error::Error>> {
    let quote_response = get_jupiter_quote(args).await?;

    let response = client
        .post("https://quote-api.jup.ag/v6/swap-instructions")
        .header("Content-Type", "application/json")
        .json(&SwapRequest {
            quoteResponse: quote_response,
            userPublicKey: user.to_string(),
            wrapAndUnwrapSol: true,
        })
        .send()
        .await
        .unwrap();

    let response: SwapInstructionsResponse = response.json().await.unwrap();

    println!("{:#?}", response);

    let setup_instructions = response
        .setupInstructions
        .iter()
        .map(|i| jup_instruction_to_solana_instruction(i).unwrap())
        .collect::<Vec<_>>();

    let swap_instruction = jup_instruction_to_solana_instruction(&response.swapInstruction)?;

    Ok(SwapInstructions {
        setup_instructions,
        swap_instruction,
    })
}

fn jup_instruction_to_solana_instruction(
    instruction: &JupiterInstruction,
) -> Result<Instruction, Box<dyn std::error::Error>> {
    let accounts = instruction
        .accounts
        .iter()
        .map(|a| AccountMeta {
            pubkey: Pubkey::from_str(a.pubkey.as_str()).unwrap(),
            is_signer: a.isSigner,
            is_writable: a.isWritable,
        })
        .collect();
    let data = base64::decode(instruction.data.as_str()).unwrap();
    let program_id = Pubkey::from_str(instruction.programId.as_str()).unwrap();

    Ok(Instruction {
        accounts,
        data,
        program_id,
    })
}
