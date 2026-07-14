//! Domain newtypes for V3 quantities.
//!
//! Wrapping ticks, sqrt prices, and liquidity in distinct types makes wrong-unit arithmetic a
//! compile error (`api-newtype-safety`, `type-newtype-validated`). Amounts and reserves stay plain
//! [`U256`], since they are interchangeable token quantities. Every newtype is
//! `#[repr(transparent)]`, `Copy`, and totally ordered for use in swap comparisons.

use alloy_primitives::U256;

use crate::error::QuoteError;
use crate::v3::constants::{MAX_TICK, MIN_TICK};

/// A price tick, validated to `[MIN_TICK, MAX_TICK]` at construction.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(i32);

impl Tick {
    /// Creates a tick, rejecting values outside `[MIN_TICK, MAX_TICK]`.
    ///
    /// # Errors
    /// [`QuoteError::TickOutOfRange`] if `tick` is out of range.
    pub const fn new(tick: i32) -> Result<Self, QuoteError> {
        if tick < MIN_TICK || tick > MAX_TICK {
            return Err(QuoteError::TickOutOfRange(tick));
        }
        Ok(Self(tick))
    }

    /// The wrapped tick value.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A Q64.96 sqrt price (`sqrt(price) * 2^96`). Not range-validated on its own; the tick and swap
/// math enforce bounds where required.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SqrtPriceX96(U256);

impl SqrtPriceX96 {
    /// Wraps a raw Q64.96 value.
    #[must_use]
    pub const fn new(value: U256) -> Self {
        Self(value)
    }

    /// The wrapped Q64.96 value.
    #[must_use]
    pub const fn get(self) -> U256 {
        self.0
    }
}

/// Active in-range liquidity `L`. Net changes across ticks are signed (`i128`).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Liquidity(u128);

impl Liquidity {
    /// Wraps a raw liquidity value.
    #[must_use]
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    /// The wrapped liquidity value.
    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }

    /// Whether the liquidity is zero (a gap in the price range).
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}
