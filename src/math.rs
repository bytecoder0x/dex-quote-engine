//! Full-precision fixed-point helpers shared by V2 and V3 math.
//!
//! `mul_div` / `mul_div_rounding_up` evaluate `a * b / denom` through a 512-bit intermediate,
//! so the product never overflows before the division. Rounding is explicit: floor vs ceil.

use alloy_primitives::{U256, U512};

use crate::error::QuoteError;

fn widen(value: U256) -> U512 {
    let [l0, l1, l2, l3] = value.into_limbs();
    U512::from_limbs([l0, l1, l2, l3, 0, 0, 0, 0])
}

fn narrow(value: U512) -> Result<U256, QuoteError> {
    let [l0, l1, l2, l3, l4, l5, l6, l7] = value.into_limbs();
    if (l4 | l5 | l6 | l7) != 0 {
        return Err(QuoteError::Overflow);
    }
    Ok(U256::from_limbs([l0, l1, l2, l3]))
}

/// Returns `floor(a * b / denom)`.
///
/// # Errors
/// [`QuoteError::DivisionByZero`] if `denom` is zero; [`QuoteError::Overflow`] if the exact
/// quotient does not fit in 256 bits.
///
/// # Examples
/// ```
/// use dex_quote_engine::{U256, math::mul_div};
/// assert_eq!(mul_div(U256::from(6u8), U256::from(7u8), U256::from(4u8)).unwrap(), U256::from(10u8));
/// ```
pub fn mul_div(a: U256, b: U256, denom: U256) -> Result<U256, QuoteError> {
    if denom.is_zero() {
        return Err(QuoteError::DivisionByZero);
    }
    narrow(widen(a) * widen(b) / widen(denom))
}

/// Returns `ceil(a * b / denom)`.
///
/// # Errors
/// [`QuoteError::DivisionByZero`] if `denom` is zero; [`QuoteError::Overflow`] if the rounded
/// quotient does not fit in 256 bits.
///
/// # Examples
/// ```
/// use dex_quote_engine::{U256, math::mul_div_rounding_up};
/// assert_eq!(mul_div_rounding_up(U256::from(6u8), U256::from(7u8), U256::from(4u8)).unwrap(), U256::from(11u8));
/// ```
pub fn mul_div_rounding_up(a: U256, b: U256, denom: U256) -> Result<U256, QuoteError> {
    if denom.is_zero() {
        return Err(QuoteError::DivisionByZero);
    }
    let prod = widen(a) * widen(b);
    let denom_wide = widen(denom);
    let result = narrow(prod / denom_wide)?;
    if (prod % denom_wide).is_zero() {
        Ok(result)
    } else {
        result.checked_add(U256::ONE).ok_or(QuoteError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_uses_the_full_512_bit_product() {
        assert_eq!(mul_div(U256::MAX, U256::MAX, U256::MAX).unwrap(), U256::MAX);
    }

    #[test]
    fn mul_div_by_zero_is_rejected() {
        assert_eq!(
            mul_div(U256::from(1u8), U256::from(1u8), U256::ZERO),
            Err(QuoteError::DivisionByZero)
        );
    }

    #[test]
    fn mul_div_overflowing_quotient_is_rejected() {
        assert_eq!(
            mul_div(U256::MAX, U256::MAX, U256::ONE),
            Err(QuoteError::Overflow)
        );
    }

    #[test]
    fn rounding_up_does_not_bump_an_exact_division() {
        assert_eq!(
            mul_div_rounding_up(U256::from(6u8), U256::from(8u8), U256::from(4u8)).unwrap(),
            U256::from(12u8)
        );
    }

    #[test]
    fn rounding_up_bumps_an_inexact_division() {
        assert_eq!(
            mul_div_rounding_up(U256::from(7u8), U256::from(3u8), U256::from(4u8)).unwrap(),
            U256::from(6u8)
        );
    }
}
