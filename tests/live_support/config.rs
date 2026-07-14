use alloy::primitives::{Address, address};

pub const QUOTER_V2: Address = address!("61fFE014bA17989E743c5F6cB21bF9697530B21e");
pub const USDC_WETH_005: Address = address!("88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640");
pub const USDC_WETH_030: Address = address!("8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8");
pub const WETH: Address = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
pub const USDC: Address = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

pub const V2_ROUTER: Address = address!("7a250d5630B4cF539739dF2C5dAcb4c659F2488D");
pub const USDC_WETH_V2: Address = address!("B4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc");

/// Standard Uniswap V2 fee (0.30%) in basis points.
pub const V2_FEE_BPS: u32 = 30;

pub const WINDOW_WORDS: i32 = 4;

/// Bounds RPC calls; a swap this size crosses only a few ticks. Overflow leaves `amount_remaining`.
pub const MAX_TICKS_PER_SIDE: usize = 64;

pub fn env_addr(key: &str, default: Address) -> Address {
    std::env::var(key)
        .ok()
        .map_or(default, |s| s.parse().expect("valid 0x address in env"))
}
