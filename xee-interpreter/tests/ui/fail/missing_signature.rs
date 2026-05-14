//! `#[xpath_fn(context_first)]` with no signature string literal
//! should surface as a clean compile error, not a panic from
//! `signature.expect("Signature not found")` (the historical
//! behaviour before the macro hardening).

use xee_xpath_macros::xpath_fn;

#[xpath_fn(context_first)]
fn missing_signature() -> i32 {
    42
}

fn main() {}
