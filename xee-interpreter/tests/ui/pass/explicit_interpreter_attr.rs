//! `#[xpath_interpreter]` injects the interpreter regardless of the
//! variable's name.

use xee_interpreter::{interpreter::Interpreter, sequence::Sequence};
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_with_interp($s as item()*) as item()*")]
fn my_with_interp(
    #[xpath_interpreter] _interp: &mut Interpreter,
    s: &Sequence,
) -> Sequence {
    s.clone()
}

fn main() {
    let _ = my_with_interp;
}
