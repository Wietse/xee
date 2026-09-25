//! The string scan skips a run of ordinary bytes eight at a time. These
//! tests put each byte that ends or changes the scan (RFC 8259 §7: the
//! closing quote, the backslash of an escape, an unescaped control
//! character), and each kind that does not, at every position of the first
//! words of a string, so a word test that missed one would show as a
//! different event or error offset.

mod common;

use common::{accept, reject};
use xee_json::{ErrorKind, Event, Str};

/// Positions 0 to 19 after the opening quote: every byte lane of the first
/// two words, and a partial third word handled byte by byte.
const POSITIONS: std::ops::Range<usize> = 0..20;

/// `len` bytes of ordinary ASCII, cycling through letters and digits.
fn filler(len: usize) -> String {
    "abcdefghijklmnopqrstuvwxyz0123456789"
        .chars()
        .cycle()
        .take(len)
        .collect()
}

#[test]
fn a_control_character_is_found_at_every_position() {
    for control in 0x00u8..0x20 {
        for before in POSITIONS {
            let input = format!(
                "\"{}{}{}\"",
                filler(before),
                char::from(control),
                filler(20)
            );
            let error = reject(&input);
            assert_eq!(
                (error.kind(), error.offset()),
                (ErrorKind::ControlCharInString, 1 + before),
                "control {control:#04x} after {before} bytes"
            );
        }
    }
}

#[test]
fn the_closing_quote_is_found_at_every_position() {
    for before in POSITIONS {
        let first = filler(before);
        let input = format!("[\"{first}\",\"{}\"]", filler(20));
        assert_eq!(
            accept(&input).get(1),
            Some(&Event::String(Str::Plain(&first))),
            "quote after {before} bytes"
        );
    }
}

#[test]
fn a_backslash_is_found_at_every_position() {
    for before in POSITIONS {
        let raw = format!("{}{}n{}", filler(before), '\\', filler(20));
        let input = format!("\"{raw}\"");
        match accept(&input).as_slice() {
            [Event::String(Str::Escaped(escaped))] => {
                assert_eq!(escaped.raw(), raw, "backslash after {before} bytes")
            }
            other => panic!("backslash after {before} bytes: {other:?}"),
        }
    }
}

#[test]
fn an_invalid_escape_is_reported_at_every_position() {
    for before in POSITIONS {
        let input = format!("\"{}{}x{}\"", filler(before), '\\', filler(20));
        let error = reject(&input);
        assert_eq!(
            (error.kind(), error.offset()),
            (ErrorKind::InvalidEscape, 2 + before),
            "escape after {before} bytes"
        );
    }
}

#[test]
fn ordinary_characters_are_skipped_at_every_position() {
    let ascii = (0x20u8..0x80)
        .map(char::from)
        .filter(|&c| c != '"' && c != '\\');
    // Two-, three- and four-byte characters: every byte is 0x80 or above.
    let multi_byte = [0xE9, 0x4E16, 0xFEFF, 0x1F600]
        .into_iter()
        .filter_map(char::from_u32);
    for c in ascii.chain(multi_byte) {
        for before in POSITIONS {
            let raw = format!("{}{c}{}", filler(before), filler(20));
            let input = format!("\"{raw}\"");
            assert_eq!(
                accept(&input),
                [Event::String(Str::Plain(&raw))],
                "{c:?} after {before} bytes"
            );
        }
    }
}

#[test]
fn an_unclosed_string_ends_at_the_end_of_the_input() {
    for len in POSITIONS {
        let input = format!("\"{}", filler(len));
        let error = reject(&input);
        assert_eq!(
            (error.kind(), error.offset()),
            (ErrorKind::UnexpectedEof, input.len()),
            "{len} bytes"
        );
    }
}

#[test]
fn the_scan_resumes_after_an_escape() {
    // A control character after an escape and a run of ordinary bytes, at
    // every position of the run: the scan restarted after the escape must
    // stop at it too.
    for before in POSITIONS {
        let input = format!("\"ab{}t{}{}\"", '\\', filler(before), '\u{1}');
        let error = reject(&input);
        assert_eq!(
            (error.kind(), error.offset()),
            (ErrorKind::ControlCharInString, 5 + before),
            "control after an escape and {before} bytes"
        );
    }
}
