use std::error::Error;

use alloy::eips::BlockId;
use alloy::primitives::aliases::I24;
use alloy::primitives::{Address as AlloyAddress, U256 as AU256};
use alloy::providers::Provider;

use dex_quote_engine::{
    Address,
    v3::{Liquidity, PoolStateV3Params, SqrtPriceX96, Tick, TickData, V3Pool},
};

use super::config::{MAX_TICKS_PER_SIDE, WINDOW_WORDS};
use super::contracts::IUniswapV3Pool;
use super::convert::{to_i32, to_i128, to_u256};

pub struct LoadedPool {
    pub pool: V3Pool,
    pub token0: AlloyAddress,
    pub token1: AlloyAddress,
}

type NetTicks = Vec<(i32, i128)>;

pub async fn load_pool<P: Provider>(
    provider: &P,
    pool_addr: AlloyAddress,
    block: BlockId,
) -> Result<LoadedPool, Box<dyn Error>> {
    let pool = IUniswapV3Pool::new(pool_addr, provider);

    let slot0 = pool.slot0().block(block).call().await?;
    let sqrt_price = to_u256(slot0.sqrtPriceX96);
    let current_tick = to_i32(slot0.tick);
    let liquidity = pool.liquidity().block(block).call().await?;
    let tick_spacing = to_i32(pool.tickSpacing().block(block).call().await?);
    let fee: u32 = pool.fee().block(block).call().await?.to::<u32>();
    let token0 = pool.token0().block(block).call().await?;
    let token1 = pool.token1().block(block).call().await?;

    let (below, above) = load_ticks(provider, pool_addr, block, current_tick, tick_spacing).await?;

    let v3 = V3Pool::new(PoolStateV3Params {
        token0: Address::from_slice(token0.as_slice()),
        token1: Address::from_slice(token1.as_slice()),
        tick: Tick::new(current_tick)?,
        tick_liquidity_net: 0, // current tick is the array centre; never re-crossed
        tick_spacing,
        fee,
        sqrt_price_x96: SqrtPriceX96::new(sqrt_price),
        liquidity: Liquidity::new(liquidity),
        zero_for_one_ticks: to_tick_data(&below)?,
        one_for_zero_ticks: to_tick_data(&above)?,
    })?;

    Ok(LoadedPool {
        pool: v3,
        token0,
        token1,
    })
}

fn to_tick_data(ticks: &[(i32, i128)]) -> Result<Vec<TickData>, Box<dyn Error>> {
    ticks
        .iter()
        .map(|&(t, net)| Ok(TickData::new(t, true, net)?))
        .collect()
}

/// Scans the tick bitmap around the current word, then fetches `liquidityNet` for the nearest
/// [`MAX_TICKS_PER_SIDE`] on each side. Returns `(below, above)` in crossing order.
async fn load_ticks<P: Provider>(
    provider: &P,
    pool_addr: AlloyAddress,
    block: BlockId,
    current_tick: i32,
    tick_spacing: i32,
) -> Result<(NetTicks, NetTicks), Box<dyn Error>> {
    let pool = IUniswapV3Pool::new(pool_addr, provider);
    let current_word = current_tick.div_euclid(tick_spacing).div_euclid(256);
    let mut below_idx: Vec<i32> = Vec::new();
    let mut above_idx: Vec<i32> = Vec::new();

    for word in (current_word - WINDOW_WORDS)..=(current_word + WINDOW_WORDS) {
        let bitmap = pool
            .tickBitmap(i16::try_from(word)?)
            .block(block)
            .call()
            .await?;
        if bitmap == AU256::ZERO {
            continue;
        }
        for bit in 0..256usize {
            if !bitmap.bit(bit) {
                continue;
            }
            let tick = (word * 256 + i32::try_from(bit)?) * tick_spacing;
            if tick < current_tick {
                below_idx.push(tick);
            } else if tick > current_tick {
                above_idx.push(tick);
            }
        }
    }

    below_idx.sort_unstable_by_key(|&t| core::cmp::Reverse(t));
    above_idx.sort_unstable_by_key(|&t| t);
    below_idx.truncate(MAX_TICKS_PER_SIDE);
    above_idx.truncate(MAX_TICKS_PER_SIDE);

    let below = fetch_nets(provider, pool_addr, block, &below_idx).await?;
    let above = fetch_nets(provider, pool_addr, block, &above_idx).await?;
    Ok((below, above))
}

async fn fetch_nets<P: Provider>(
    provider: &P,
    pool_addr: AlloyAddress,
    block: BlockId,
    ticks: &[i32],
) -> Result<NetTicks, Box<dyn Error>> {
    let pool = IUniswapV3Pool::new(pool_addr, provider);
    let mut out = Vec::with_capacity(ticks.len());
    for &tick in ticks {
        let net = to_i128(
            pool.ticks(I24::try_from(tick)?)
                .block(block)
                .call()
                .await?
                .liquidityNet,
        );
        out.push((tick, net));
    }
    Ok(out)
}
