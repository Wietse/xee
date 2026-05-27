//! Compile-time validation that `#[xpath_fn]` emits paths that resolve
//! from outside `xee-interpreter`'s own source tree.
//!
//! Integration tests under `tests/` are compiled as a separate crate
//! that depends on `xee-interpreter`. If the macro emits paths the
//! external crate cannot resolve, this file fails to build — which is
//! exactly the regression class snapshot tests cannot catch.
//!
//! Each test function takes its argument as `&Sequence` and returns a
//! `Sequence`. That deliberately avoids exercising the method-call
//! paths the macro emits for atomic-type arguments
//! (`Sequence::unboxed_atomized`, `Interpreter::xot()`, etc.) — those
//! methods are still `pub(crate)` and exposing them is a follow-up to
//! this PR. The signatures here are enough to validate that:
//!
//!   * the macro's emitted absolute paths to types
//!     (`::xee_interpreter::context::DynamicContext`,
//!     `::xee_interpreter::sequence::Sequence`,
//!     `::xee_interpreter::function::StaticFunctionType`, etc.)
//!     resolve from an external crate;
//!   * `wrap_xpath_fn!` expands cleanly outside `xee-interpreter`,
//!     reaching `StaticFunctionDescription`, `FunctionKind`, and
//!     `FunctionKind::parse` through the now-`pub #[doc(hidden)]`
//!     re-exports under `xee_interpreter::function`;
//!   * the `#[xpath_context]` / `#[xpath_interpreter]` parameter
//!     attributes work as the explicit injection mechanism for
//!     renamed parameters (the renamed-arg variants below all use
//!     the attributes, not name-based detection).

use xee_interpreter::{
    context::DynamicContext, interpreter::Interpreter, sequence::Sequence, wrap_xpath_fn,
};
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:test_passthrough($s as item()*) as item()*")]
fn test_passthrough(s: &Sequence) -> Sequence {
    s.clone()
}

#[xpath_fn("fn:test_ctx_rename($s as item()*) as item()*")]
fn test_ctx_rename(#[xpath_context] _ctx: &DynamicContext, s: &Sequence) -> Sequence {
    s.clone()
}

#[xpath_fn("fn:test_interp_rename($s as item()*) as item()*")]
fn test_interp_rename(#[xpath_interpreter] _interp: &mut Interpreter, s: &Sequence) -> Sequence {
    s.clone()
}

#[xpath_fn("fn:test_both_renamed($s as item()*) as item()*")]
fn test_both_renamed(
    #[xpath_context] _ctx: &DynamicContext,
    #[xpath_interpreter] _interp: &mut Interpreter,
    s: &Sequence,
) -> Sequence {
    s.clone()
}

/// Building a `StaticFunctionDescription` via `wrap_xpath_fn!` exercises
/// every macro-emitted path: it pulls the wrapper fn pointer (typed as
/// `StaticFunctionType`), the signature string, and the function kind —
/// all under `::xee_interpreter::function::*` paths.
#[test]
fn external_xpath_fn_descriptors_construct() {
    let _passthrough = wrap_xpath_fn!(test_passthrough);
    let _ctx = wrap_xpath_fn!(test_ctx_rename);
    let _interp = wrap_xpath_fn!(test_interp_rename);
    let _both = wrap_xpath_fn!(test_both_renamed);
}
