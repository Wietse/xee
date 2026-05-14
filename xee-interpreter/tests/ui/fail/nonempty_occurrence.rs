//! NonEmpty occurrence (`+`) is not yet supported. Used to panic
//! via `todo!`; now surfaces as a clean compile error.

use xee_interpreter::sequence::Sequence;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:nonempty_arg($a as item()+) as item()*")]
fn nonempty_arg(a: &Sequence) -> Sequence {
    a.clone()
}

fn main() {}
