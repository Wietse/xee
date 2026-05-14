//! `xs:integer` parameter — atomic-type bridging through
//! `Sequence::unboxed_atomized` + `Interpreter::xot`. Compiles only
//! when both methods are publicly reachable.

use ibig::IBig;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_double($n as xs:integer) as xs:integer")]
fn my_double(n: IBig) -> IBig {
    n * 2
}

fn main() {
    let _ = my_double;
}
