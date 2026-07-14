//! Live parity: the engine vs Uniswap's on-chain references.
//!
//! Fetches real pool state at a pinned block (all reads + the quote share one block, or cross-block
//! price drift shows a false mismatch): V3 against `QuoterV2.quoteExactInputSingle`, V2 against
//! `Router02.getAmountsOut`. Supporting code lives in `live_support/`.
//!
//! ```text
//! ETH_RPC_URL=<rpc> cargo test --features live --test live_quoter -- --ignored --nocapture
//! ```
#![cfg(feature = "live")]

mod live_support;

use std::error::Error;

use alloy::eips::BlockId;
use alloy::primitives::U256 as AU256;
use alloy::providers::{Provider, ProviderBuilder};

use dex_quote_engine::U256;

use live_support::config::{
    QUOTER_V2, USDC, USDC_WETH_005, USDC_WETH_030, USDC_WETH_V2, V2_FEE_BPS, V2_ROUTER, WETH,
    env_addr,
};
use live_support::convert::to_u256;
use live_support::pool::load_pool;
use live_support::quoter::quote_exact_input_single;
use live_support::v2::{load_v2_pool, quote_v2_router};

#[tokio::test]
#[ignore = "requires ETH_RPC_URL; run with --features live -- --ignored"]
async fn v3_matches_official_quoter_v2() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    let rpc =
        std::env::var("ETH_RPC_URL").expect("set ETH_RPC_URL to an Ethereum mainnet endpoint");
    let provider = ProviderBuilder::new().connect_http(rpc.parse()?);

    let quoter = env_addr("QUOTER_V2_ADDRESS", QUOTER_V2);
    let pool_005 = env_addr("POOL_USDC_WETH_005", USDC_WETH_005);
    let pool_030 = env_addr("POOL_USDC_WETH_030", USDC_WETH_030);

    let block = BlockId::from(provider.get_block_number().await?);
    println!("pinned block: {block:?}");

    let cases = [
        (pool_005, WETH, AU256::from(10_000_000_000_000_000u64)),
        (pool_005, WETH, AU256::from(1_000_000_000_000_000_000u128)),
        (pool_005, USDC, AU256::from(3_000_000_000u64)),
        (pool_030, WETH, AU256::from(1_000_000_000_000_000_000u128)),
        (pool_030, USDC, AU256::from(3_000_000_000u64)),
    ];

    let mut failures = 0u32;
    for (pool_addr, token_in, amount_in) in cases {
        let loaded = load_pool(&provider, pool_addr, block).await?;
        let zero_for_one = token_in == loaded.token0;
        let token_out = if zero_for_one {
            loaded.token1
        } else {
            loaded.token0
        };
        let fee = loaded.pool.fee();

        let (mine, remaining) = loaded
            .pool
            .get_amount_out(zero_for_one, to_u256(amount_in))?;
        let onchain = quote_exact_input_single(
            &provider, quoter, token_in, token_out, amount_in, fee, block,
        )
        .await?;

        let diff = mine.abs_diff(onchain);
        let ppm = if onchain.is_zero() {
            U256::ZERO
        } else {
            diff * U256::from(1_000_000u32) / onchain
        };
        println!(
            "pool {pool_addr} zeroForOne={zero_for_one} in={amount_in}\n  engine = {mine}\n  quoter = {onchain}\n  diff   = {diff} ({ppm} ppm)  remaining = {remaining}"
        );

        // A few wei of drift is tolerated across bitmap-word boundaries (array model steps
        // tick-to-tick, the chain steps per-word); within a word it is exact.
        let ok = remaining.is_zero() && (diff <= U256::from(3u8) || ppm <= U256::from(2u8));
        failures += u32::from(!ok);
    }

    assert_eq!(failures, 0, "{failures} quote(s) diverged from QuoterV2");
    Ok(())
}

#[tokio::test]
#[ignore = "requires ETH_RPC_URL; run with --features live -- --ignored"]
async fn v2_matches_uniswap_v2_router() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    let rpc =
        std::env::var("ETH_RPC_URL").expect("set ETH_RPC_URL to an Ethereum mainnet endpoint");
    let provider = ProviderBuilder::new().connect_http(rpc.parse()?);

    let router = env_addr("V2_ROUTER_ADDRESS", V2_ROUTER);
    let pair = env_addr("PAIR_USDC_WETH_V2", USDC_WETH_V2);

    let block = BlockId::from(provider.get_block_number().await?);
    println!("pinned block: {block:?}");

    // Amounts divisible by 1000: the reference's pre-divided fee then loses nothing, so the engine
    // is bit-exact with the router's canonical formula.
    let cases = [
        (WETH, AU256::from(1_000_000_000_000_000_000u128)),
        (USDC, AU256::from(3_000_000_000u64)),
    ];

    let mut failures = 0u32;
    for (token_in, amount_in) in cases {
        let loaded = load_v2_pool(&provider, pair, V2_FEE_BPS, block).await?;
        let zero_for_one = token_in == loaded.token0;
        let token_out = if zero_for_one {
            loaded.token1
        } else {
            loaded.token0
        };

        let mine = loaded
            .pool
            .get_amount_out(zero_for_one, to_u256(amount_in))?;
        let onchain =
            quote_v2_router(&provider, router, token_in, token_out, amount_in, block).await?;

        let diff = mine.abs_diff(onchain);
        println!(
            "pair {pair} zeroForOne={zero_for_one} in={amount_in}\n  engine = {mine}\n  router = {onchain}\n  diff   = {diff}"
        );
        failures += u32::from(diff > U256::ZERO);
    }

    assert_eq!(
        failures, 0,
        "{failures} V2 quote(s) diverged from the router"
    );
    Ok(())
}
