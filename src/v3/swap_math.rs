//! A single swap step within one liquidity range (`SwapMath.computeSwapStep`).

use alloy_primitives::U256;

use crate::error::QuoteError;
use crate::math::{mul_div, mul_div_rounding_up};
use crate::v3::constants::PIPS_DENOMINATOR;
use crate::v3::sqrt_price_math::{
    get_amount0_delta, get_amount1_delta, next_sqrt_price_from_input, next_sqrt_price_from_output,
};
use crate::v3::types::{Liquidity, SqrtPriceX96};

/// The outcome of one swap step: the price reached and the amounts moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapStep {
    /// Sqrt price after the step.
    pub sqrt_price_next: SqrtPriceX96,
    /// Input consumed, **excluding** the fee.
    pub amount_in: U256,
    /// Output produced.
    pub amount_out: U256,
    /// Fee taken on the input.
    pub fee_amount: U256,
}

/// Computes one swap step from `sqrt_current` toward `sqrt_target` within a single liquidity range.
///
/// Direction is inferred from the prices (`sqrt_current >= sqrt_target` ⇒ token0-in). The invariant
/// held in every branch: `amount_in` rounds **up**, `amount_out` rounds **down**, `fee_amount`
/// rounds **up**. When an exact-input step does not reach the target, all leftover input becomes
/// fee (not recomputed) so multi-step totals stay wei-exact.
///
/// # Errors
/// [`QuoteError::InvalidFee`] if `fee_pips` is not below [`PIPS_DENOMINATOR`]; propagates arithmetic
/// errors ([`QuoteError::Overflow`], [`QuoteError::SqrtPriceOutOfRange`], …) from the price and
/// delta math.
pub fn compute_swap_step(
    sqrt_current: SqrtPriceX96,
    sqrt_target: SqrtPriceX96,
    liquidity: Liquidity,
    amount_remaining: U256,
    exact_in: bool,
    fee_pips: u32,
) -> Result<SwapStep, QuoteError> {
    if fee_pips >= PIPS_DENOMINATOR {
        return Err(QuoteError::InvalidFee);
    }
    let zero_for_one = sqrt_current >= sqrt_target;
    let pips = U256::from(PIPS_DENOMINATOR);
    let fee = U256::from(fee_pips);
    let fee_complement = pips - fee;

    let mut amount_in = U256::ZERO;
    let mut amount_out = U256::ZERO;
    let sqrt_next: SqrtPriceX96;

    if exact_in {
        let amount_remaining_less_fee = mul_div(amount_remaining, fee_complement, pips)?;
        amount_in = if zero_for_one {
            get_amount0_delta(sqrt_target, sqrt_current, liquidity, true)?
        } else {
            get_amount1_delta(sqrt_current, sqrt_target, liquidity, true)?
        };
        sqrt_next = if amount_remaining_less_fee >= amount_in {
            sqrt_target
        } else {
            next_sqrt_price_from_input(
                sqrt_current,
                liquidity,
                amount_remaining_less_fee,
                zero_for_one,
            )?
        };
    } else {
        amount_out = if zero_for_one {
            get_amount1_delta(sqrt_target, sqrt_current, liquidity, false)?
        } else {
            get_amount0_delta(sqrt_current, sqrt_target, liquidity, false)?
        };
        sqrt_next = if amount_remaining >= amount_out {
            sqrt_target
        } else {
            next_sqrt_price_from_output(sqrt_current, liquidity, amount_remaining, zero_for_one)?
        };
    }

    let reached_target = sqrt_target == sqrt_next;

    // Recompute both amounts for the price actually reached (input up, output down).
    if zero_for_one {
        if !(reached_target && exact_in) {
            amount_in = get_amount0_delta(sqrt_next, sqrt_current, liquidity, true)?;
        }
        if !reached_target || exact_in {
            amount_out = get_amount1_delta(sqrt_next, sqrt_current, liquidity, false)?;
        }
    } else {
        if !(reached_target && exact_in) {
            amount_in = get_amount1_delta(sqrt_current, sqrt_next, liquidity, true)?;
        }
        if !reached_target || exact_in {
            amount_out = get_amount0_delta(sqrt_current, sqrt_next, liquidity, false)?;
        }
    }

    // Exact-output can never deliver more than requested.
    if !exact_in && amount_out > amount_remaining {
        amount_out = amount_remaining;
    }

    let fee_amount = if exact_in && !reached_target {
        // Step consumed everything without reaching target: leftover input is the fee.
        amount_remaining - amount_in
    } else {
        mul_div_rounding_up(amount_in, fee, fee_complement)?
    };

    Ok(SwapStep {
        sqrt_price_next: sqrt_next,
        amount_in,
        amount_out,
        fee_amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3::constants::Q96;

    fn price(mult: u64) -> SqrtPriceX96 {
        SqrtPriceX96::new(Q96 * U256::from(mult))
    }

    #[test]
    fn exact_in_reaching_target_charges_fee_on_input() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let step =
            compute_swap_step(price(2), price(1), l, U256::from(u128::MAX), true, 3000).unwrap();
        assert_eq!(step.sqrt_price_next, price(1));
        let expected_in = get_amount0_delta(price(1), price(2), l, true).unwrap();
        let expected_out = get_amount1_delta(price(1), price(2), l, false).unwrap();
        assert_eq!(step.amount_in, expected_in);
        assert_eq!(step.amount_out, expected_out);
        let expected_fee =
            mul_div_rounding_up(expected_in, U256::from(3000u32), U256::from(997_000u32)).unwrap();
        assert_eq!(step.fee_amount, expected_fee);
    }

    #[test]
    fn exact_in_not_reaching_target_consumes_all_input() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let remaining = U256::from(1_000_000u64);
        let step = compute_swap_step(price(2), price(1), l, remaining, true, 3000).unwrap();
        assert_ne!(step.sqrt_price_next, price(1));
        assert_eq!(step.amount_in + step.fee_amount, remaining);
    }

    #[test]
    fn exact_out_partial_step_pins_in_out_and_fee() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        let want_out = U256::from(1000u64);
        let step = compute_swap_step(price(2), price(1), l, want_out, false, 3000).unwrap();

        let sqrt_next = next_sqrt_price_from_output(price(2), l, want_out, true).unwrap();
        let expected_in = get_amount0_delta(sqrt_next, price(2), l, true).unwrap();
        let expected_fee =
            mul_div_rounding_up(expected_in, U256::from(3000u32), U256::from(997_000u32)).unwrap();

        assert_eq!(step.sqrt_price_next, sqrt_next);
        assert_eq!(step.amount_out, want_out);
        assert_eq!(step.amount_in, expected_in);
        assert_eq!(step.fee_amount, expected_fee);
        assert!(sqrt_next.get() < price(2).get() && sqrt_next.get() > price(1).get());
    }

    #[test]
    fn fee_at_or_above_denominator_is_rejected() {
        let l = Liquidity::new(1_000_000_000_000_000_000);
        assert_eq!(
            compute_swap_step(price(2), price(1), l, U256::from(1000u64), true, 1_000_000),
            Err(QuoteError::InvalidFee)
        );
    }

    proptest::proptest! {
        #[test]
        fn exact_in_never_spends_more_than_remaining(
            remaining in 1u128..=u128::from(u64::MAX),
        ) {
            let l = Liquidity::new(1_000_000_000_000_000_000);
            let step = compute_swap_step(
                price(2), price(1), l, U256::from(remaining), true, 3000,
            ).unwrap();
            proptest::prop_assert!(step.amount_in + step.fee_amount <= U256::from(remaining));
        }
    }
}
