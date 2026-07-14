//! The [`V2Pool`] snapshot and its constant-product quote functions.

use alloy_primitives::{Address, U256};

use crate::error::QuoteError;
use crate::math::mul_div;
use crate::v2::constants::FEE_DENOMINATOR;

/// A constant-product (`x * y = k`) pool snapshot: two reserves and a fee.
///
/// Quotes are pure and non-mutating. Direction is `zero_for_one`: `true` swaps `token0` in for
/// `token1` out (input reserve is `reserve0`); `false` is the reverse.
#[derive(Debug, Clone)]
pub struct V2Pool {
    token0: Address,
    token1: Address,
    reserve0: U256,
    reserve1: U256,
    fee: u32,
}

impl V2Pool {
    /// Creates a pool from a reserve snapshot. `fee` is in basis points (`30` = 0.30%).
    ///
    /// # Errors
    /// [`QuoteError::InvalidFee`] if `fee` is not below [`FEE_DENOMINATOR`].
    pub fn new(
        token0: Address,
        token1: Address,
        reserve0: U256,
        reserve1: U256,
        fee: u32,
    ) -> Result<Self, QuoteError> {
        if fee >= FEE_DENOMINATOR {
            return Err(QuoteError::InvalidFee);
        }
        Ok(Self {
            token0,
            token1,
            reserve0,
            reserve1,
            fee,
        })
    }

    /// The pool's `token0` address.
    #[must_use]
    pub const fn token0(&self) -> Address {
        self.token0
    }

    /// The pool's `token1` address.
    #[must_use]
    pub const fn token1(&self) -> Address {
        self.token1
    }

    /// The current reserves as `(reserve0, reserve1)`.
    #[must_use]
    pub const fn reserves(&self) -> (U256, U256) {
        (self.reserve0, self.reserve1)
    }

    /// The fee in basis points.
    #[must_use]
    pub const fn fee(&self) -> u32 {
        self.fee
    }

    fn in_out(&self, zero_for_one: bool) -> (U256, U256) {
        if zero_for_one {
            (self.reserve0, self.reserve1)
        } else {
            (self.reserve1, self.reserve0)
        }
    }

    /// Returns the output amount for an exact `amount_in` in the given direction.
    ///
    /// # Errors
    /// [`QuoteError::InsufficientLiquidity`] if either reserve is zero; propagates
    /// [`QuoteError::Overflow`] from the intermediate arithmetic.
    ///
    /// # Examples
    /// ```
    /// use dex_quote_engine::{Address, U256, v2::V2Pool};
    /// let pool = V2Pool::new(
    ///     Address::from([1u8; 20]), Address::from([2u8; 20]),
    ///     U256::from(1000u64), U256::from(1000u64), 30,
    /// ).unwrap();
    /// assert_eq!(pool.get_amount_out(true, U256::from(100u64)).unwrap(), U256::from(90u64));
    /// ```
    pub fn get_amount_out(&self, zero_for_one: bool, amount_in: U256) -> Result<U256, QuoteError> {
        let (reserve_in, reserve_out) = self.in_out(zero_for_one);
        amount_out(amount_in, reserve_in, reserve_out, self.fee)
    }

    /// Returns the input amount required for an exact `amount_out` in the given direction.
    ///
    /// # Errors
    /// [`QuoteError::InsufficientLiquidity`] if a reserve is zero or `amount_out` is not below
    /// the output reserve; propagates [`QuoteError::Overflow`].
    ///
    /// # Examples
    /// ```
    /// use dex_quote_engine::{Address, U256, v2::V2Pool};
    /// let pool = V2Pool::new(
    ///     Address::from([1u8; 20]), Address::from([2u8; 20]),
    ///     U256::from(1000u64), U256::from(1000u64), 30,
    /// ).unwrap();
    /// assert_eq!(pool.get_amount_in(true, U256::from(90u64)).unwrap(), U256::from(100u64));
    /// ```
    pub fn get_amount_in(&self, zero_for_one: bool, amount_out: U256) -> Result<U256, QuoteError> {
        let (reserve_in, reserve_out) = self.in_out(zero_for_one);
        amount_in(amount_out, reserve_in, reserve_out, self.fee)
    }
}

fn amount_out(
    amount_in: U256,
    reserve_in: U256,
    reserve_out: U256,
    fee: u32,
) -> Result<U256, QuoteError> {
    if reserve_in.is_zero() || reserve_out.is_zero() {
        return Err(QuoteError::InsufficientLiquidity);
    }
    // Fee is pre-divided (floor), then constant product. This mirrors the reference's exact integer
    // order — it loses precision vs. the canonical form but must match it wei-for-wei.
    let amount_in_with_fee = mul_div(
        amount_in,
        U256::from(FEE_DENOMINATOR - fee),
        U256::from(FEE_DENOMINATOR),
    )?;
    let denominator = reserve_in
        .checked_add(amount_in_with_fee)
        .ok_or(QuoteError::Overflow)?;
    mul_div(amount_in_with_fee, reserve_out, denominator)
}

