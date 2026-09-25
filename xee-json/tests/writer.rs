//! Writer unit tests: one per Serialization 3.1 §9 escaping rule, the
//! "is encodable" predicate, verbatim text, ordered members, numbers,
//! misuse and layout.
//!
//! Expected escapes are built with `esc`, never written out as escape text
//! in the source.

mod common;

use xee_json::{EscapeProfile, Event, Layout, Str, WriteError, Writer};

use common::{accept, decode_string};

const PROFILE: EscapeProfile = EscapeProfile::Serialization;

/// The six-character escape with the given hexadecimal digits.
fn esc(hex: &str) -> String {
    format!("{}u{}", '\\', hex)
}

/// The six-character escape of a code unit, upper-case hex.
fn esc_unit(unit: u32) -> String {
    esc(&format!("{unit:04X}"))
}

/// The two-character escape ending in `letter`.
fn short(letter: char) -> String {
    format!("{}{}", '\\', letter)
}

fn compact() -> Writer<'static> {
    Writer::new(Layout::Compact, PROFILE)
}

fn indented() -> Writer<'static> {
    Writer::new(Layout::Indented, PROFILE)
}

/// `text` written as a top-level string by a compact writer.
fn written(text: &str) -> String {
    let mut writer = compact();
    writer.string(text).unwrap();
    writer.finish().unwrap()
}

/// `text` written as a top-level string with the given predicate.
fn written_with(text: &str, encodable: impl Fn(char) -> bool) -> String {
    let mut writer = compact().with_encodable(encodable);
    writer.string(text).unwrap();
    writer.finish().unwrap()
}

fn quoted(inner: &str) -> String {
    format!("\"{inner}\"")
}

/// Checks that `c` is written as the two-character escape ending in
/// `letter`, alone and between other characters, in a value and in a key.
fn assert_short_escape(c: char, letter: char) {
    assert_eq!(written(&c.to_string()), quoted(&short(letter)), "{c:?}");
    assert_eq!(
        written(&format!("a{c}b")),
        quoted(&format!("a{}b", short(letter))),
        "{c:?}"
    );
    let mut writer = compact();
    writer.start_object().unwrap();
    writer.key(&c.to_string()).unwrap();
    writer.null().unwrap();
    writer.end_object().unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        format!("{{\"{}\":null}}", short(letter)),
        "{c:?} in a key"
    );
}

// The eight two-character escapes of §9.

#[test]
fn quotation_mark_is_escaped() {
    assert_short_escape('"', '"');
}

#[test]
fn reverse_solidus_is_escaped() {
    assert_short_escape('\\', '\\');
}

#[test]
fn solidus_is_escaped() {
    assert_short_escape('/', '/');
}

#[test]
fn backspace_is_escaped() {
    assert_short_escape('\u{8}', 'b');
}

#[test]
fn form_feed_is_escaped() {
    assert_short_escape('\u{C}', 'f');
}

#[test]
fn newline_is_escaped() {
    assert_short_escape('\n', 'n');
}

#[test]
fn carriage_return_is_escaped() {
    assert_short_escape('\r', 'r');
}

#[test]
fn tab_is_escaped() {
    assert_short_escape('\t', 't');
}

// The six-character escapes.

/// §9: "any other codepoint in the range 1-31".
#[test]
fn other_c0_controls_are_six_character_escapes() {
    for cp in 0x01..=0x1F {
        if [0x08, 0x09, 0x0A, 0x0C, 0x0D].contains(&cp) {
            continue;
        }
        let c = char::from_u32(cp).unwrap();
        assert_eq!(written(&c.to_string()), quoted(&esc_unit(cp)), "U+{cp:04X}");
    }
}

/// U+0000 is outside §9's 1-31, but RFC 8259 §7 requires it escaped.
#[test]
fn nul_is_a_six_character_escape() {
    assert_eq!(written("\u{0}"), quoted(&esc("0000")));
    assert_eq!(written("a\u{0}b"), quoted(&format!("a{}b", esc("0000"))));
}

