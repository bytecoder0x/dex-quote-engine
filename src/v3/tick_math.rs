//! Tick ↔ sqrt-price conversions (`TickMath`).
//!
//! [`get_sqrt_ratio_at_tick`] is the faithful 20-magic-constant port: `sqrtRatio = sqrt(1.0001^tick)`
//! in Q64.96, with the final Q128→Q96 cast rounded **up** so a tick's price is never understated.
//! [`get_tick_at_sqrt_ratio`] is the canonical `log2` port and returns the **floor** tick — the
//! greatest tick whose sqrt price is `<=` the input. The two are consistent left/right inverses.

use alloy_primitives::{I256, U256, uint};

use crate::error::QuoteError;
use crate::v3::constants::{MAX_SQRT_RATIO, MIN_SQRT_RATIO};
use crate::v3::types::{SqrtPriceX96, Tick};

/// Returns `sqrtRatio(tick) = floor-ish(sqrt(1.0001^tick) * 2^96)` (Q128→Q96 cast rounds **up**).
///
/// Infallible: [`Tick`] is already validated to `[MIN_TICK, MAX_TICK]`, which is the only failure
/// mode the on-chain function guards against (`type-newtype-validated`, `err-result-over-panic` —
/// no `Result` for an operation that cannot fail).
#[must_use]
pub fn get_sqrt_ratio_at_tick(tick: Tick) -> SqrtPriceX96 {
    let abs_tick = tick.get().unsigned_abs();

    // Each bit of |tick| contributes a Q128.128 factor sqrt(1.0001)^(2^i). Seed with bit 0.
    let mut ratio = if abs_tick & 0x1 != 0 {
        uint!(0xfffcb933bd6fad37aa2d162d1a594001_U256)
    } else {
        uint!(0x100000000000000000000000000000000_U256)
    };

    let factors: [(u32, U256); 19] = [
        (0x2, uint!(0xfff97272373d413259a46990580e213a_U256)),
        (0x4, uint!(0xfff2e50f5f656932ef12357cf3c7fdcc_U256)),
        (0x8, uint!(0xffe5caca7e10e4e61c3624eaa0941cd0_U256)),
        (0x10, uint!(0xffcb9843d60f6159c9db58835c926644_U256)),
        (0x20, uint!(0xff973b41fa98c081472e6896dfb254c0_U256)),
        (0x40, uint!(0xff2ea16466c96a3843ec78b326b52861_U256)),
        (0x80, uint!(0xfe5dee046a99a2a811c461f1969c3053_U256)),
        (0x100, uint!(0xfcbe86c7900a88aedcffc83b479aa3a4_U256)),
        (0x200, uint!(0xf987a7253ac413176f2b074cf7815e54_U256)),
        (0x400, uint!(0xf3392b0822b70005940c7a398e4b70f3_U256)),
        (0x800, uint!(0xe7159475a2c29b7443b29c7fa6e889d9_U256)),
        (0x1000, uint!(0xd097f3bdfd2022b8845ad8f792aa5825_U256)),
        (0x2000, uint!(0xa9f746462d870fdf8a65dc1f90e061e5_U256)),
        (0x4000, uint!(0x70d869a156d2a1b890bb3df62baf32f7_U256)),
        (0x8000, uint!(0x31be135f97d08fd981231505542fcfa6_U256)),
        (0x10000, uint!(0x9aa508b5b7a84e1c677de54f3e99bc9_U256)),
        (0x20000, uint!(0x5d6af8dedb81196699c329225ee604_U256)),
        (0x40000, uint!(0x2216e584f5fa1ea926041bedfe98_U256)),
        (0x80000, uint!(0x48a170391f7dc42444e8fa2_U256)),
    ];
    for (bit, factor) in factors {
        if abs_tick & bit != 0 {
            ratio = (ratio * factor) >> 128;
        }
    }

    // The factors compute 1.0001^-|tick|; invert for positive ticks.
    if tick.get() > 0 {
        ratio = U256::MAX / ratio;
    }

    // Q128.128 -> Q64.96, rounding UP: `(ratio + (2^32 - 1)) >> 32` = ceil(ratio / 2^32).
    SqrtPriceX96::new((ratio + uint!(0xffffffff_U256)) >> 32)
}