fn amount_in(
    amount_out: U256,
    reserve_in: U256,
    reserve_out: U256,
    fee: u32,
) -> Result<U256, QuoteError> {
    if reserve_in.is_zero() || reserve_out.is_zero() {
        return Err(QuoteError::InsufficientLiquidity);
    }
    if amount_out.is_zero() {
        return Ok(U256::ZERO);
    }
    if amount_out >= reserve_out {
        return Err(QuoteError::InsufficientLiquidity);
    }
    let numerator_scaled = reserve_in
        .checked_mul(U256::from(FEE_DENOMINATOR))
        .ok_or(QuoteError::Overflow)?;
    let denominator = (reserve_out - amount_out)
        .checked_mul(U256::from(FEE_DENOMINATOR - fee))
        .ok_or(QuoteError::Overflow)?;
    let base = mul_div(numerator_scaled, amount_out, denominator)?;
    base.checked_add(U256::ONE).ok_or(QuoteError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(reserve0: u64, reserve1: u64, fee: u32) -> V2Pool {
        V2Pool::new(
            Address::from([1u8; 20]),
            Address::from([2u8; 20]),
            U256::from(reserve0),
            U256::from(reserve1),
            fee,
        )
        .unwrap()
    }

    #[test]
    fn get_amount_out_matches_canonical_v2() {
        let pool = pool(1000, 1000, 30);
        let out = pool.get_amount_out(true, U256::from(100u64)).unwrap();
        assert_eq!(out, U256::from(90u64));
    }

    #[test]
    fn get_amount_out_pre_divides_the_fee() {
        let pool = pool(100, 100, 30);
        assert_eq!(
            pool.get_amount_out(true, U256::from(3u64)).unwrap(),
            U256::from(1u64)
        );
    }

    #[test]
    fn get_amount_in_is_inverse_of_out() {
        let pool = pool(1000, 1000, 30);
        assert_eq!(
            pool.get_amount_in(true, U256::from(90u64)).unwrap(),
            U256::from(100u64)
        );
    }

    #[test]
    fn direction_flips_reserves() {
        let pool = pool(2000, 1000, 30);
        let z4o = pool.get_amount_out(true, U256::from(100u64)).unwrap();
        let o4z = pool.get_amount_out(false, U256::from(100u64)).unwrap();
        assert_ne!(z4o, o4z);
    }

    #[test]
    fn zero_amount_out_needs_zero_input() {
        let pool = pool(1000, 1000, 30);
        assert_eq!(pool.get_amount_in(true, U256::ZERO).unwrap(), U256::ZERO);
    }

    #[test]
    fn zero_reserves_are_insufficient_liquidity() {
        let pool = pool(0, 1000, 30);
        assert_eq!(
            pool.get_amount_out(true, U256::from(1u64)),
            Err(QuoteError::InsufficientLiquidity)
        );
    }

    #[test]
    fn amount_out_at_or_above_reserve_is_insufficient() {
        let pool = pool(1000, 1000, 30);
        assert_eq!(
            pool.get_amount_in(true, U256::from(1000u64)),
            Err(QuoteError::InsufficientLiquidity)
        );
    }

    #[test]
    fn amount_out_reserve_overflow_is_reported() {
        let pool = V2Pool::new(
            Address::ZERO,
            Address::ZERO,
            U256::MAX,
            U256::from(1000u64),
            30,
        )
        .unwrap();
        assert_eq!(
            pool.get_amount_out(true, U256::from(1_000_000u64)),
            Err(QuoteError::Overflow)
        );
    }

    #[test]
    fn amount_in_reserve_overflow_is_reported() {
        let pool = V2Pool::new(Address::ZERO, Address::ZERO, U256::MAX, U256::MAX, 30).unwrap();
        assert_eq!(
            pool.get_amount_in(true, U256::from(1000u64)),
            Err(QuoteError::Overflow)
        );
    }

    #[test]
    fn invalid_fee_is_rejected() {
        let r = V2Pool::new(
            Address::ZERO,
            Address::ZERO,
            U256::from(1u64),
            U256::from(1u64),
            10_000,
        );
        assert_eq!(r.unwrap_err(), QuoteError::InvalidFee);
    }

    // Note: `get_amount_out(get_amount_in(x)) >= x` does NOT hold here. `get_amount_out` pre-divides
    // the fee (`amount_in * (FEE_DENOMINATOR - fee) / FEE_DENOMINATOR`), so it is not the exact
    // inverse of `get_amount_in`; asserting a round-trip lower bound would contradict the formulas.
    proptest::proptest! {
        #[test]
        fn more_input_never_less_output(
            reserve_in in 1_000u128..=u128::from(u64::MAX),
            reserve_out in 1_000u128..=u128::from(u64::MAX),
            a in 1u64..=u64::from(u32::MAX),
            b in 1u64..=u64::from(u32::MAX),
        ) {
            let pool = V2Pool::new(Address::ZERO, Address::ZERO,
                U256::from(reserve_in), U256::from(reserve_out), 30).unwrap();
            let lo = pool.get_amount_out(true, U256::from(a.min(b))).unwrap();
            let hi = pool.get_amount_out(true, U256::from(a.max(b))).unwrap();
            proptest::prop_assert!(hi >= lo);
        }
    }
}
