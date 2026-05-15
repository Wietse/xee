//! External-crate integration test for the extension-function
//! registry.
//!
//! Integration tests under `tests/` compile as a separate crate that
//! depends on `xee-interpreter`, so this exercises the registration
//! round-trip the way a real host crate (e.g. an XBRL formula engine)
//! would: author a function with `#[xpath_fn]`, wrap it with
//! `wrap_xpath_fn!`, register it on a `StaticContextBuilder`, and
//! `build()`.
//!
//! Extension functions name themselves with `Q{uri}local` notation —
//! the braced URI is a literal, so the macro parses the signature
//! against the standard namespaces at expansion time without the host
//! registering anything. The registration logic itself (id offsets,
//! name resolution, conflict detection) is covered by the in-crate
//! unit tests in `static_context_builder.rs`; this test's distinct
//! job is to prove the whole path *compiles and runs from outside the
//! crate*.
//!
//! Arguments are `&Sequence` to keep the focus on registration rather
//! than on the atomic-type conversion paths.

use xee_interpreter::{context::StaticContextBuilder, sequence::Sequence, wrap_xpath_fn};
use xee_xpath_macros::xpath_fn;

#[xpath_fn("Q{http://www.xbrl.org/2008/function/instance}identity($s as item()*) as item()*")]
fn xfi_identity(s: &Sequence) -> Sequence {
    s.clone()
}

#[xpath_fn("Q{http://www.xbrl.org/2008/function/instance}passthrough($s as item()*) as item()*")]
fn xfi_passthrough(s: &Sequence) -> Sequence {
    s.clone()
}

#[test]
fn q_notation_extension_registers_from_external_crate() {
    let mut builder = StaticContextBuilder::default();
    builder.add_function(wrap_xpath_fn!(xfi_identity));
    // No `add_namespace` call: a `Q{uri}` signature needs no namespace
    // setup on the host side.
    assert!(
        builder.build().is_ok(),
        "a Q{{uri}}local extension should register and build cleanly"
    );
}

#[test]
fn multiple_q_notation_extensions_register_from_external_crate() {
    let mut builder = StaticContextBuilder::default();
    builder.add_functions([
        wrap_xpath_fn!(xfi_identity),
        wrap_xpath_fn!(xfi_passthrough),
    ]);
    assert!(builder.build().is_ok());
}