/// §9: "any other codepoint in the range ... 127-159".
#[test]
fn delete_and_c1_controls_are_six_character_escapes() {
    for cp in 0x7F..=0x9F {
        let c = char::from_u32(cp).unwrap();
        assert_eq!(written(&c.to_string()), quoted(&esc_unit(cp)), "U+{cp:04X}");
    }
}

/// The documented hex case: upper-case.
#[test]
fn hex_digits_are_upper_case() {
    assert_eq!(written("\u{1F}"), quoted(&esc("001F")));
    assert_eq!(written("\u{7F}"), quoted(&esc("007F")));
    assert_eq!(written("\u{9F}"), quoted(&esc("009F")));
    assert_eq!(
        written_with("\u{FEFF}\u{ABCD}", |c| c.is_ascii()),
        quoted(&format!("{}{}", esc("FEFF"), esc("ABCD")))
    );
    let out = written("\u{1A}\u{1B}\u{1C}\u{1D}\u{1E}\u{1F}\u{8A}\u{9B}");
    assert!(!out.contains(['a', 'b', 'c', 'd', 'e', 'f']), "{out:?}");
}

/// The characters just outside each escaped range are written as
/// themselves.
#[test]
fn range_boundaries_are_literal() {
    assert_eq!(written(" "), quoted(" "));
    assert_eq!(written("~"), quoted("~"));
    assert_eq!(written("\u{A0}"), quoted("\u{A0}"));
    assert_eq!(
        written("\u{1F} ~\u{7F}\u{9F}\u{A0}"),
        quoted(&format!(
            "{} ~{}{}\u{A0}",
            esc("001F"),
            esc("007F"),
            esc("009F")
        ))
    );
}

/// Every printable ASCII character other than the three with a
/// two-character escape is written as itself.
#[test]
fn printable_ascii_is_literal() {
    for c in (0x20u8..=0x7E).map(char::from) {
        if matches!(c, '"' | '\\' | '/') {
            continue;
        }
        assert_eq!(written(&c.to_string()), quoted(&c.to_string()), "{c:?}");
    }
}

#[test]
fn line_and_paragraph_separators_are_literal() {
    assert_eq!(written("\u{2028}\u{2029}"), quoted("\u{2028}\u{2029}"));
}

#[test]
fn non_bmp_is_literal_when_encodable() {
    assert_eq!(written("\u{1D11E}"), quoted("\u{1D11E}"));
    assert_eq!(written("\u{10FFFF}"), quoted("\u{10FFFF}"));
    let mut writer = compact().with_encodable(|_| true);
    writer.string("\u{1D11E}\u{E9}").unwrap();
    assert_eq!(writer.finish().unwrap(), quoted("\u{1D11E}\u{E9}"));
}

#[test]
fn empty_string() {
    assert_eq!(written(""), "\"\"");
}

// The "is encodable" predicate.

#[test]
fn a_rejected_bmp_character_is_a_six_character_escape() {
    let out = written_with("caf\u{E9} \u{2028}", |c| c != '\u{E9}');
    assert_eq!(out, quoted(&format!("caf{} \u{2028}", esc("00E9"))));
}

/// QT3 serialize-json-114: U+1D11E under ISO-8859-1.
#[test]
fn a_rejected_non_bmp_character_is_a_surrogate_pair() {
    let latin1 = |c: char| u32::from(c) <= 0xFF;
    assert_eq!(
        written_with("\u{1D11E}", latin1),
        quoted(&format!("{}{}", esc("D834"), esc("DD1E")))
    );
    assert_eq!(
        written_with("\u{10000}\u{10FFFF}", latin1),
        quoted(&format!(
            "{}{}{}{}",
            esc("D800"),
            esc("DC00"),
            esc("DBFF"),
            esc("DFFF")
        ))
    );
}

/// The predicate is asked about every character the profile would write
/// as itself, ASCII included.
#[test]
fn a_rejected_ascii_character_is_a_six_character_escape() {
    assert_eq!(
        written_with("bab", |c| c != 'a'),
        quoted(&format!("b{}b", esc("0061")))
    );
}

/// The two-character escapes do not depend on the predicate.
#[test]
fn short_escapes_do_not_consult_the_predicate() {
    assert_eq!(
        written_with("\"/\n", |_| false),
        quoted(&format!("{}{}{}", short('"'), short('/'), short('n')))
    );
}

