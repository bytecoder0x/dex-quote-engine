use std::error::Error;

use alloy::eips::BlockId;
use alloy::primitives::aliases::{U24, U160};
use alloy::primitives::{Address as AlloyAddress, U256 as AU256};
use alloy::providers::Provider;

use dex_quote_engine::U256;

use super::contracts::IQuoterV2;
use super::convert::to_u256;

pub async fn quote_exact_input_single<P: Provider>(
    provider: &P,
    quoter_addr: AlloyAddress,
    token_in: AlloyAddress,
    token_out: AlloyAddress,
    amount_in: AU256,
    fee: u32,
    block: BlockId,
) -> Result<U256, Box<dyn Error>> {
    let quoter = IQuoterV2::new(quoter_addr, provider);
    let params = IQuoterV2::QuoteExactInputSingleParams {
        tokenIn: token_in,
        tokenOut: token_out,
        amountIn: amount_in,
        fee: U24::try_from(fee)?,
        sqrtPriceLimitX96: U160::ZERO,
    };
    let out = quoter
        .quoteExactInputSingle(params)
        .block(block)
        .call()
        .await?
        .amountOut;
    Ok(to_u256(out))
}
