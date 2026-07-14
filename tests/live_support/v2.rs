use std::error::Error;

use alloy::eips::BlockId;
use alloy::primitives::{Address as AlloyAddress, U256 as AU256};
use alloy::providers::Provider;

use dex_quote_engine::{Address, U256, v2::V2Pool};

use super::contracts::{IUniswapV2Pair, IUniswapV2Router02};
use super::convert::to_u256;

pub struct LoadedV2Pool {
    pub pool: V2Pool,
    pub token0: AlloyAddress,
    pub token1: AlloyAddress,
}

pub async fn load_v2_pool<P: Provider>(
    provider: &P,
    pair: AlloyAddress,
    fee_bps: u32,
    block: BlockId,
) -> Result<LoadedV2Pool, Box<dyn Error>> {
    let contract = IUniswapV2Pair::new(pair, provider);
    let reserves = contract.getReserves().block(block).call().await?;
    let token0 = contract.token0().block(block).call().await?;
    let token1 = contract.token1().block(block).call().await?;

    let pool = V2Pool::new(
        Address::from_slice(token0.as_slice()),
        Address::from_slice(token1.as_slice()),
        to_u256(reserves.reserve0),
        to_u256(reserves.reserve1),
        fee_bps,
    )?;

    Ok(LoadedV2Pool {
        pool,
        token0,
        token1,
    })
}

/// The on-chain reference: `Router02.getAmountsOut(amountIn, [tokenIn, tokenOut])[1]`.
pub async fn quote_v2_router<P: Provider>(
    provider: &P,
    router: AlloyAddress,
    token_in: AlloyAddress,
    token_out: AlloyAddress,
    amount_in: AU256,
    block: BlockId,
) -> Result<U256, Box<dyn Error>> {
    let contract = IUniswapV2Router02::new(router, provider);
    let amounts = contract
        .getAmountsOut(amount_in, vec![token_in, token_out])
        .block(block)
        .call()
        .await?;
    let out = amounts.get(1).ok_or("router returned no output amount")?;
    Ok(to_u256(*out))
}
