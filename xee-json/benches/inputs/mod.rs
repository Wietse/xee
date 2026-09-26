//! The benchmark's inputs, the two sides it times, and the guards that
//! check each input before it is timed.
//!
//! Defined once and shared by two includers: `benches/tokenizer.rs`, which
//! times the inputs, and `tests/bench_inputs.rs`, which runs every guard
//! under `cargo test`. A generator or guard that breaks therefore fails the
//! test suite, not only a bench run.
//!
//! Every input is generated once, deterministically, on first use, and is
//! about 1 MiB (the first value past [`TARGET_LEN`] bytes completes it). The
//! guards, run when an input is first used:
//!
//! - every input: xee-json and serde_json both accept it, and the xee-json
//!   drain ends without an error and before its bound ([`checked`]);
//! - [`ONE_PLAIN_STRING`]: it drains to exactly one [`Str::Plain`] string
//!   event, whose text is the input between its quotes, so the escape-free
//!   single-string case is not silently timing escapes;
//! - [`DECODE_STRINGS`]: the decode pair's two sides give equal vectors,
//!   no decoded string holds U+FFFD, and every string is [`Str::Escaped`];
//! - [`BORROW_STRINGS`]: the borrow pair's two sides give equal, non-empty
//!   vectors.
//!
//! A broken generator therefore fails instead of being timed as a fast
//! rejection.
//!
//! Every drain is bounded at `input.len() + 2` items (each event consumes at
//! least one byte), and every segment iteration at `raw.len() + 1` segments,
//! so a tokenizer that kept yielding would stop rather than grow without
//! limit.

#![allow(dead_code)]

use std::fmt::Write;
use std::hint::black_box;
use std::sync::LazyLock;

use serde::de::IgnoredAny;
use xee_json::{Decoded, Error, Event, Segment, Str, Tokenizer, DEFAULT_MAX_DEPTH};

/// Each generated input stops growing at the first value that takes it
/// past this many bytes: 1 MiB.
pub const TARGET_LEN: usize = 1 << 20;

// ============================================================================
// The two sides.
// ============================================================================

/// Drains the tokenizer, bounded at `input.len() + 2` items: the number of
/// events, or the first error.
pub fn xee_drain(input: &str) -> Result<usize, Error> {
    let mut events = 0;
    for item in Tokenizer::new(input).take(input.len() + 2) {
        black_box(item?);
        events += 1;
    }
    Ok(events)
}

pub fn serde_validate(input: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<IgnoredAny>(input).map(|_| ())
}

/// Decodes one string; a lone surrogate becomes U+FFFD.
pub fn decode(string: Str<'_>) -> String {
    match string {
        Str::Plain(text) => text.to_owned(),
        Str::Escaped(escaped) => {
            let raw = escaped.raw();
            let mut out = String::with_capacity(raw.len());
            for segment in escaped.segments().take(raw.len() + 1) {
                match segment {
                    Segment::Literal(text) => out.push_str(text),
                    Segment::Escape {
                        decoded: Decoded::Char(c),
                        ..
                    } => out.push(c),
                    Segment::Escape {
                        decoded: Decoded::LoneSurrogate(_),
                        ..
                    } => out.push(char::REPLACEMENT_CHARACTER),
                }
            }
            out
        }
    }
}

/// Every string value, decoded, in order.
pub fn xee_decode_strings(input: &str) -> Result<Vec<String>, Error> {
    let mut strings = Vec::new();
    for item in Tokenizer::new(input).take(input.len() + 2) {
        if let Event::String(string) = item? {
            strings.push(decode(string));
        }
    }
    Ok(strings)
}

/// Every escape-free string value, borrowed, in order. An escaped string
/// is left out; the [`BORROW_STRINGS`] guard checks the input has none.
pub fn xee_borrow_strings(input: &str) -> Result<Vec<&str>, Error> {
    let mut strings = Vec::new();
    for item in Tokenizer::new(input).take(input.len() + 2) {
        if let Event::String(Str::Plain(text)) = item? {
            strings.push(text);
        }
    }
    Ok(strings)
}

// ============================================================================
// Guards.
// ============================================================================

