//! Signature declares two XPath parameters but the Rust function
//! only takes one. The macro should surface this as a clean error
//! pointing at the Rust function name, not a generic index-out-of-
//! bounds panic from the macro's argument lookup.

use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:short($a as item()*, $b as item()*) as item()*")]
fn short(_s: &xee_interpreter::sequence::Sequence) -> xee_interpreter::sequence::Sequence {
    panic!()
}

fn main() {}