#[test]
fn the_predicate_applies_to_keys() {
    let mut writer = compact().with_encodable(|c| c.is_ascii());
    writer.start_object().unwrap();
    writer.key("\u{E9}").unwrap();
    writer.string("\u{E9}").unwrap();
    writer.end_object().unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        format!("{{\"{0}\":\"{0}\"}}", esc("00E9"))
    );
}

/// Every character class through an ASCII-only predicate: the output is
/// pure ASCII and reads back as the input.
#[test]
fn an_ascii_only_predicate_gives_ascii_output() {
    let mut input = String::new();
    for cp in (0x00..=0x2FF).chain([0x2028, 0x2029, 0xD7FF, 0xE000, 0xFFFD, 0xFFFF]) {
        input.push(char::from_u32(cp).unwrap());
    }
    input.push_str("\u{10000}\u{1D11E}\u{10FFFF}");
    let out = written_with(&input, |c| c.is_ascii());
    assert!(out.is_ascii(), "{out:?}");
    match accept(&out).as_slice() {
        [Event::String(s)] => assert_eq!(decode_string(s).as_deref(), Some(input.as_str())),
        events => panic!("{events:?}"),
    }
}

// Verbatim text.

#[test]
fn verbatim_text_is_written_as_given_between_escaped_text() {
    let mut writer = compact().with_encodable(|c| c.is_ascii());
    writer.begin_string().unwrap();
    writer.string_text("a/").unwrap();
    writer.string_verbatim("/\u{E9}&amp;").unwrap();
    writer.string_text("\u{E9}\"").unwrap();
    writer.string_verbatim("").unwrap();
    writer.string_text("").unwrap();
    writer.string_verbatim("/").unwrap();
    writer.end_string().unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        quoted(&format!(
            "a{}/\u{E9}&amp;{}{}/",
            short('/'),
            esc("00E9"),
            short('"')
        ))
    );
}

/// A verbatim quotation mark is not escaped, even though the result is
/// then not valid JSON: verbatim text is outside the writer's guarantee.
#[test]
fn a_verbatim_quotation_mark_is_not_escaped() {
    let mut writer = compact();
    writer.begin_string().unwrap();
    writer.string_verbatim("\"").unwrap();
    writer.end_string().unwrap();
    assert_eq!(writer.finish().unwrap(), "\"\"\"");
}

#[test]
fn verbatim_text_in_a_key() {
    let mut writer = compact();
    writer.start_object().unwrap();
    writer.begin_key().unwrap();
    writer.string_text("/").unwrap();
    writer.string_verbatim("/").unwrap();
    writer.end_string().unwrap();
    writer.number("1").unwrap();
    writer.end_object().unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        format!("{{\"{}/\":1}}", short('/'))
    );
}

// Ordered members.

