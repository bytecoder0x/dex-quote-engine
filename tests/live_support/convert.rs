use dex_quote_engine::U256;

#[allow(clippy::needless_pass_by_value)]
pub fn to_u256(x: impl ToString) -> U256 {
    U256::from_str_radix(&x.to_string(), 10).expect("decimal integer")
}

#[allow(clippy::needless_pass_by_value)]
pub fn to_i32(x: impl ToString) -> i32 {
    x.to_string().parse().expect("i32")
}

#[allow(clippy::needless_pass_by_value)]
pub fn to_i128(x: impl ToString) -> i128 {
    x.to_string().parse().expect("i128")
}
