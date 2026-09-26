//! Runs the benchmark's input guards under `cargo test`.
//!
//! `benches/tokenizer.rs` times its inputs only after they pass their
//! guards, but `cargo test` does not run benches. This binary includes the
//! same module (generators, sides and guards, defined once) and forces each
//! input, so a generator or guard that breaks fails the test suite. Each
//! input is generated and checked once, on first use; the guards are
//! described in the module's own documentation.

#[path = "../benches/inputs/mod.rs"]
mod inputs;

use std::sync::LazyLock;

use inputs::{
    BORROW_STRINGS, DECODE_STRINGS, DEEP, ESCAPED_STRINGS, NUMBERS, OBJECT, ONE_PLAIN_STRING,
    PLAIN_STRINGS, TARGET_LEN,
};

/// Forces an input, so its guards run, and checks it reached the target
/// size.
fn force(input: &LazyLock<String>) {
    assert!(LazyLock::force(input).len() >= TARGET_LEN);
}

#[test]
fn numbers_pass_the_guards() {
    force(&NUMBERS);
}

#[test]
fn object_passes_the_guards() {
    force(&OBJECT);
}

#[test]
fn plain_strings_pass_the_guards() {
    force(&PLAIN_STRINGS);
}

/// Besides both parsers accepting it: exactly one `Str::Plain` string,
/// the input between its quotes.
#[test]
fn one_plain_string_is_one_plain_string() {
    force(&ONE_PLAIN_STRING);
}

#[test]
fn escaped_strings_pass_the_guards() {
    force(&ESCAPED_STRINGS);
}

#[test]
fn deep_passes_the_guards() {
    force(&DEEP);
}

/// The decode pair agrees with serde_json's `Vec<String>`, with no U+FFFD
/// and every string `Str::Escaped`.
#[test]
fn decode_strings_agree_with_serde_json() {
    assert!(LazyLock::force(&DECODE_STRINGS).len() >= TARGET_LEN);
}

/// The borrow pair agrees with serde_json's `Vec<&str>`.
#[test]
fn borrow_strings_agree_with_serde_json() {
    assert!(LazyLock::force(&BORROW_STRINGS).len() >= TARGET_LEN);
}
