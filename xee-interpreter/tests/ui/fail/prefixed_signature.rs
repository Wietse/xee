//! Extension functions must name themselves with `Q{uri}local`, not a
//! prefixed QName. `#[xpath_fn]` parses the signature against the
//! standard namespaces at macro-expansion time, so an unknown prefix
//! is a clean compile error here — long before any namespace
//! registration on the `StaticContextBuilder` could apply.

use xee_xpath_macros::xpath_fn;

#[xpath_fn("xfi:add($a as item()*) as item()*")]
fn prefixed_arg() {}

fn main() {}
