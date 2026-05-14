//! An XPath signature parameter literally named `$context` does
//! *not* shadow the wrapper's injected context local, because the
//! wrapper uses prefixed names (`__xpath_fn_context` etc.) for its
//! own parameters. This fixture compiles when the macro correctly
//! emits the prefixed locals; it would break if the wrapper's
//! injected param were still named `context`.

use xee_interpreter::{context::DynamicContext, sequence::Sequence};
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_shadow_test($context as item()*) as item()*")]
fn my_shadow_test(
    #[xpath_context] _ctx: &DynamicContext,
    context: &Sequence,
) -> Sequence {
    context.clone()
}

fn main() {
    let _ = my_shadow_test;
}