/// Returns the greatest [`Tick`] whose sqrt price is `<= sqrt_price` (a **floor**).
///
/// Uses the `log2` approximation: compute `log2(price)` to 14 fractional bits, bracket the tick in
/// `[tick_low, tick_high]`, and disambiguate with a single forward call.
///
/// # Errors
/// [`QuoteError::SqrtPriceOutOfRange`] if `sqrt_price` is not in `[MIN_SQRT_RATIO, MAX_SQRT_RATIO)`.
pub fn get_tick_at_sqrt_ratio(sqrt_price: SqrtPriceX96) -> Result<Tick, QuoteError> {
    let price = sqrt_price.get();
    if price < MIN_SQRT_RATIO || price >= MAX_SQRT_RATIO {
        return Err(QuoteError::SqrtPriceOutOfRange);
    }

    let ratio: U256 = price << 32;
    // Most-significant-bit index (floor(log2(ratio))). `ratio` is non-zero here.
    let msb = 255 - ratio.leading_zeros();

    // Normalise `r` to a Q127 mantissa in `[2^127, 2^128)`.
    let mut r: U256 = if msb >= 128 {
        ratio >> (msb - 127)
    } else {
        ratio << (127 - msb)
    };

    // `log_2` is a signed Q192.64; hold the two's-complement bit pattern in a `U256` so the
    // fractional-bit `|=` accumulation matches the Solidity assembly, then reinterpret as `I256`.
    let mut log_2: U256 =
        (I256::from_raw(U256::from(msb)) - I256::from_raw(U256::from(128u32))).into_raw() << 64;
    for i in 0..14u32 {
        r = (r * r) >> 127;
        let f: U256 = r >> 128;
        if !f.is_zero() {
            log_2 |= U256::ONE << (63 - i);
            r >>= 1;
        }
    }
    let log_2 = I256::from_raw(log_2);

    // log_sqrt10001 = log_2 / log2(sqrt(1.0001)), scaled to Q128.128.
    let log_sqrt10001 = log_2 * I256::from_raw(uint!(255738958999603826347141_U256));

    let tick_low = i256_to_tick_i32(
        (log_sqrt10001 - I256::from_raw(uint!(3402992956809132418596140100660247210_U256))) >> 128,
    );
    let tick_high = i256_to_tick_i32(
        (log_sqrt10001 + I256::from_raw(uint!(291339464771989622907027621153398088495_U256)))
            >> 128,
    );

    let tick = if tick_low == tick_high {
        tick_low
    } else if get_sqrt_ratio_at_tick(Tick::new(tick_high)?).get() <= price {
        tick_high
    } else {
        tick_low
    };
    Tick::new(tick)
}

// Faithful port of the Solidity `int24(x)` cast. `tick_low`/`tick_high` are within the tick range
// (|t| < 2^23) by construction, so truncating the two's-complement value to its low 32 bits is
// exact (`num-cast-try-from` deviation: a literal port needs the wrapping cast; range is
// guaranteed, so `TryFrom` would never observe a failure).
#[allow(clippy::cast_possible_truncation)]
fn i256_to_tick_i32(value: I256) -> i32 {
    value.into_raw().into_limbs()[0] as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3::constants::{MAX_TICK, MIN_TICK};

    fn tick(t: i32) -> Tick {
        Tick::new(t).unwrap()
    }

    #[test]
    fn sqrt_ratio_at_tick_zero_is_q96() {
        assert_eq!(get_sqrt_ratio_at_tick(tick(0)).get(), U256::ONE << 96);
    }

    #[test]
    fn sqrt_ratio_at_min_tick_is_min_sqrt_ratio() {
        assert_eq!(get_sqrt_ratio_at_tick(tick(MIN_TICK)).get(), MIN_SQRT_RATIO);
    }

    #[test]
    fn sqrt_ratio_at_max_tick_is_max_sqrt_ratio() {
        assert_eq!(get_sqrt_ratio_at_tick(tick(MAX_TICK)).get(), MAX_SQRT_RATIO);
    }

    #[test]
    fn sqrt_ratio_is_monotonic_across_zero() {
        assert!(get_sqrt_ratio_at_tick(tick(-1)).get() < get_sqrt_ratio_at_tick(tick(0)).get());
        assert!(get_sqrt_ratio_at_tick(tick(0)).get() < get_sqrt_ratio_at_tick(tick(1)).get());
    }

    #[test]
    fn tick_at_q96_is_zero() {
        let q96 = SqrtPriceX96::new(U256::ONE << 96);
        assert_eq!(get_tick_at_sqrt_ratio(q96).unwrap(), tick(0));
    }

    #[test]
    fn tick_at_min_sqrt_ratio_is_min_tick() {
        let p = SqrtPriceX96::new(MIN_SQRT_RATIO);
        assert_eq!(get_tick_at_sqrt_ratio(p).unwrap(), tick(MIN_TICK));
    }

    #[test]
    fn sqrt_price_out_of_range_is_rejected() {
        let too_low = SqrtPriceX96::new(MIN_SQRT_RATIO - U256::ONE);
        assert_eq!(
            get_tick_at_sqrt_ratio(too_low),
            Err(QuoteError::SqrtPriceOutOfRange)
        );
        let too_high = SqrtPriceX96::new(MAX_SQRT_RATIO);
        assert_eq!(
            get_tick_at_sqrt_ratio(too_high),
            Err(QuoteError::SqrtPriceOutOfRange)
        );
    }

    proptest::proptest! {
        #[test]
        fn tick_round_trips_through_sqrt_ratio(t in MIN_TICK..MAX_TICK) {
            // get_sqrt_ratio_at_tick rounds up; the floor inverse recovers the same tick.
            let recovered = get_tick_at_sqrt_ratio(get_sqrt_ratio_at_tick(tick(t))).unwrap();
            proptest::prop_assert_eq!(recovered.get(), t);
        }

        #[test]
        fn tick_at_sqrt_ratio_is_a_floor(t in (MIN_TICK + 1)..MAX_TICK) {
            let p = get_sqrt_ratio_at_tick(tick(t)).get();
            let below = SqrtPriceX96::new(p - U256::ONE);
            let recovered = get_tick_at_sqrt_ratio(below).unwrap();
            proptest::prop_assert!(recovered.get() < t);
        }
    }
}
