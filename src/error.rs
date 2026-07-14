//! Error type for quote computations.

/// An error returned by a swap-quote calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QuoteError {
    /// A division had a zero denominator.
    #[error("division by zero")]
    DivisionByZero,

    /// An intermediate value exceeded the 256-bit range.
    #[error("arithmetic overflow")]
    Overflow,

    /// A reserve or output amount left no usable liquidity.
    #[error("insufficient liquidity")]
    InsufficientLiquidity,

    /// The fee was not below the fee denominator.
    #[error("invalid fee")]
    InvalidFee,

    /// A tick was outside the valid `[MIN_TICK, MAX_TICK]` range.
    #[error("tick {0} out of range")]
    TickOutOfRange(i32),

    /// The tick spacing was not a positive value.
    #[error("invalid tick spacing {0}")]
    InvalidTickSpacing(i32),

    /// A sqrt price was outside its valid range (`[MIN_SQRT_RATIO, MAX_SQRT_RATIO)`, or `0`).
    #[error("sqrt price out of range")]
    SqrtPriceOutOfRange,

    /// A swap price limit was on the wrong side of the current price.
    #[error("price limit on wrong side")]
    PriceLimitWrongSide,
}
