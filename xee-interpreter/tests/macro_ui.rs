//! Trybuild-based UI tests for `#[xpath_fn]`.
//!
//! Each fixture under `tests/ui/pass/` is compiled by rustc and must
//! succeed. Each fixture under `tests/ui/fail/` must fail with the
//! exact error captured in the corresponding `.stderr` file.
//!
//! This gives us *real compile-time* validation of macro output —
//! something snapshot tests of token strings cannot offer. The
//! `external_xpath_fn` integration test next to this one covers the
//! same compile-success ground from inside a single binary; the UI
//! tests add isolated per-fixture builds plus the compile-fail
//! cases.
//!
//! To regenerate the `.stderr` files after intentional changes to
//! error messages, run with `TRYBUILD=overwrite`:
//!
//!     TRYBUILD=overwrite cargo test -p xee-interpreter --test macro_ui

#[test]
fn macro_ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
    t.compile_fail("tests/ui/fail/*.rs");
}
