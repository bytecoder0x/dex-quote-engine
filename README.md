# dex_quote_engine

A small Rust library that computes **Uniswap V2 and V3 swap quotes** with exact integer math.

Given a pool's state, it tells you how many tokens you get out for a given input
(`get_amount_out`) or how many you need to put in for a wanted output (`get_amount_in`) — the same
numbers the real Uniswap contracts would give.

## Why

- **Exact.** All math uses 256-bit integers (`U256`), never floats, and copies Uniswap's rounding
  rules. Results match the chain to the last wei.
- **Fast and offline.** It's a pure calculation. You pass in the pool state; there are no network
  calls in the core library.
- **Two versions.** Uniswap V2 (constant product `x * y = k`) and V3 (concentrated liquidity with
  ticks).
- **Safe.** No `unsafe` code. Every operation that can fail returns a `Result` instead of panicking.

## How it works (short)

- **V2**: a pool is just two reserves and a fee. The output comes from the `x * y = k` formula.
- **V3**: a pool has a current price, active liquidity, and a list of "ticks" (price boundaries).
  A swap walks through the ticks one range at a time until the input is used up.

You are responsible for reading the pool state from the chain (reserves for V2; price, liquidity,
and nearby ticks for V3) and passing it in. See the tests for a full working example.

## Usage

### V2

```rust
use dex_quote_engine::{Address, U256, v2::V2Pool};

// fee is in basis points: 30 = 0.30%
let pool = V2Pool::new(
    token0, token1,
    reserve0, reserve1,
    30,
)?;

// true = swap token0 in for token1 out
let amount_out = pool.get_amount_out(true, U256::from(1_000_000u64))?;
```

### V3

```rust
use dex_quote_engine::{
    Address, U256,
    v3::{Liquidity, PoolStateV3Params, SqrtPriceX96, Tick, TickData, V3Pool},
};

// fee is in pips: 3000 = 0.30%
let pool = V3Pool::new(PoolStateV3Params {
    token0,
    token1,
    tick: Tick::new(current_tick)?,
    tick_liquidity_net: 0,
    tick_spacing: 60,
    fee: 3000,
    sqrt_price_x96: SqrtPriceX96::new(current_sqrt_price),
    liquidity: Liquidity::new(current_liquidity),
    zero_for_one_ticks,   // ticks below the price, nearest first
    one_for_zero_ticks,   // ticks above the price, nearest first
})?;

// returns (amount_out, amount_left_over)
let (amount_out, remaining) = pool.get_amount_out(true, U256::from(1_000_000u64))?;
```

`remaining` is non-zero only if the swap is bigger than the ticks you loaded.

## Testing

```bash
# unit and doc tests (offline, no network)
cargo test

# check style and lints
cargo clippy --all-targets
cargo fmt --check
```

There is also an optional test that compares the engine against the **real Uniswap contracts on
Ethereum** (V3 QuoterV2 and the V2 Router) for USDC/WETH pools. It needs an RPC endpoint:

```bash
ETH_RPC_URL=<your_rpc_url> cargo test --features live --test live_quoter -- --ignored --nocapture
```

You can put the URL in a `.env` file instead (see `.env.example`).

## Requirements

- Rust 2024 edition (stable, 1.96 or newer).
