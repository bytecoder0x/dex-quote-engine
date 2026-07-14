//! Amount deltas and price transitions within a single liquidity range (`SqrtPriceMath`).
//!
//! Rounding follows the fund-safety invariant: **`amount_in` rounds UP, `amount_out` rounds DOWN**
//! (via the `round_up` flag on the delta helpers), and price transitions round so the pool never
//! releases more than it must. See the rounding table in `docs/references.md`.

use alloy_primitives::U256;

use crate::error::QuoteError;
use crate::math::{mul_div, mul_div_rounding_up};
use crate::v3::constants::Q96;
use crate::v3::types::{Liquidity, SqrtPriceX96};

fn sorted(a: U256, b: U256) -> (U256, U256) {
    if a <= b { (a, b) } else { (b, a) }
}

fn require_price_and_liquidity(
    sqrt_p: SqrtPriceX96,
    liquidity: Liquidity,
) -> Result<(), QuoteError> {
    if sqrt_p.get().is_zero() {
        return Err(QuoteError::SqrtPriceOutOfRange);
    }
    if liquidity.is_zero() {
        return Err(QuoteError::InsufficientLiquidity);
    }
    Ok(())
}

/// `amount0 = L * (1/sqrt_a - 1/sqrt_b)`, exact integer form.
///
/// With `round_up` the result is ceiled twice (used for an input amount); otherwise floored twice
/// (used for an output amount) — mirroring `SqrtPriceMath.getAmount0Delta`.
///
/// # Errors
/// [`QuoteError::SqrtPriceOutOfRange`] if the lower sqrt price is zero; propagates
/// [`QuoteError::Overflow`].
pub fn get_amount0_delta(
    sqrt_a: SqrtPriceX96,
    sqrt_b: SqrtPriceX96,
    liquidity: Liquidity,
    round_up: bool,
) -> Result<U256, QuoteError> {
    let (lo, hi) = sorted(sqrt_a.get(), sqrt_b.get());
    if lo.is_zero() {
        return Err(QuoteError::SqrtPriceOutOfRange);
    }
    let numerator1 = U256::from(liquidity.get()) << 96;
    let numerator2 = hi - lo;
    if round_up {
        let inner = mul_div_rounding_up(numerator1, numerator2, hi)?;
        mul_div_rounding_up(inner, U256::ONE, lo)
    } else {
        Ok(mul_div(numerator1, numerator2, hi)? / lo)
    }
}

/// `amount1 = L * (sqrt_b - sqrt_a)`, in Q96.
///
/// # Errors
/// Propagates [`QuoteError::Overflow`] from the intermediate arithmetic.
pub fn get_amount1_delta(
    sqrt_a: SqrtPriceX96,
    sqrt_b: SqrtPriceX96,
    liquidity: Liquidity,
    round_up: bool,
) -> Result<U256, QuoteError> {
    let (lo, hi) = sorted(sqrt_a.get(), sqrt_b.get());
    let l = U256::from(liquidity.get());
    let numerator = hi - lo;
    if round_up {
        mul_div_rounding_up(l, numerator, Q96)
    } else {
        mul_div(l, numerator, Q96)
    }
}

/// Adding/removing token0 moves the price; the resulting price rounds **up**.
fn next_sqrt_price_from_amount0_rounding_up(
    sqrt_p: U256,
    liquidity: u128,
    amount: U256,
    add: bool,
) -> Result<U256, QuoteError> {
    if amount.is_zero() {
        return Ok(sqrt_p);
    }
    let numerator1: U256 = U256::from(liquidity) << 96;
    if add {
        // Prefer the precise form `L·2^96·sqrtP / (L·2^96 + amount·sqrtP)`; fall back to an
        // overflow-proof rearrangement when the product does not fit (`num-overflow-explicit`).
        if let Some(product) = amount.checked_mul(sqrt_p)
            && let Some(denominator) = numerator1.checked_add(product)
            && denominator >= numerator1
        {
            return mul_div_rounding_up(numerator1, sqrt_p, denominator);
        }
        if sqrt_p.is_zero() {
            return Err(QuoteError::SqrtPriceOutOfRange);
        }
        let denominator = (numerator1 / sqrt_p)
            .checked_add(amount)
            .ok_or(QuoteError::Overflow)?;
        mul_div_rounding_up(numerator1, U256::ONE, denominator)
    } else {
        let product = amount.checked_mul(sqrt_p).ok_or(QuoteError::Overflow)?;
        if numerator1 <= product {
            return Err(QuoteError::SqrtPriceOutOfRange);
        }
        let denominator = numerator1 - product;
        mul_div_rounding_up(numerator1, sqrt_p, denominator)
    }
}

/// Adding/removing token1 moves the price; the resulting price rounds **down**.
fn next_sqrt_price_from_amount1_rounding_down(
    sqrt_p: U256,
    liquidity: u128,
    amount: U256,
    add: bool,
) -> Result<U256, QuoteError> {
    if liquidity == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }
    let l = U256::from(liquidity);
    if add {
        let quotient = mul_div(amount, Q96, l)?;
        sqrt_p.checked_add(quotient).ok_or(QuoteError::Overflow)
    } else {
        // Ceil the quotient so that `sqrt_p - quotient` is itself floored.
        let quotient = mul_div_rounding_up(amount, Q96, l)?;
        if sqrt_p <= quotient {
            return Err(QuoteError::SqrtPriceOutOfRange);
        }
        Ok(sqrt_p - quotient)
    }
}