/// Fails unless both parsers accept `input` and the xee-json drain ends
/// before its bound; returns `input`.
pub fn checked(name: &str, input: String) -> String {
    let bound = input.len() + 2;
    match xee_drain(&input) {
        Ok(events) => assert!(
            events < bound,
            "{name}: {events} events for {} bytes: the drain reached its bound",
            input.len()
        ),
        Err(error) => panic!("{name}: xee-json rejects the input: {error}"),
    }
    if let Err(error) = serde_validate(&input) {
        panic!("{name}: serde_json rejects the input: {error}");
    }
    input
}

/// Fails unless `input` drains, within its bound, to exactly one item: a
/// [`Str::Plain`] string event whose text is `input` without its two
/// quotes.
pub fn assert_one_plain_string(name: &str, input: &str) {
    let bound = input.len() + 2;
    let items: Vec<_> = Tokenizer::new(input).take(bound).collect();
    assert!(
        items.len() < bound,
        "{name}: {} items for {} bytes: the drain reached its bound",
        items.len(),
        input.len()
    );
    assert_eq!(items.len(), 1, "{name}: not exactly one item");
    let inner = input
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'));
    // The messages leave out the text: it is about 1 MiB.
    match &items[0] {
        Ok(Event::String(Str::Plain(text))) => assert!(
            Some(*text) == inner,
            "{name}: the plain string is not the input between its quotes"
        ),
        Ok(Event::String(Str::Escaped(_))) => panic!("{name}: the string has an escape"),
        Ok(_) => panic!("{name}: the item is not a string"),
        Err(error) => panic!("{name}: {error}"),
    }
}

// ============================================================================
// Inputs.
// ============================================================================

/// Closes an array whose members were written as `"x,"`: drops the last
/// comma and appends `]`.
fn close_array(mut text: String) -> String {
    if text.ends_with(',') {
        text.pop();
    }
    text.push(']');
    text
}

/// At least `len` bytes of prose from ASCII words and a few multi-byte
/// characters, with no escape or quote. `between(index)` gives the text
/// written after the `index`th word.
fn prose(len: usize, mut between: impl FnMut(usize) -> String) -> String {
    let words = [
        "The quick brown fox jumps over the lazy dog".to_owned(),
        format!("gr{}{}e", char::from(0xFC_u8), char::from(0xDF_u8)),
        "Pack my box with five dozen liquor jugs".to_owned(),
        ['\u{4E16}', '\u{754C}'].iter().collect(),
    ];
    let mut text = String::new();
    let mut index = 0;
    while text.len() < len {
        text.push_str(&words[index % words.len()]);
        text.push_str(&between(index));
        index += 1;
    }
    text
}

/// `[0,-1.1,2e-2,0.3E+3,...]`: integers, fractions and exponents.
pub static NUMBERS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::from("[");
    let mut i = 0u64;
    while text.len() < TARGET_LEN {
        match i % 4 {
            0 => write!(text, "{i},"),
            1 => write!(text, "-{i}.{},", i % 1000),
            2 => write!(text, "{i}e-{},", i % 300),
            _ => write!(text, "0.{i}E+{},", i % 20),
        }
        .unwrap();
        i += 1;
    }
    checked("numbers", close_array(text))
});

/// `{"member0000000":0,"member0000001":"v1",...}`: many members with
/// distinct escape-free keys and scalar values.
pub static OBJECT: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::from("{");
    let mut i = 0u64;
    while text.len() < TARGET_LEN {
        write!(text, "\"member{i:07}\":").unwrap();
        match i % 5 {
            0 => write!(text, "{i},"),
            1 => write!(text, "\"v{i}\","),
            2 => write!(text, "true,"),
            3 => write!(text, "false,"),
            _ => write!(text, "null,"),
        }
        .unwrap();
        i += 1;
    }
    text.pop();
    text.push('}');
    checked("object", text)
});

/// An array of 4 KiB escape-free strings.
fn plain_strings() -> String {
    let mut text = String::from("[");
    while text.len() < TARGET_LEN {
        text.push('"');
        text.push_str(&prose(4096, |_| " ".to_owned()));
        text.push_str("\",");
    }
    close_array(text)
}

