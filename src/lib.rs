//! DEX quote engine: exact-integer Uniswap V2/V3 swap-quote math.
//!
//! All arithmetic is integer / fixed-point (`U256` / `U512`) and mirrors on-chain rounding
//! exactly. Design notes: `docs/architecture.md`, `docs/references.md`.

pub mod error;
pub mod math;
pub mod v2;
pub mod v3;

pub use alloy_primitives::{Address, U256};
pub use error::QuoteError;
