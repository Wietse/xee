//! `xs:integer?` — the optional path goes through
//! `occurrence::option`, returning an `Option<T>` to the user's
//! function.

use ibig::IBig;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_default_or_zero($n as xs:integer?) as xs:integer")]
fn my_default_or_zero(n: Option<IBig>) -> IBig {
    n.unwrap_or_else(|| IBig::from(0))
}

fn main() {
    let _ = my_default_or_zero;
}
