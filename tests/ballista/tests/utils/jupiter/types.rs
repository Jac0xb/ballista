use serde::{Deserialize, Serialize};

pub struct SwapArgs {
    pub amount: u64,
    pub input_mint: &'static str,
    pub output_mint: &'static str,
    pub max_accounts: u64,
    pub slippage_bps: u32,
}

#[derive(serde::Deserialize, Debug, Serialize)]
pub struct QuoteResponse {
    pub inputMint: String,
    pub inAmount: String,
    pub outputMint: String,
    pub outAmount: String,
    pub otherAmountThreshold: String,
    pub swapMode: String,
    pub slippageBps: u32,
    pub platformFee: Option<String>,
    pub priceImpactPct: String,
    pub routePlan: Vec<RoutePlan>,
    pub contextSlot: u64,
    pub timeTaken: f64,
}

#[derive(serde::Deserialize, Debug, Serialize)]
pub struct RoutePlan {
    pub swapInfo: SwapInfo,
    pub percent: u32,
}

#[derive(serde::Deserialize, Debug, Serialize)]
pub struct SwapInfo {
    pub ammKey: String,
    pub label: String,
    pub inputMint: String,
    pub outputMint: String,
    pub inAmount: String,
    pub outAmount: String,
    pub feeAmount: String,
    pub feeMint: String,
}

#[derive(Serialize)]
pub struct SwapRequest {
    pub quoteResponse: QuoteResponse,
    pub userPublicKey: String,
    pub wrapAndUnwrapSol: bool,
    // Optionally include feeAccount if needed
    // feeAccount: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SwapResponse {
    pub swapTransaction: String,
    pub lastValidBlockHeight: u64,
    pub prioritizationFeeLamports: u64,
    pub computeUnitLimit: u64,
    pub prioritizationType: PrioritizationType,
    pub dynamicSlippageReport: Option<String>, // Assuming this can be null
    pub simulationError: Option<String>,       // Assuming this can be null
}

#[derive(Deserialize, Debug)]
pub struct PrioritizationType {
    pub computeBudget: ComputeBudget,
}

#[derive(Deserialize, Debug)]
pub struct ComputeBudget {
    pub microLamports: u64,
    pub estimatedMicroLamports: u64,
}

// use serde::{Deserialize, Serialize};

// pub struct SwapArgs {
//     pub amount: u64,
//     pub input_mint: &'static str,
//     pub output_mint: &'static str,
// }

// #[derive(Deserialize, Debug, Serialize)]
// pub struct QuoteResponse {
//     pub inputMint: String,
//     pub inAmount: String,
//     pub outputMint: String,
//     pub outAmount: String,
//     pub otherAmountThreshold: String,
//     pub swapMode: String,
//     pub slippageBps: u32,
//     pub platformFee: Option<String>,
//     pub priceImpactPct: String,
//     pub routePlan: Vec<RoutePlan>,
//     pub contextSlot: u64,
//     pub timeTaken: f64,
// }

// #[derive(Deserialize, Debug, Serialize)]
// pub struct RoutePlan {
//     pub swapInfo: SwapInfo,
//     pub percent: u32,
// }

// #[derive(Deserialize, Debug, Serialize)]
// pub struct SwapInfo {
//     pub ammKey: String,
//     pub label: String,
//     pub inputMint: String,
//     pub outputMint: String,
//     pub inAmount: String,
//     pub outAmount: String,
//     pub feeAmount: String,
//     pub feeMint: String,
// }

// #[derive(Serialize)]
// pub struct SwapRequest {
//     pub quoteResponse: QuoteResponse,
//     pub userPublicKey: String,
//     pub wrapAndUnwrapSol: bool,
//     // Optionally include feeAccount if needed
//     // feeAccount: Option<String>,
// }

#[derive(Deserialize, Debug)]
pub struct SwapInstructionsResponse {
    pub addressLookupTableAddresses: Vec<String>,
    pub cleanupInstruction: Option<JupiterInstruction>,
    pub computeBudgetInstructions: Vec<JupiterInstruction>,
    pub computeUnitLimit: u64,
    pub dynamicSlippageReport: Option<String>,
    pub otherInstructions: Vec<JupiterInstruction>,
    pub prioritizationFeeLamports: u64,
    pub prioritizationType: PrioritizationType,
    pub setupInstructions: Vec<JupiterInstruction>,
    pub simulationError: Option<String>,
    pub swapInstruction: JupiterInstruction,
    pub tokenLedgerInstruction: Option<JupiterInstruction>,
}

#[derive(Deserialize, Debug)]
pub struct JupiterInstruction {
    pub programId: String,
    pub accounts: Vec<AccountMeta>,
    pub data: String, // Base64 encoded instruction data
}

#[derive(Deserialize, Debug)]
pub struct AccountMeta {
    pub pubkey: String,
    pub isSigner: bool,
    pub isWritable: bool,
}

// #[derive(Deserialize, Debug)]
// pub struct PrioritizationType {
//     pub computeBudget: ComputeBudget,
// }

// #[derive(Deserialize, Debug)]
// pub struct ComputeBudget {
//     pub microLamports: u64,
//     pub estimatedMicroLamports: u64,
// }
