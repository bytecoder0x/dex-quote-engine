//! The [`V3Pool`] snapshot and its tick-array swap loop.
//!
//! The pool holds an ordered tick array laid out as `reverse(zero_for_one_ticks) ++ current_tick ++
//! one_for_zero_ticks`, with `current_tick_index` marking the current position. A swap starts at the
//! array slot **adjacent** to the current tick (`current_tick_index ∓ 1`) and walks outward by index
//! (∓1 per iteration), running one [`compute_swap_step`] per range. It stops when the input is
//! spent, the price limit is reached, or the pre-loaded array is exhausted.

use alloy_primitives::{Address, U256};

use crate::error::QuoteError;
use crate::v3::constants::{MAX_SQRT_RATIO, MIN_SQRT_RATIO, PIPS_DENOMINATOR};
use crate::v3::swap_math::compute_swap_step;
use crate::v3::tick_math::get_sqrt_ratio_at_tick;
use crate::v3::types::{Liquidity, SqrtPriceX96, Tick};

/// An initialized tick and its net liquidity change (crossing upward adds it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickData {
    /// The tick index.
    pub tick: Tick,
    /// Whether this tick is initialized (its liquidity net is applied when crossed).
    pub initialized: bool,
    /// Signed liquidity delta applied when crossing this tick upward.
    pub liquidity_net: i128,
}

impl TickData {
    /// Creates tick data, validating the tick range.
    ///
    /// # Errors
    /// [`QuoteError::TickOutOfRange`] if `tick` is out of range.
    pub fn new(tick: i32, initialized: bool, liquidity_net: i128) -> Result<Self, QuoteError> {
        Ok(Self {
            tick: Tick::new(tick)?,
            initialized,
            liquidity_net,
        })
    }
}

/// Construction parameters for a [`V3Pool`] snapshot. `zero_for_one_ticks` and `one_for_zero_ticks`
/// are the pre-loaded ticks below/above the current price, each in crossing order (nearest first).
#[derive(Debug, Clone)]
pub struct PoolStateV3Params {
    /// The pool's `token0` address.
    pub token0: Address,
    /// The pool's `token1` address.
    pub token1: Address,
    /// The current tick.
    pub tick: Tick,
    /// The current tick's net liquidity (unused by the loop; the current tick is never re-crossed).
    pub tick_liquidity_net: i128,
    /// The pool's tick spacing.
    pub tick_spacing: i32,
    /// The fee in pips.
    pub fee: u32,
    /// The current sqrt price (Q64.96).
    pub sqrt_price_x96: SqrtPriceX96,
    /// The current active liquidity.
    pub liquidity: Liquidity,
    /// Pre-loaded ticks below the current price, in crossing order (nearest below first).
    pub zero_for_one_ticks: Vec<TickData>,
    /// Pre-loaded ticks above the current price, in crossing order (nearest above first).
    pub one_for_zero_ticks: Vec<TickData>,
}

/// A concentrated-liquidity pool snapshot. Quotes are pure and take `&self`.
#[derive(Debug, Clone)]
pub struct V3Pool {
    token0: Address,
    token1: Address,
    tick_spacing: i32,
    fee: u32,
    liquidity: Liquidity,
    sqrt_price_x96: SqrtPriceX96,
    ticks: Vec<TickData>,
    current_tick_index: usize,
}

