use super::types::SwapArgs;
use crate::utils::jupiter::types::QuoteResponse;

pub async fn get_jupiter_quote(
    args: SwapArgs,
) -> Result<QuoteResponse, Box<dyn std::error::Error>> {
    let url =
      format!(
          "https://quote-api.jup.ag/v6/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}&maxAccounts={}",
          args.input_mint, args.output_mint, args.amount, args.slippage_bps, args.max_accounts
      );

    let response = reqwest::get(url).await.unwrap();
    let quote_response: QuoteResponse = response.json().await.unwrap();
    Ok(quote_response)
}
