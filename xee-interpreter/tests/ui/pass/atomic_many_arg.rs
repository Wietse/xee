//! `xs:integer*` — the many path hands the user an iterator
//! over `Result<IBig, _>` produced by `unboxed_atomized`. The
//! macro-generated wrapper references `my_sum` internally, so
//! the function is reachable even though `main` doesn't name it.

use ibig::IBig;
use xee_interpreter::error;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_sum($ns as xs:integer*) as xs:integer")]
fn my_sum(ns: impl Iterator<Item = error::Result<IBig>>) -> error::Result<IBig> {
    let mut total = IBig::from(0);
    for n in ns {
        total += n?;
    }
    Ok(total)
}

fn main() {}