impl V3Pool {
    /// Builds a pool from a state snapshot, assembling the tick array as
    /// `reverse(zero_for_one_ticks) ++ current_tick ++ one_for_zero_ticks`.
    ///
    /// # Errors
    /// [`QuoteError::InvalidFee`] if `fee` is not below [`PIPS_DENOMINATOR`];
    /// [`QuoteError::InvalidTickSpacing`] if `tick_spacing` is not positive.
    pub fn new(params: PoolStateV3Params) -> Result<Self, QuoteError> {
        if params.fee >= PIPS_DENOMINATOR {
            return Err(QuoteError::InvalidFee);
        }
        if params.tick_spacing <= 0 {
            return Err(QuoteError::InvalidTickSpacing(params.tick_spacing));
        }

        let mut ticks = params.zero_for_one_ticks;
        ticks.reverse();
        let current_tick_index = ticks.len();
        ticks.push(TickData {
            tick: params.tick,
            initialized: true,
            liquidity_net: params.tick_liquidity_net,
        });
        ticks.extend(params.one_for_zero_ticks);

        Ok(Self {
            token0: params.token0,
            token1: params.token1,
            tick_spacing: params.tick_spacing,
            fee: params.fee,
            liquidity: params.liquidity,
            sqrt_price_x96: params.sqrt_price_x96,
            ticks,
            current_tick_index,
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

    /// The current sqrt price (Q64.96).
    #[must_use]
    pub const fn sqrt_price_x96(&self) -> SqrtPriceX96 {
        self.sqrt_price_x96
    }

    /// The current active liquidity.
    #[must_use]
    pub const fn liquidity(&self) -> Liquidity {
        self.liquidity
    }

    /// The fee in pips.
    #[must_use]
    pub const fn fee(&self) -> u32 {
        self.fee
    }

    /// The pool's tick spacing.
    #[must_use]
    pub const fn tick_spacing(&self) -> i32 {
        self.tick_spacing
    }

    /// The tick data currently pointed at by `current_tick_index`.
    #[must_use]
    pub fn current_tick_data(&self) -> TickData {
        self.ticks[self.current_tick_index]
    }

    /// Returns `(amount_out, amount_remaining)` for an exact `amount_in` in the given direction.
    /// `amount_remaining` is non-zero when the pre-loaded ticks are exhausted before the input is.
    ///
    /// # Errors
    /// Propagates arithmetic errors from the step math.
    pub fn get_amount_out(
        &self,
        zero_for_one: bool,
        amount_in: U256,
    ) -> Result<(U256, U256), QuoteError> {
        if amount_in.is_zero() {
            return Ok((U256::ZERO, U256::ZERO));
        }
        self.run_swap(zero_for_one, amount_in, true)
    }

    /// Returns `(amount_in, amount_remaining)` for an exact `amount_out` in the given direction.
    /// `amount_remaining` is non-zero when the pre-loaded ticks are exhausted before the output is
    /// fully delivered.
    ///
    /// # Errors
    /// Propagates arithmetic errors from the step math.
    pub fn get_amount_in(
        &self,
        zero_for_one: bool,
        amount_out: U256,
    ) -> Result<(U256, U256), QuoteError> {
        if amount_out.is_zero() {
            return Ok((U256::ZERO, U256::ZERO));
        }
        self.run_swap(zero_for_one, amount_out, false)
    }

    fn run_swap(
        &self,
        zero_for_one: bool,
        amount_specified: U256,
        exact_in: bool,
    ) -> Result<(U256, U256), QuoteError> {
        let sqrt_limit = default_limit(zero_for_one);
        let mut sqrt_price = self.sqrt_price_x96;
        let mut liquidity = self.liquidity;
        let mut amount_remaining = amount_specified;
        let mut amount_calculated = U256::ZERO;
        let len = self.ticks.len();

        // Start adjacent to the current tick; the current tick itself is never re-crossed.
        let mut cursor = step_cursor(self.current_tick_index, zero_for_one, len);

        loop {
            if amount_remaining.is_zero() || sqrt_price == sqrt_limit {
                break;
            }
            let Some(index) = cursor else { break };
            let tick_data = self.ticks[index];

            let sqrt_next_tick = get_sqrt_ratio_at_tick(tick_data.tick);
            let sqrt_target = if zero_for_one {
                sqrt_next_tick.max(sqrt_limit)
            } else {
                sqrt_next_tick.min(sqrt_limit)
            };

            let step = compute_swap_step(
                sqrt_price,
                sqrt_target,
                liquidity,
                amount_remaining,
                exact_in,
                self.fee,
            )?;
            sqrt_price = step.sqrt_price_next;

            let step_input = step
                .amount_in
                .checked_add(step.fee_amount)
                .ok_or(QuoteError::Overflow)?;
            let (spent, gained) = if exact_in {
                (step_input, step.amount_out)
            } else {
                (step.amount_out, step_input)
            };
            amount_remaining = amount_remaining
                .checked_sub(spent)
                .ok_or(QuoteError::Overflow)?;
            amount_calculated = amount_calculated
                .checked_add(gained)
                .ok_or(QuoteError::Overflow)?;

            // Cross the tick only when the price actually landed on it.
            if sqrt_price == sqrt_next_tick && tick_data.initialized {
                liquidity = cross_tick(liquidity, tick_data.liquidity_net, zero_for_one)?;
            }

            cursor = step_cursor(index, zero_for_one, len);
        }

        Ok((amount_calculated, amount_remaining))
    }
}

/// The next array slot outward from `index`: one lower going down, one higher (in bounds) going up.
fn step_cursor(index: usize, zero_for_one: bool, len: usize) -> Option<usize> {
    if zero_for_one {
        index.checked_sub(1)
    } else {
        (index + 1 < len).then_some(index + 1)
    }
}

/// The widest price limit for a direction (`MIN_SQRT_RATIO + 1` down, `MAX_SQRT_RATIO - 1` up).
fn default_limit(zero_for_one: bool) -> SqrtPriceX96 {
    if zero_for_one {
        SqrtPriceX96::new(MIN_SQRT_RATIO + U256::ONE)
    } else {
        SqrtPriceX96::new(MAX_SQRT_RATIO - U256::ONE)
    }
}

/// Applies a tick's net liquidity when crossing it. Moving up (`!zero_for_one`) adds `net`; moving
/// down subtracts it. `unsigned_abs` avoids overflow at `i128::MIN`.
fn cross_tick(
    liquidity: Liquidity,
    net: i128,
    zero_for_one: bool,
) -> Result<Liquidity, QuoteError> {
    let base = liquidity.get();
    let magnitude = net.unsigned_abs();
    let add = (net >= 0) != zero_for_one;
    let updated = if add {
        base.checked_add(magnitude).ok_or(QuoteError::Overflow)?
    } else {
        base.checked_sub(magnitude)
            .ok_or(QuoteError::InsufficientLiquidity)?
    };
    Ok(Liquidity::new(updated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3::constants::Q96;

    fn addr(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    fn pool_with_range(liquidity: u128) -> V3Pool {
        let net = i128::try_from(liquidity).unwrap();
        V3Pool::new(PoolStateV3Params {
            token0: addr(1),
            token1: addr(2),
            tick: Tick::new(0).unwrap(),
            tick_liquidity_net: 0,
            tick_spacing: 60,
            fee: 3000,
            sqrt_price_x96: SqrtPriceX96::new(Q96),
            liquidity: Liquidity::new(liquidity),
            zero_for_one_ticks: vec![TickData::new(-600, true, net).unwrap()],
            one_for_zero_ticks: vec![TickData::new(600, true, -net).unwrap()],
        })
        .unwrap()
    }

    #[test]
    fn tick_array_layout_places_current_in_the_middle() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        assert_eq!(pool.current_tick_index, 1);
        assert_eq!(pool.ticks[0].tick.get(), -600);
        assert_eq!(pool.current_tick_data().tick.get(), 0);
        assert_eq!(pool.ticks[2].tick.get(), 600);
    }

    #[test]
    fn exact_in_zero_for_one_lowers_price_and_pays_out() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        let (out, remaining) = pool
            .get_amount_out(true, U256::from(1_000_000_000_000u64))
            .unwrap();
        assert!(out > U256::ZERO);
        assert!(pool.sqrt_price_x96().get() == Q96); // pure quote does not mutate
        assert!(remaining.is_zero());
    }

    #[test]
    fn exact_in_one_for_zero_pays_out() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        let (out, remaining) = pool
            .get_amount_out(false, U256::from(1_000_000_000_000u64))
            .unwrap();
        assert!(out > U256::ZERO);
        assert!(remaining.is_zero());
    }

    #[test]
    fn small_swap_stays_within_range() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        let (out, remaining) = pool.get_amount_out(true, U256::from(1_000_000u64)).unwrap();
        assert!(out > U256::ZERO);
        assert!(remaining.is_zero());
    }

    #[test]
    fn huge_swap_exhausts_ticks_and_reports_remaining() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        let (_out, remaining) = pool.get_amount_out(true, U256::MAX).unwrap();
        assert!(remaining > U256::ZERO);
    }

    #[test]
    fn zero_amount_in_returns_zero() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        assert_eq!(
            pool.get_amount_out(true, U256::ZERO).unwrap(),
            (U256::ZERO, U256::ZERO)
        );
    }

