//! `xs:string` parameter — atomic-type bridging that produces a
//! borrowed `&str` (the macro emits `let a = a.as_ref();` after
//! the `unboxed_atomized` conversion).

use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_uppercase($s as xs:string) as xs:string")]
fn my_uppercase(s: &str) -> String {
    s.to_uppercase()
}

fn main() {
    let _ = my_uppercase;
}