/// The sqrt price after adding `amount_in` of the input token.
///
/// # Errors
/// [`QuoteError::SqrtPriceOutOfRange`] if the price is zero; [`QuoteError::InsufficientLiquidity`]
/// if liquidity is zero; propagates [`QuoteError::Overflow`].
pub fn next_sqrt_price_from_input(
    sqrt_p: SqrtPriceX96,
    liquidity: Liquidity,
    amount_in: U256,
    zero_for_one: bool,
) -> Result<SqrtPriceX96, QuoteError> {
    require_price_and_liquidity(sqrt_p, liquidity)?;
    let next = if zero_for_one {
        next_sqrt_price_from_amount0_rounding_up(sqrt_p.get(), liquidity.get(), amount_in, true)?
    } else {
        next_sqrt_price_from_amount1_rounding_down(sqrt_p.get(), liquidity.get(), amount_in, true)?
    };
    Ok(SqrtPriceX96::new(next))
}

/// The sqrt price after removing `amount_out` of the output token.
///
/// # Errors
/// [`QuoteError::SqrtPriceOutOfRange`] if the price is zero; [`QuoteError::InsufficientLiquidity`]
/// if liquidity is zero; propagates [`QuoteError::Overflow`].
pub fn next_sqrt_price_from_output(
    sqrt_p: SqrtPriceX96,
    liquidity: Liquidity,
    amount_out: U256,
    zero_for_one: bool,
) -> Result<SqrtPriceX96, QuoteError> {
    require_price_and_liquidity(sqrt_p, liquidity)?;
    let next = if zero_for_one {
        next_sqrt_price_from_amount1_rounding_down(
            sqrt_p.get(),
            liquidity.get(),
            amount_out,
            false,
        )?
    } else {
        next_sqrt_price_from_amount0_rounding_up(sqrt_p.get(), liquidity.get(), amount_out, false)?
    };
    Ok(SqrtPriceX96::new(next))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(mult: u64) -> SqrtPriceX96 {
        SqrtPriceX96::new(Q96 * U256::from(mult))
    }

    #[test]
    fn amount1_delta_over_one_q96_step_is_liquidity() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let got = get_amount1_delta(price(1), price(2), l, false).unwrap();
        assert_eq!(got, U256::from(l.get()));
    }

    #[test]
    fn amount0_delta_over_one_q96_step_halves_liquidity() {
        let l = Liquidity::new(2);
        let got = get_amount0_delta(price(1), price(2), l, false).unwrap();
        assert_eq!(got, U256::ONE);
    }

    #[test]
    fn amount0_delta_round_up_is_at_least_round_down() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let down = get_amount0_delta(price(1), price(3), l, false).unwrap();
        let up = get_amount0_delta(price(1), price(3), l, true).unwrap();
        assert!(up >= down);
        assert!(up - down <= U256::ONE);
    }

    #[test]
    fn delta_is_symmetric_in_price_order() {
        let l = Liquidity::new(12_345);
        assert_eq!(
            get_amount0_delta(price(1), price(5), l, true).unwrap(),
            get_amount0_delta(price(5), price(1), l, true).unwrap()
        );
    }

    #[test]
    fn next_price_from_token1_input_moves_price_up() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let next = next_sqrt_price_from_input(price(1), l, U256::from(l.get()), false).unwrap();
        assert_eq!(next, price(2));
    }

    #[test]
    fn next_price_from_token0_input_moves_price_down() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let next =
            next_sqrt_price_from_input(price(2), l, U256::from(1_000_000_000u64), true).unwrap();
        assert!(next.get() < price(2).get());
    }

    #[test]
    fn zero_liquidity_input_is_insufficient() {
        assert_eq!(
            next_sqrt_price_from_input(price(1), Liquidity::new(0), U256::ONE, true),
            Err(QuoteError::InsufficientLiquidity)
        );
    }

    #[test]
    fn next_price_from_token1_output_lowers_price_one_step() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let next = next_sqrt_price_from_output(price(2), l, U256::from(l.get()), true).unwrap();
        assert_eq!(next, price(1));
    }

    #[test]
    fn next_price_from_token0_output_raises_price() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let next =
            next_sqrt_price_from_output(price(1), l, U256::from(1_000_000u64), false).unwrap();
        assert!(next.get() > price(1).get());
    }

    #[test]
    fn output_that_would_drive_price_below_zero_is_rejected() {
        let l = Liquidity::new(1000);
        assert_eq!(
            next_sqrt_price_from_output(price(1), l, U256::from(2000u64), true),
            Err(QuoteError::SqrtPriceOutOfRange)
        );
    }

    proptest::proptest! {
        #[test]
        fn amount_deltas_round_up_bounds_round_down(
            a in 1u64..=8,
            b in 1u64..=8,
            l in 1u128..=u128::from(u64::MAX),
        ) {
            proptest::prop_assume!(a != b);
            let liq = Liquidity::new(l);

            let d0 = get_amount0_delta(price(a), price(b), liq, false).unwrap();
            let u0 = get_amount0_delta(price(a), price(b), liq, true).unwrap();
            proptest::prop_assert!(u0 >= d0 && u0 - d0 <= U256::ONE);

            let d1 = get_amount1_delta(price(a), price(b), liq, false).unwrap();
            let u1 = get_amount1_delta(price(a), price(b), liq, true).unwrap();
            proptest::prop_assert!(u1 >= d1 && u1 - d1 <= U256::ONE);
        }
    }
}