pub static PLAIN_STRINGS: LazyLock<String> =
    LazyLock::new(|| checked("plain_strings", plain_strings()));

/// A single escape-free string of about 1 MiB.
pub static ONE_PLAIN_STRING: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::from("\"");
    text.push_str(&prose(TARGET_LEN, |_| " ".to_owned()));
    text.push('"');
    let text = checked("one_plain_string", text);
    assert_one_plain_string("one_plain_string", &text);
    text
});

/// The escapes the escaped strings cycle through: every RFC 8259 §7 short
/// form, BMP `\u` escapes, and a surrogate pair. No lone surrogate. Built
/// from their parts so that no escape is written literally in this file.
fn escapes() -> Vec<String> {
    let backslash = '\\';
    let mut escapes: Vec<String> = ['"', '\\', '/', 'b', 'f', 'n', 'r', 't']
        .iter()
        .map(|c| format!("{backslash}{c}"))
        .collect();
    for unit in [0x00E9_u16, 0x4E16, 0x0001] {
        escapes.push(format!("{backslash}u{unit:04x}"));
    }
    escapes.push(format!(
        "{backslash}u{:04X}{backslash}u{:04X}",
        0xD83D, 0xDE00
    ));
    escapes
}

/// An array of 4 KiB strings with an escape after every word, about one
/// escape per 25 bytes.
fn escaped_strings() -> String {
    let escapes = escapes();
    let mut text = String::from("[");
    while text.len() < TARGET_LEN {
        text.push('"');
        text.push_str(&prose(4096, |index| escapes[index % escapes.len()].clone()));
        text.push_str("\",");
    }
    close_array(text)
}

pub static ESCAPED_STRINGS: LazyLock<String> =
    LazyLock::new(|| checked("escaped_strings", escaped_strings()));

/// The nesting of each element of [`DEEP`], an even number: with the outer
/// array, the text is one level under the default limit.
const DEEP_LEVELS: usize = DEFAULT_MAX_DEPTH - 2;

/// An array of values nested [`DEEP_LEVELS`] deep, alternating arrays and
/// single-member objects: `[[{"k":[{"k":...0...}]}],...]`.
pub static DEEP: LazyLock<String> = LazyLock::new(|| {
    let pairs = DEEP_LEVELS / 2;
    let open = "[{\"k\":".repeat(pairs);
    let close = "}]".repeat(pairs);
    let mut text = String::from("[");
    while text.len() < TARGET_LEN {
        text.push_str(&open);
        text.push('0');
        text.push_str(&close);
        text.push(',');
    }
    checked("deep", close_array(text))
});

/// [`ESCAPED_STRINGS`], for the decode pair: every string decoded into a
/// `Vec<String>` on both sides.
pub static DECODE_STRINGS: LazyLock<&'static str> = LazyLock::new(|| {
    let input = ESCAPED_STRINGS.as_str();
    let ours = xee_decode_strings(input).unwrap();
    let theirs = serde_json::from_str::<Vec<String>>(input).unwrap();
    assert_eq!(ours, theirs, "the decoded strings differ");
    assert!(ours
        .iter()
        .all(|s| !s.contains(char::REPLACEMENT_CHARACTER)));
    let escaped = Tokenizer::new(input)
        .take(input.len() + 2)
        .filter(|item| matches!(item, Ok(Event::String(Str::Escaped(_)))))
        .count();
    assert!(!ours.is_empty());
    assert_eq!(escaped, ours.len(), "a generated string has no escape");
    input
});

/// [`PLAIN_STRINGS`], for the borrow pair: every string borrowed into a
/// `Vec<&str>` on both sides.
pub static BORROW_STRINGS: LazyLock<&'static str> = LazyLock::new(|| {
    let input = PLAIN_STRINGS.as_str();
    let ours = xee_borrow_strings(input).unwrap();
    let theirs = serde_json::from_str::<Vec<&str>>(input).unwrap();
    assert!(!ours.is_empty());
    assert_eq!(ours, theirs, "the borrowed strings differ");
    input
});