    #[test]
    fn exact_out_requires_more_input_than_output() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        let want_out = U256::from(1_000_000_000u64);
        let (amount_in, remaining) = pool.get_amount_in(true, want_out).unwrap();
        assert!(amount_in > want_out);
        assert!(remaining.is_zero());
    }

    #[test]
    fn exact_out_exhausts_ticks_and_reports_remaining() {
        let pool = pool_with_range(1_000_000_000_000_000_000);
        let (_amount_in, remaining) = pool.get_amount_in(true, U256::MAX).unwrap();
        assert!(remaining > U256::ZERO);
    }

    #[test]
    fn non_positive_tick_spacing_is_rejected() {
        let result = V3Pool::new(PoolStateV3Params {
            token0: addr(1),
            token1: addr(2),
            tick: Tick::new(0).unwrap(),
            tick_liquidity_net: 0,
            tick_spacing: 0,
            fee: 3000,
            sqrt_price_x96: SqrtPriceX96::new(Q96),
            liquidity: Liquidity::new(1),
            zero_for_one_ticks: vec![],
            one_for_zero_ticks: vec![],
        });
        assert_eq!(result.unwrap_err(), QuoteError::InvalidTickSpacing(0));
    }

    #[test]
    fn cross_tick_applies_net_by_direction() {
        let l = Liquidity::new(1000);
        assert_eq!(cross_tick(l, 400, false).unwrap().get(), 1400); // up, +net
        assert_eq!(cross_tick(l, -400, false).unwrap().get(), 600); // up, -|net|
        assert_eq!(cross_tick(l, 400, true).unwrap().get(), 600); // down, -net
        assert_eq!(cross_tick(l, -400, true).unwrap().get(), 1400); // down, +|net|
    }

    #[test]
    fn cross_tick_underflow_is_insufficient_liquidity() {
        assert_eq!(
            cross_tick(Liquidity::new(100), -400, false),
            Err(QuoteError::InsufficientLiquidity)
        );
    }

    // Two ranges above the price: liquidity doubles past tick 60. A swap that crosses tick 60 must
    // continue in the 2·L range — the exact output only matches if cross_tick updated liquidity.
    #[test]
    fn crossing_a_tick_updates_liquidity_for_the_next_range() {
        let l0 = 1_000_000_000_000_000_000u128;
        let net = i128::try_from(l0).unwrap();
        let pool = V3Pool::new(PoolStateV3Params {
            token0: addr(1),
            token1: addr(2),
            tick: Tick::new(0).unwrap(),
            tick_liquidity_net: 0,
            tick_spacing: 60,
            fee: 3000,
            sqrt_price_x96: SqrtPriceX96::new(Q96),
            liquidity: Liquidity::new(l0),
            zero_for_one_ticks: vec![],
            one_for_zero_ticks: vec![
                TickData::new(60, true, net).unwrap(),
                TickData::new(600, true, -(2 * net)).unwrap(),
            ],
        })
        .unwrap();

        let amount_in = U256::from(5_000_000_000_000_000u64);
        let (out, remaining) = pool.get_amount_out(false, amount_in).unwrap();
        assert!(remaining.is_zero());

        let sqrt60 = get_sqrt_ratio_at_tick(Tick::new(60).unwrap());
        let sqrt600 = get_sqrt_ratio_at_tick(Tick::new(600).unwrap());
        let step1 = compute_swap_step(
            SqrtPriceX96::new(Q96),
            sqrt60,
            Liquidity::new(l0),
            amount_in,
            true,
            3000,
        )
        .unwrap();
        let after1 = amount_in - (step1.amount_in + step1.fee_amount);
        let step2 =
            compute_swap_step(sqrt60, sqrt600, Liquidity::new(2 * l0), after1, true, 3000).unwrap();
        assert_eq!(step1.sqrt_price_next, sqrt60); // step 1 actually reached and crossed tick 60
        assert_eq!(out, step1.amount_out + step2.amount_out);
    }

    // The current range holds zero liquidity; the swap must hop it at zero cost and resume in the
    // funded range past tick 60.
    #[test]
    fn zero_liquidity_gap_is_traversed() {
        let l1 = 1_000_000_000_000_000_000u128;
        let net = i128::try_from(l1).unwrap();
        let pool = V3Pool::new(PoolStateV3Params {
            token0: addr(1),
            token1: addr(2),
            tick: Tick::new(0).unwrap(),
            tick_liquidity_net: 0,
            tick_spacing: 60,
            fee: 3000,
            sqrt_price_x96: SqrtPriceX96::new(Q96),
            liquidity: Liquidity::new(0),
            zero_for_one_ticks: vec![],
            one_for_zero_ticks: vec![
                TickData::new(60, true, net).unwrap(),
                TickData::new(600, true, -net).unwrap(),
            ],
        })
        .unwrap();

        let (out, remaining) = pool
            .get_amount_out(false, U256::from(1_000_000_000u64))
            .unwrap();
        assert!(out > U256::ZERO);
        assert!(remaining.is_zero());
    }

    proptest::proptest! {
        #[test]
        fn more_input_never_less_output(
            a in 1u64..=5_000_000_000,
            b in 1u64..=5_000_000_000,
        ) {
            let pool = pool_with_range(1_000_000_000_000_000_000);
            let (lo, _) = pool.get_amount_out(true, U256::from(a.min(b))).unwrap();
            let (hi, _) = pool.get_amount_out(true, U256::from(a.max(b))).unwrap();
            proptest::prop_assert!(hi >= lo);
        }
    }
}
