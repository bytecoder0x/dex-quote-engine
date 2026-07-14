//! V2 numeric constants.

/// Fee denominator. Fees are expressed in basis points (`30` = 0.30%): the input multiplier is
/// `FEE_DENOMINATOR - fee`.
pub const FEE_DENOMINATOR: u32 = 10_000;
