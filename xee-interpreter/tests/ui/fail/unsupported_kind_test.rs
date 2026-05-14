//! `attribute()` is not yet supported in #[xpath_fn] signatures.
//! This used to panic at macro-expansion time via
//! `todo!("Unsupported kind test")`; now it surfaces as a clean
//! compile error pointing at the user's Rust argument.

use xee_interpreter::sequence::Sequence;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:attr_test($a as attribute()*) as item()*")]
fn attr_test(a: &Sequence) -> Sequence {
    a.clone()
}

fn main() {}
