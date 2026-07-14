//! V3 numeric constants (tick bounds, sqrt-ratio bounds, fixed-point scale, fee denominator).

use alloy_primitives::{U256, uint};

/// Minimum usable tick. `1.0001^MIN_TICK` is the lowest representable price.
pub const MIN_TICK: i32 = -887_272;

/// Maximum usable tick (`-MIN_TICK`).
pub const MAX_TICK: i32 = 887_272;

/// `get_sqrt_ratio_at_tick(MIN_TICK)` — the lowest valid sqrt price (Q64.96).
pub const MIN_SQRT_RATIO: U256 = uint!(4295128739_U256);

/// `get_sqrt_ratio_at_tick(MAX_TICK)` — the exclusive upper bound for a valid sqrt price (Q64.96).
pub const MAX_SQRT_RATIO: U256 = uint!(1461446703485210103287273052203988822378723970342_U256);

/// The Q64.96 fixed-point scale, `2^96`.
pub const Q96: U256 = U256::from_limbs([0, 1 << 32, 0, 0]);

/// Fee denominator for V3. Fees are in pips (hundredths of a bip): `3000` = 0.30%.
pub const PIPS_DENOMINATOR: u32 = 1_000_000;