#[test]
fn repeated_keys_are_all_written_in_order() {
    let mut writer = compact();
    writer.start_object().unwrap();
    writer.key("a").unwrap();
    writer.number("1").unwrap();
    writer.key("b").unwrap();
    writer.number("2").unwrap();
    writer.key("a").unwrap();
    writer.number("3").unwrap();
    writer.key("a").unwrap();
    writer.start_array().unwrap();
    writer.end_array().unwrap();
    writer.end_object().unwrap();
    let out = writer.finish().unwrap();
    assert_eq!(out, r#"{"a":1,"b":2,"a":3,"a":[]}"#);
    assert_eq!(
        accept(&out),
        [
            Event::StartObject,
            Event::Key(Str::Plain("a")),
            Event::Number("1"),
            Event::Key(Str::Plain("b")),
            Event::Number("2"),
            Event::Key(Str::Plain("a")),
            Event::Number("3"),
            Event::Key(Str::Plain("a")),
            Event::StartArray,
            Event::EndArray,
            Event::EndObject,
        ]
    );
}

// Numbers.

#[test]
fn number_text_is_written_as_given() {
    for text in [
        "0",
        "-0",
        "1",
        "-1",
        "10",
        "1.50",
        "1.0E3",
        "1E400",
        "1e-7",
        "1E+2",
        "-0.0e0",
        "123456789012345678901234567890",
    ] {
        let mut writer = compact();
        writer.number(text).unwrap();
        assert_eq!(writer.finish().unwrap(), text);
    }
    let mut writer = compact();
    writer.start_array().unwrap();
    writer.number("-0").unwrap();
    writer.number("1.50").unwrap();
    writer.end_array().unwrap();
    assert_eq!(writer.finish().unwrap(), "[-0,1.50]");
}

#[test]
fn text_that_is_not_a_number_is_refused_and_nothing_is_written() {
    for text in [
        "01", "-01", "00", "1.", ".5", "+1", "-", "1e", "1e+", "1E-", "--1", "1.5.5", "0x1",
        "1e5.0", "NaN", "INF", "-INF", "Infinity", " 1", "1 ", "\t1", "1\n", "", "1,2", "\u{661}",
        "\u{FF11}",
    ] {
        let mut writer = compact();
        writer.start_array().unwrap();
        assert_eq!(
            writer.number(text),
            Err(WriteError::InvalidNumber),
            "{text:?}"
        );
        writer.number("2").unwrap();
        assert_eq!(
            writer.number(text),
            Err(WriteError::InvalidNumber),
            "{text:?}"
        );
        writer.end_array().unwrap();
        assert_eq!(writer.finish().unwrap(), "[2]", "{text:?}");

        let mut writer = compact();
        assert_eq!(
            writer.number(text),
            Err(WriteError::InvalidNumber),
            "{text:?}"
        );
        assert_eq!(writer.finish(), Err(WriteError::Incomplete), "{text:?}");
    }
}

// Misuse.

#[test]
fn a_value_in_an_object_without_a_key_is_refused() {
    let mut writer = compact();
    writer.start_object().unwrap();
    assert_eq!(writer.null(), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.bool(true), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.number("1"), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.string("x"), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.begin_string(), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.start_array(), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.start_object(), Err(WriteError::ValueWithoutKey));
    writer.key("k").unwrap();
    writer.null().unwrap();
    // A second value after the member's value.
    assert_eq!(writer.null(), Err(WriteError::ValueWithoutKey));
    assert_eq!(writer.start_array(), Err(WriteError::ValueWithoutKey));
    writer.end_object().unwrap();
    assert_eq!(writer.finish().unwrap(), r#"{"k":null}"#);
}

#[test]
fn a_key_outside_an_object_or_after_a_key_is_refused() {
    let mut writer = compact();
    assert_eq!(writer.key("top"), Err(WriteError::UnexpectedKey));
    assert_eq!(writer.begin_key(), Err(WriteError::UnexpectedKey));
    writer.start_array().unwrap();
    assert_eq!(writer.key("in array"), Err(WriteError::UnexpectedKey));
    writer.start_object().unwrap();
    writer.key("a").unwrap();
    assert_eq!(writer.key("b"), Err(WriteError::UnexpectedKey));
    assert_eq!(writer.begin_key(), Err(WriteError::UnexpectedKey));
    writer.number("1").unwrap();
    writer.end_object().unwrap();
    assert_eq!(writer.key("in array"), Err(WriteError::UnexpectedKey));
    writer.end_array().unwrap();
    assert_eq!(writer.key("after the end"), Err(WriteError::UnexpectedKey));
    assert_eq!(writer.finish().unwrap(), r#"[{"a":1}]"#);
}

#[test]
fn a_close_that_does_not_match_is_refused() {
    let mut writer = compact();
    assert_eq!(writer.end_array(), Err(WriteError::MismatchedClose));
    assert_eq!(writer.end_object(), Err(WriteError::MismatchedClose));
    writer.start_array().unwrap();
    assert_eq!(writer.end_object(), Err(WriteError::MismatchedClose));
    writer.start_object().unwrap();
    assert_eq!(writer.end_array(), Err(WriteError::MismatchedClose));
    writer.end_object().unwrap();
    writer.end_array().unwrap();
    assert_eq!(writer.end_array(), Err(WriteError::MismatchedClose));
    assert_eq!(writer.end_object(), Err(WriteError::MismatchedClose));
    assert_eq!(writer.finish().unwrap(), "[{}]");
}

#[test]
fn closing_an_object_after_a_key_is_refused() {
    let mut writer = compact();
    writer.start_object().unwrap();
    writer.key("a").unwrap();
    assert_eq!(writer.end_object(), Err(WriteError::KeyWithoutValue));
    assert_eq!(writer.end_array(), Err(WriteError::MismatchedClose));
    writer.bool(false).unwrap();
    writer.end_object().unwrap();
    assert_eq!(writer.finish().unwrap(), r#"{"a":false}"#);
}

#[test]
fn a_second_top_level_value_is_refused() {
    let mut writer = compact();
    writer.null().unwrap();
    assert_eq!(writer.null(), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.number("1"), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.string("x"), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.begin_string(), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.start_array(), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.finish().unwrap(), "null");

    let mut writer = compact();
    writer.start_array().unwrap();
    writer.end_array().unwrap();
    assert_eq!(writer.start_object(), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.bool(true), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.finish().unwrap(), "[]");

    let mut writer = compact();
    writer.string("a").unwrap();
    assert_eq!(writer.string("b"), Err(WriteError::SecondTopLevelValue));
    assert_eq!(writer.finish().unwrap(), "\"a\"");
}

#[test]
fn finishing_an_incomplete_text_is_refused() {
    assert_eq!(compact().finish(), Err(WriteError::Incomplete));

    let mut writer = compact();
    writer.start_array().unwrap();
    assert_eq!(writer.finish(), Err(WriteError::Incomplete));

    let mut writer = compact();
    writer.start_array().unwrap();
    writer.start_object().unwrap();
    writer.end_object().unwrap();
    assert_eq!(writer.finish(), Err(WriteError::Incomplete));

    let mut writer = compact();
    writer.start_object().unwrap();
    writer.key("a").unwrap();
    assert_eq!(writer.finish(), Err(WriteError::Incomplete));

    let mut writer = compact();
    writer.begin_string().unwrap();
    assert_eq!(writer.finish(), Err(WriteError::Incomplete));

    let mut writer = compact();
    writer.start_object().unwrap();
    writer.begin_key().unwrap();
    assert_eq!(writer.finish(), Err(WriteError::Incomplete));
}

#[test]
fn string_parts_outside_a_string_are_refused() {
    let mut writer = compact();
    assert_eq!(writer.string_text("a"), Err(WriteError::NoStringOpen));
    assert_eq!(writer.string_verbatim("a"), Err(WriteError::NoStringOpen));
    assert_eq!(writer.end_string(), Err(WriteError::NoStringOpen));
    writer.start_array().unwrap();
    writer.string("x").unwrap();
    assert_eq!(writer.string_text("a"), Err(WriteError::NoStringOpen));
    assert_eq!(writer.string_verbatim("a"), Err(WriteError::NoStringOpen));
    assert_eq!(writer.end_string(), Err(WriteError::NoStringOpen));
    writer.end_array().unwrap();
    assert_eq!(writer.end_string(), Err(WriteError::NoStringOpen));
    assert_eq!(writer.finish().unwrap(), r#"["x"]"#);
}

#[test]
fn other_calls_inside_an_open_string_are_refused() {
    for role in ["value", "key"] {
        let mut writer = compact();
        writer.start_object().unwrap();
        if role == "key" {
            writer.begin_key().unwrap();
        } else {
            writer.key("k").unwrap();
            writer.begin_string().unwrap();
        }
        writer.string_text("a").unwrap();
        assert_eq!(writer.null(), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.bool(true), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.number("1"), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.string("b"), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.key("b"), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.begin_string(), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.begin_key(), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.start_array(), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.start_object(), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.end_array(), Err(WriteError::StringOpen), "{role}");
        assert_eq!(writer.end_object(), Err(WriteError::StringOpen), "{role}");
        writer.string_text("c").unwrap();
        writer.end_string().unwrap();
        if role == "key" {
            writer.null().unwrap();
            writer.end_object().unwrap();
            assert_eq!(writer.finish().unwrap(), r#"{"ac":null}"#);
        } else {
            writer.end_object().unwrap();
            assert_eq!(writer.finish().unwrap(), r#"{"k":"ac"}"#);
        }
    }
}

#[test]
fn errors_display_a_description() {
    for error in [
        WriteError::InvalidNumber,
        WriteError::ValueWithoutKey,
        WriteError::UnexpectedKey,
        WriteError::KeyWithoutValue,
        WriteError::MismatchedClose,
        WriteError::SecondTopLevelValue,
        WriteError::StringOpen,
        WriteError::NoStringOpen,
        WriteError::Incomplete,
    ] {
        assert!(!error.to_string().is_empty(), "{error:?}");
    }
}

// Layout.

/// Writes the layout fixture: nested containers, empty ones, every scalar
/// and a repeated key.
fn write_fixture(writer: &mut Writer<'_>) {
    writer.start_object().unwrap();
    writer.key("a b").unwrap();
    writer.start_array().unwrap();
    writer.end_array().unwrap();
    writer.key("c").unwrap();
    writer.start_object().unwrap();
    writer.end_object().unwrap();
    writer.key("d").unwrap();
    writer.start_array().unwrap();
    writer.number("1").unwrap();
    writer.string(" x y ").unwrap();
    writer.start_object().unwrap();
    writer.key("e").unwrap();
    writer.null().unwrap();
    writer.key("e").unwrap();
    writer.bool(true).unwrap();
    writer.end_object().unwrap();
    writer.start_array().unwrap();
    writer.bool(false).unwrap();
    writer.end_array().unwrap();
    writer.end_array().unwrap();
    writer.key("f").unwrap();
    writer.number("-0").unwrap();
    writer.end_object().unwrap();
}

#[test]
fn compact_output_is_pinned() {
    let mut writer = compact();
    write_fixture(&mut writer);
    assert_eq!(
        writer.finish().unwrap(),
        r#"{"a b":[],"c":{},"d":[1," x y ",{"e":null,"e":true},[false]],"f":-0}"#
    );
}

/// §9.1.4: with `indent` no, no whitespace adjacent to structural tokens.
#[test]
fn compact_output_has_no_whitespace_outside_strings() {
    let mut writer = compact();
    write_fixture(&mut writer);
    let out = writer.finish().unwrap();
    assert_eq!(common::whitespace_outside_strings(&out), 0, "{out:?}");
}

#[test]
fn indented_output_is_pinned() {
    let mut writer = indented();
    write_fixture(&mut writer);
    let out = writer.finish().unwrap();
    let expected = [
        "{",
        r#"  "a b": [],"#,
        r#"  "c": {},"#,
        r#"  "d": ["#,
        "    1,",
        r#"    " x y ","#,
        "    {",
        r#"      "e": null,"#,
        r#"      "e": true"#,
        "    },",
        "    [",
        "      false",
        "    ]",
        "  ],",
        r#"  "f": -0"#,
        "}",
    ]
    .join("\n");
    assert_eq!(out, expected);
    let mut compact_writer = compact();
    write_fixture(&mut compact_writer);
    assert_eq!(accept(&out), accept(&compact_writer.finish().unwrap()));
}

#[test]
fn indented_top_level_values_have_no_surrounding_whitespace() {
    let mut writer = indented();
    writer.number("1").unwrap();
    assert_eq!(writer.finish().unwrap(), "1");
    let mut writer = indented();
    writer.string("a").unwrap();
    assert_eq!(writer.finish().unwrap(), "\"a\"");
    let mut writer = indented();
    writer.start_array().unwrap();
    writer.end_array().unwrap();
    assert_eq!(writer.finish().unwrap(), "[]");
    let mut writer = indented();
    writer.start_object().unwrap();
    writer.end_object().unwrap();
    assert_eq!(writer.finish().unwrap(), "{}");
}

#[test]
fn getters_read_back_the_construction() {
    let writer = indented();
    assert_eq!(writer.layout(), Layout::Indented);
    assert_eq!(writer.profile(), EscapeProfile::Serialization);
    assert_eq!(compact().layout(), Layout::Compact);
    assert_eq!(Layout::default(), Layout::Compact);
    assert_eq!(EscapeProfile::default(), EscapeProfile::Serialization);
}
