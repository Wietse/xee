//! `#[xpath_context]` injects the dynamic context regardless of the
//! variable's name. The wrapper must compile correctly — i.e. the
//! prefixed local `__xpath_fn_context` is what gets passed into the
//! user's function, not the user's renamed `ctx`.

use xee_interpreter::{context::DynamicContext, sequence::Sequence};
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_with_context($s as item()*) as item()*")]
fn my_with_context(
    #[xpath_context] _ctx: &DynamicContext,
    s: &Sequence,
) -> Sequence {
    s.clone()
}

fn main() {
    let _ = my_with_context;
}
