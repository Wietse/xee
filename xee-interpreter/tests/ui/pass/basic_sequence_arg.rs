//! A minimal `#[xpath_fn]` with a `&Sequence` parameter compiles
//! cleanly outside `xee-interpreter`. This is the bare-minimum
//! happy-path test for the macro's absolute-path emission.

use xee_interpreter::sequence::Sequence;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_passthrough($s as item()*) as item()*")]
fn my_passthrough(s: &Sequence) -> Sequence {
    s.clone()
}

fn main() {
    let _ = my_passthrough;
}
