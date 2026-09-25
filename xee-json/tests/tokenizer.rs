//! Unit tests of the tokenizer: events, strings and escapes, surrogates,
//! numbers, whitespace, the byte order mark, error kinds and offsets, and
//! fusing.

mod common;

use std::iter::FusedIterator;

use common::{accept, check_segments, decode, items, parse_with, reject, reject_with, segments};
use xee_json::{BomPolicy, Decoded, ErrorKind, Escaped, Event, Options, Segment, Str, Tokenizer};

const IGNORE_BOM: Options = Options::new().with_bom_policy(BomPolicy::Ignore);

/// The single string value of `input`, which must be a JSON string.
fn string(input: &str) -> Str<'_> {
    match accept(input).as_slice() {
        [Event::String(s)] => {
            check_segments(s);
            *s
        }
        other => panic!("{input:?} gave {other:?}"),
    }
}

fn escaped(input: &str) -> Escaped<'_> {
    match string(input) {
        Str::Escaped(e) => e,
        Str::Plain(p) => panic!("{input:?} is plain: {p:?}"),
    }
}

fn escape(decoded: Decoded, source: &str) -> Segment<'_> {
    Segment::Escape { decoded, source }
}

fn error_at(input: &str, kind: ErrorKind, offset: usize) {
    let error = reject(input);
    assert_eq!((error.kind(), error.offset()), (kind, offset), "{input:?}");
}

fn error_at_with(input: &str, options: Options, kind: ErrorKind, offset: usize) {
    let error = reject_with(input, options);
    assert_eq!(
        (error.kind(), error.offset()),
        (kind, offset),
        "{input:?} {options:?}"
    );
}

// ---------------------------------------------------------------------------
// Fusing

#[test]
fn tokenizer_is_fused_after_an_error() {
    let mut tokenizer = Tokenizer::new("[1,]");
    let mut error = None;
    for _ in 0..10 {
        match tokenizer.next() {
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                error = Some(e);
                break;
            }
            None => panic!("ended without an error"),
        }
    }
    let error = error.expect("an error within 10 items");
    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::UnexpectedChar, 3)
    );
    for _ in 0..100 {
        assert_eq!(tokenizer.next(), None);
    }
}

#[test]
fn tokenizer_is_fused_after_the_end() {
    let mut tokenizer = Tokenizer::new("[1] ");
    let mut ended = false;
    for _ in 0..10 {
        match tokenizer.next() {
            Some(Ok(_)) => {}
            Some(Err(e)) => panic!("error {e}"),
            None => {
                ended = true;
                break;
            }
        }
    }
    assert!(ended, "no end within 10 items");
    for _ in 0..100 {
        assert_eq!(tokenizer.next(), None);
    }
}

#[test]
fn tokenizer_is_fused_after_an_error_at_the_start() {
    let mut tokenizer = Tokenizer::new("\u{FEFF}1");
    assert_eq!(
        tokenizer.next().map(|r| r.map_err(|e| e.kind())),
        Some(Err(ErrorKind::BomNotAllowed))
    );
    for _ in 0..100 {
        assert_eq!(tokenizer.next(), None);
    }
}

#[test]
fn tokenizer_implements_fused_iterator() {
    fn assert_fused<I: FusedIterator>(_: &I) {}
    assert_fused(&Tokenizer::new("1"));
    assert_fused(&escaped(r#""\n""#).segments());
}

// ---------------------------------------------------------------------------
// Events

#[test]
fn events_in_document_order() {
    assert_eq!(
        accept(r#"{"a":[1,{"b":null}],"c":true,"d":false,"e":"x"}"#),
        [
            Event::StartObject,
            Event::Key(Str::Plain("a")),
            Event::StartArray,
            Event::Number("1"),
            Event::StartObject,
            Event::Key(Str::Plain("b")),
            Event::Null,
            Event::EndObject,
            Event::EndArray,
            Event::Key(Str::Plain("c")),
            Event::Bool(true),
            Event::Key(Str::Plain("d")),
            Event::Bool(false),
            Event::Key(Str::Plain("e")),
            Event::String(Str::Plain("x")),
            Event::EndObject,
        ]
    );
}

#[test]
fn duplicate_keys_are_not_merged() {
    assert_eq!(
        accept(r#"{"a":1,"a":2,"a":1}"#),
        [
            Event::StartObject,
            Event::Key(Str::Plain("a")),
            Event::Number("1"),
            Event::Key(Str::Plain("a")),
            Event::Number("2"),
            Event::Key(Str::Plain("a")),
            Event::Number("1"),
            Event::EndObject,
        ]
    );
}

#[test]
fn empty_containers() {
    assert_eq!(accept("[]"), [Event::StartArray, Event::EndArray]);
    assert_eq!(accept("{}"), [Event::StartObject, Event::EndObject]);
    // Whitespace alone inside the brackets.
    for input in ["[ ]", "[\t\n\r ]"] {
        assert_eq!(
            accept(input),
            [Event::StartArray, Event::EndArray],
            "{input:?}"
        );
    }
    for input in ["{ }", "{\r\n}"] {
        assert_eq!(
            accept(input),
            [Event::StartObject, Event::EndObject],
            "{input:?}"
        );
    }
    assert_eq!(
        accept("[{},[]]"),
        [
            Event::StartArray,
            Event::StartObject,
            Event::EndObject,
            Event::StartArray,
            Event::EndArray,
            Event::EndArray,
        ]
    );
}

#[test]
fn top_level_scalars() {
    assert_eq!(accept("1"), [Event::Number("1")]);
    assert_eq!(accept("-0.5e3"), [Event::Number("-0.5e3")]);
    assert_eq!(accept(r#""s""#), [Event::String(Str::Plain("s"))]);
    assert_eq!(accept("true"), [Event::Bool(true)]);
    assert_eq!(accept("false"), [Event::Bool(false)]);
    assert_eq!(accept("null"), [Event::Null]);
    assert_eq!(accept(" 2 "), [Event::Number("2")]);
}

#[test]
fn empty_input_is_rejected() {
    error_at("", ErrorKind::UnexpectedEof, 0);
    error_at(" \t\r\n", ErrorKind::UnexpectedEof, 4);
}

#[test]
fn trailing_content_is_rejected_after_the_value_events() {
    let items = items("[1]x");
    assert_eq!(
        items[..3],
        [
            Ok(Event::StartArray),
            Ok(Event::Number("1")),
            Ok(Event::EndArray)
        ]
    );
    let error = items[3].expect_err("an error after the value");
    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::TrailingContent, 3)
    );
    assert_eq!(items.len(), 4);

    error_at("1 2", ErrorKind::TrailingContent, 2);
    error_at("[] []", ErrorKind::TrailingContent, 3);
    error_at("{} x", ErrorKind::TrailingContent, 3);
    error_at("[1]]", ErrorKind::TrailingContent, 3);
    error_at("null,", ErrorKind::TrailingContent, 4);
    error_at("\"a\"\"b\"", ErrorKind::TrailingContent, 3);
    // A number ends at the first byte that cannot continue it.
    error_at("0x1", ErrorKind::TrailingContent, 1);
    error_at("1.5.3", ErrorKind::TrailingContent, 3);
    error_at("123\u{0}", ErrorKind::TrailingContent, 3);
}

#[test]
fn structural_errors() {
    error_at("[1,]", ErrorKind::UnexpectedChar, 3);
    error_at(r#"{"a":1,}"#, ErrorKind::UnexpectedChar, 7);
    error_at(r#"{"a" 1}"#, ErrorKind::UnexpectedChar, 5);
    error_at(r#"{"a"}"#, ErrorKind::UnexpectedChar, 4);
    error_at(r#"{"a":}"#, ErrorKind::UnexpectedChar, 5);
    error_at("{1:2}", ErrorKind::UnexpectedChar, 1);
    error_at("[1:2]", ErrorKind::UnexpectedChar, 2);
    error_at("[1 2]", ErrorKind::UnexpectedChar, 3);
    error_at("[,1]", ErrorKind::UnexpectedChar, 1);
    error_at("]", ErrorKind::UnexpectedChar, 0);
    error_at("}", ErrorKind::UnexpectedChar, 0);
    error_at("[}", ErrorKind::UnexpectedChar, 1);
    error_at("{]", ErrorKind::UnexpectedChar, 1);
    error_at("[1}", ErrorKind::UnexpectedChar, 2);
    error_at(r#"{"a":1]"#, ErrorKind::UnexpectedChar, 6);
    error_at("'a'", ErrorKind::UnexpectedChar, 0);
    error_at("[1,,2]", ErrorKind::UnexpectedChar, 3);
    error_at("[1true]", ErrorKind::UnexpectedChar, 2);
}

#[test]
fn unexpected_eof() {
    for (input, offset) in [
        ("[", 1),
        ("[1", 2),
        ("[1,", 3),
        ("{", 1),
        (r#"{"a""#, 4),
        (r#"{"a":"#, 5),
        (r#"{"a":1"#, 6),
        (r#"{"a":1,"#, 7),
        (r#""abc"#, 4),
        (r#""a\"#, 3),
        (r#""\u12"#, 5),
        ("-", 1),
        ("1.", 2),
        ("1e", 2),
        ("1e+", 3),
        ("tru", 3),
        ("nul", 3),
        ("f", 1),
    ] {
        error_at(input, ErrorKind::UnexpectedEof, offset);
    }
}

#[test]
fn invalid_literals() {
    error_at("[tru]", ErrorKind::InvalidLiteral, 4);
    error_at("nulL", ErrorKind::InvalidLiteral, 3);
    error_at("fals e", ErrorKind::InvalidLiteral, 4);
    error_at("True", ErrorKind::UnexpectedChar, 0);
    error_at("NaN", ErrorKind::UnexpectedChar, 0);
    error_at("[truex]", ErrorKind::UnexpectedChar, 5);
}

// ---------------------------------------------------------------------------
// Strings

#[test]
fn strings_without_escapes_are_plain_slices() {
    let input = r#"["abc", "", "é€𝄞", "\u{7f}"]"#.replace("\\u{7f}", "\u{7f}");
    let events = accept(&input);
    assert_eq!(
        events[1..5],
        [
            Event::String(Str::Plain("abc")),
            Event::String(Str::Plain("")),
            Event::String(Str::Plain("é€𝄞")),
            Event::String(Str::Plain("\u{7f}")),
        ]
    );
    // Borrowed from the input, not copied.
    let Event::String(Str::Plain(abc)) = events[1] else {
        unreachable!()
    };
    assert_eq!(abc.as_ptr(), input[2..].as_ptr());
}

#[test]
fn raw_control_characters_are_rejected() {
    for c in '\u{0}'..='\u{1F}' {
        let input = format!("\"a{c}\"");
        error_at(&input, ErrorKind::ControlCharInString, 2);
        let key = format!("{{\"{c}\":1}}");
        error_at(&key, ErrorKind::ControlCharInString, 2);
    }
}

#[test]
fn other_raw_characters_are_accepted() {
    // DEL, C1 controls, noncharacters, a byte order mark, line separators.
    for c in [
        '\u{7F}',
        '\u{80}',
        '\u{9F}',
        '\u{FFFE}',
        '\u{FFFF}',
        '\u{FEFF}',
        '\u{2028}',
        '\u{10FFFF}',
    ] {
        let input = format!("\"{c}\"");
        assert_eq!(string(&input), Str::Plain(&input[1..input.len() - 1]));
    }
}

#[test]
fn every_simple_escape() {
    let e = escaped(r#""\"\\\/\b\f\n\r\t""#);
    assert_eq!(e.raw(), r#"\"\\\/\b\f\n\r\t"#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::Char('"'), r#"\""#),
            escape(Decoded::Char('\\'), r"\\"),
            escape(Decoded::Char('/'), r"\/"),
            escape(Decoded::Char('\u{8}'), r"\b"),
            escape(Decoded::Char('\u{c}'), r"\f"),
            escape(Decoded::Char('\n'), r"\n"),
            escape(Decoded::Char('\r'), r"\r"),
            escape(Decoded::Char('\t'), r"\t"),
        ]
    );
}

#[test]
fn unicode_escapes_in_either_case() {
    for (input, c) in [
        (r#""\u00e9""#, 'é'),
        (r#""\u00E9""#, 'é'),
        (r#""\uabCD""#, '\u{ABCD}'),
        (r#""\u0000""#, '\u{0}'),
        (r#""\u001f""#, '\u{1F}'),
        (r#""\uFFFF""#, '\u{FFFF}'),
        (r#""\u0041""#, 'A'),
    ] {
        let e = escaped(input);
        assert_eq!(
            segments(&Str::Escaped(e)),
            [escape(Decoded::Char(c), e.raw())],
            "{input}"
        );
    }
}

#[test]
fn literal_runs_between_escapes() {
    let e = escaped(r#""ab\ncd€\u0041""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            Segment::Literal("ab"),
            escape(Decoded::Char('\n'), r"\n"),
            Segment::Literal("cd€"),
            escape(Decoded::Char('A'), r"\u0041"),
        ]
    );
}

#[test]
fn invalid_escapes() {
    error_at(r#""\x""#, ErrorKind::InvalidEscape, 2);
    error_at(r#""\a""#, ErrorKind::InvalidEscape, 2);
    error_at(r#""\U0041""#, ErrorKind::InvalidEscape, 2);
    error_at(r#""\'""#, ErrorKind::InvalidEscape, 2);
    error_at(r#""\u12G4""#, ErrorKind::InvalidEscape, 5);
    error_at(r#""\u+123""#, ErrorKind::InvalidEscape, 3);
    error_at(r#""\u12""#, ErrorKind::InvalidEscape, 5);
    error_at("\"\\é\"", ErrorKind::InvalidEscape, 2);
    error_at("\"\\u00é0\"", ErrorKind::InvalidEscape, 5);
    // A raw control character after a backslash is an invalid escape.
    error_at("\"\\\n\"", ErrorKind::InvalidEscape, 2);
}

// ---------------------------------------------------------------------------
// Surrogates

#[test]
fn surrogate_pair_is_one_char_with_its_full_source() {
    for input in [r#""\uD834\uDD1E""#, r#""\ud834\udd1e""#] {
        let e = escaped(input);
        assert_eq!(
            segments(&Str::Escaped(e)),
            [escape(Decoded::Char('𝄞'), e.raw())],
            "{input}"
        );
        assert_eq!(e.raw().len(), 12);
    }
    let e = escaped(r#""\uD800\uDC00\uDBFF\uDFFF""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::Char('\u{10000}'), r"\uD800\uDC00"),
            escape(Decoded::Char('\u{10FFFF}'), r"\uDBFF\uDFFF"),
        ]
    );
}

#[test]
fn high_surrogate_followed_by_a_non_escape_is_lone() {
    let e = escaped(r#""\uD800abc""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::LoneSurrogate(0xD800), r"\uD800"),
            Segment::Literal("abc"),
        ]
    );
}

#[test]
fn high_surrogate_followed_by_another_escape_is_lone() {
    let e = escaped(r#""\uD800\n\uD800\u0041""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::LoneSurrogate(0xD800), r"\uD800"),
            escape(Decoded::Char('\n'), r"\n"),
            escape(Decoded::LoneSurrogate(0xD800), r"\uD800"),
            escape(Decoded::Char('A'), r"\u0041"),
        ]
    );
}

#[test]
fn high_surrogate_at_end_of_string_is_lone() {
    let e = escaped(r#""x\uDBFF""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            Segment::Literal("x"),
            escape(Decoded::LoneSurrogate(0xDBFF), r"\uDBFF"),
        ]
    );
}

#[test]
fn low_surrogate_alone_is_lone() {
    let e = escaped(r#""\uDC00\udfff""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::LoneSurrogate(0xDC00), r"\uDC00"),
            escape(Decoded::LoneSurrogate(0xDFFF), r"\udfff"),
        ]
    );
}

#[test]
fn reversed_surrogate_pair_is_two_lone_surrogates() {
    let e = escaped(r#""\uDD1E\uD834""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::LoneSurrogate(0xDD1E), r"\uDD1E"),
            escape(Decoded::LoneSurrogate(0xD834), r"\uD834"),
        ]
    );
}

#[test]
fn high_high_low_is_lone_then_a_pair() {
    let e = escaped(r#""\uD800\uD800\uDC00""#);
    assert_eq!(
        segments(&Str::Escaped(e)),
        [
            escape(Decoded::LoneSurrogate(0xD800), r"\uD800"),
            escape(Decoded::Char('\u{10000}'), r"\uD800\uDC00"),
        ]
    );
}

#[test]
fn high_surrogate_pairs_only_with_a_backslash_u_escape() {
    // Four low-surrogate hex digits two bytes after the high surrogate
    // complete a pair only when those two bytes are `\u`; after another
    // escape or literal text they are literal text.
    for (input, second, digits) in [
        (
            "\"\\uD800\\\\DC00\"",
            escape(Decoded::Char('\\'), r"\\"),
            "DC00",
        ),
        (
            "\"\\uDBFF\\/DFFF\"",
            escape(Decoded::Char('/'), r"\/"),
            "DFFF",
        ),
        (
            "\"\\uD800\\nDC00\"",
            escape(Decoded::Char('\n'), r"\n"),
            "DC00",
        ),
        (
            "\"\\uD800\\\"DC00\"",
            escape(Decoded::Char('"'), r#"\""#),
            "DC00",
        ),
    ] {
        let e = escaped(input);
        let high = &e.raw()[..6];
        let expected_high = if high == r"\uD800" { 0xD800 } else { 0xDBFF };
        assert_eq!(
            segments(&Str::Escaped(e)),
            [
                escape(Decoded::LoneSurrogate(expected_high), high),
                second,
                Segment::Literal(digits),
            ],
            "{input}"
        );
    }
    // Literal text in the gap: neither byte alone decides. `au` ends in the
    // `u` of `\u` but does not start with its backslash.
    for (input, rest) in [
        ("\"\\uD800xxDC00\"", "xxDC00"),
        ("\"\\uD800auDC00\"", "auDC00"),
    ] {
        assert_eq!(
            segments(&Str::Escaped(escaped(input))),
            [
                escape(Decoded::LoneSurrogate(0xD800), r"\uD800"),
                Segment::Literal(rest),
            ],
            "{input}"
        );
    }
}

#[test]
fn high_surrogate_followed_by_an_escape_above_the_low_range_is_lone() {
    // U+E000 and U+FFFF are just above the low surrogates (DC00-DFFF).
    for (input, next, next_source) in [
        ("\"\\uD800\\uE000\"", 0xE000, "\\uE000"),
        ("\"\\uDBFF\\uffff\"", 0xFFFF, "\\uffff"),
    ] {
        let e = escaped(input);
        let high = &e.raw()[..6];
        let expected_high = if high == r"\uD800" { 0xD800 } else { 0xDBFF };
        assert_eq!(
            segments(&Str::Escaped(e)),
            [
                escape(Decoded::LoneSurrogate(expected_high), high),
                escape(
                    Decoded::Char(char::from_u32(next).expect("a scalar value")),
                    next_source
                ),
            ],
            "{input}"
        );
    }
}

#[test]
fn lone_surrogate_in_a_key() {
    let events = accept(r#"{"\uDFAA":0}"#);
    let Event::Key(key) = events[1] else {
        panic!("{events:?}")
    };
    check_segments(&key);
    assert_eq!(decode(&key), [Err(0xDFAA)]);
    assert_eq!(key.raw(), r"\uDFAA");
}

#[test]
fn segment_invariants_over_unit_inputs() {
    let inputs = [
        r#""plain""#,
        r#""""#,
        r#""\"\\\/\b\f\n\r\t""#,
        r#""a\u00e9b\u00E9c""#,
        r#""\uD834\uDD1E and \ud834\udd1e""#,
        r#""\uD800abc""#,
        r#""\uD800\n""#,
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\uDD1E\uD834""#,
        r#""\uD800\uD800\uDC00""#,
        r#""\uDBFF\uDBFF\uDBFF""#,
        r#""€\u20AC€""#,
        r#""\\u0041""#,
        r#""\\\u0041""#,
        r#""\u005C\u0022""#,
        // A high surrogate, then two bytes that are not `\u`, then four
        // low-surrogate hex digits: lone, not a pair.
        "\"\\uD800\\\\DC00\"",
        "\"\\uDBFF\\/DFFF\"",
        "\"\\uD800\\nDC00\"",
        "\"\\uD800xxDC00\"",
        "\"\\uD800auDC00\"",
        "\"\\uD800\\\\uDC00\"",
        // A high surrogate, then an escape just above the low range.
        "\"\\uD800\\uE000\"",
        "\"\\uDBFF\\uFFFF\"",
    ];
    let mut checked = 0;
    for input in inputs {
        for s in common::strings(&accept(input)) {
            check_segments(&s);
            checked += 1;
        }
    }
    assert_eq!(checked, inputs.len());
    // An escaped backslash followed by `u0041` is literal text, not an escape.
    assert_eq!(
        common::decode_string(&string(r#""\\u0041""#)).as_deref(),
        Some(r"\u0041")
    );
    assert_eq!(
        common::decode_string(&string(r#""a\u00e9b\u00E9c""#)).as_deref(),
        Some("aébéc")
    );
}

// ---------------------------------------------------------------------------
// Numbers

#[test]
fn valid_numbers_are_delivered_as_text() {
    for input in [
        "0",
        "-0",
        "7",
        "-7",
        "10",
        "1234567890",
        "0.0",
        "-0.0",
        "1.5",
        "0.000001",
        "1e5",
        "1E5",
        "1e+5",
        "1e-5",
        "1E-0",
        "0e0",
        "-1.25e+10",
        "123e99999999999999999999999999",
        "1e-99999999999999999999999999",
        "99999999999999999999999999999999999999",
    ] {
        assert_eq!(accept(input), [Event::Number(input)], "{input}");
        let in_array = format!("[{input}]");
        assert_eq!(accept(&in_array)[1], Event::Number(input), "{in_array}");
    }
}

#[test]
fn invalid_numbers() {
    error_at("01", ErrorKind::InvalidNumber, 1);
    error_at("00", ErrorKind::InvalidNumber, 1);
    error_at("-01", ErrorKind::InvalidNumber, 2);
    error_at("[012]", ErrorKind::InvalidNumber, 2);
    error_at("-a", ErrorKind::InvalidNumber, 1);
    error_at("-.5", ErrorKind::InvalidNumber, 1);
    error_at("- 1", ErrorKind::InvalidNumber, 1);
    error_at("1.e5", ErrorKind::InvalidNumber, 2);
    error_at("[1.]", ErrorKind::InvalidNumber, 3);
    error_at("[1e]", ErrorKind::InvalidNumber, 3);
    error_at("1ea", ErrorKind::InvalidNumber, 2);
    error_at("1e+-1", ErrorKind::InvalidNumber, 3);
    error_at("-é", ErrorKind::InvalidNumber, 1);
    error_at("+1", ErrorKind::UnexpectedChar, 0);
    error_at(".5", ErrorKind::UnexpectedChar, 0);
    error_at("[+1]", ErrorKind::UnexpectedChar, 1);
    error_at("Infinity", ErrorKind::UnexpectedChar, 0);
    // A sign only leads the number or the exponent: anywhere else the
    // number has ended and the sign is what follows it.
    error_at("[1+]", ErrorKind::UnexpectedChar, 2);
    error_at("[1-1]", ErrorKind::UnexpectedChar, 2);
    error_at("[1.5+1]", ErrorKind::UnexpectedChar, 4);
    error_at("[1e5-1]", ErrorKind::UnexpectedChar, 4);
    error_at("[--1]", ErrorKind::InvalidNumber, 2);
    error_at("1+", ErrorKind::TrailingContent, 1);
}

// ---------------------------------------------------------------------------
// Whitespace

#[test]
fn whitespace_is_space_tab_lf_cr_only() {
    assert_eq!(
        accept(
            " \t\n\r[ \t\n\r1 \t\n\r, \t\n\r{ \t\n\r\"k\" \t\n\r: \t\n\r2 \t\n\r} \t\n\r] \t\n\r"
        ),
        [
            Event::StartArray,
            Event::Number("1"),
            Event::StartObject,
            Event::Key(Str::Plain("k")),
            Event::Number("2"),
            Event::EndObject,
            Event::EndArray,
        ]
    );
    for ws in [
        '\u{B}', '\u{C}', '\u{A0}', '\u{85}', '\u{2028}', '\u{3000}', '\u{0}',
    ] {
        error_at(&format!("{ws}1"), ErrorKind::UnexpectedChar, 0);
        error_at(&format!("[{ws}1]"), ErrorKind::UnexpectedChar, 1);
        error_at(&format!("1{ws}"), ErrorKind::TrailingContent, 1);
    }
}

// ---------------------------------------------------------------------------
// Byte order mark

#[test]
fn default_bom_policy_is_reject() {
    assert_eq!(Options::default().bom_policy(), BomPolicy::Reject);
    assert_eq!(Options::new(), Options::default());
}

#[test]
fn options_read_back_what_was_set() {
    let options = Options::new()
        .with_max_depth(7)
        .with_bom_policy(BomPolicy::Ignore);
    assert_eq!(
        (options.bom_policy(), options.max_depth()),
        (BomPolicy::Ignore, 7)
    );
    let options = options.with_bom_policy(BomPolicy::Reject);
    assert_eq!(
        (options.bom_policy(), options.max_depth()),
        (BomPolicy::Reject, 7)
    );
}

#[test]
fn leading_bom_is_rejected_under_reject() {
    let items = items("\u{FEFF}{}");
    assert_eq!(items.len(), 1, "no events before the error: {items:?}");
    let error = items[0].expect_err("rejected");
    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::BomNotAllowed, 0)
    );
    error_at("\u{FEFF}", ErrorKind::BomNotAllowed, 0);
}

#[test]
fn leading_bom_is_skipped_under_ignore() {
    assert_eq!(
        parse_with("\u{FEFF}{}", IGNORE_BOM),
        Ok(vec![Event::StartObject, Event::EndObject])
    );
    assert_eq!(
        parse_with("\u{FEFF} 1 ", IGNORE_BOM),
        Ok(vec![Event::Number("1")])
    );
}

#[test]
fn bom_followed_by_nothing_is_rejected_under_both_policies() {
    error_at_with("\u{FEFF}", IGNORE_BOM, ErrorKind::UnexpectedEof, 3);
    error_at_with("\u{FEFF} \n", IGNORE_BOM, ErrorKind::UnexpectedEof, 5);
    error_at_with("\u{FEFF}", Options::new(), ErrorKind::BomNotAllowed, 0);
    error_at_with("\u{FEFF} \n", Options::new(), ErrorKind::BomNotAllowed, 0);
}

#[test]
fn bom_not_at_offset_zero_is_not_skipped() {
    for options in [Options::new(), IGNORE_BOM] {
        error_at_with(" \u{FEFF}1", options, ErrorKind::UnexpectedChar, 1);
        error_at_with("[\u{FEFF}]", options, ErrorKind::UnexpectedChar, 1);
        error_at_with("1\u{FEFF}", options, ErrorKind::TrailingContent, 1);
    }
    // Only one leading mark is skipped.
    error_at_with(
        "\u{FEFF}\u{FEFF}1",
        IGNORE_BOM,
        ErrorKind::UnexpectedChar,
        3,
    );
    // Inside a string it is an ordinary character.
    assert_eq!(
        parse_with("\"\u{FEFF}\"", Options::new()),
        Ok(vec![Event::String(Str::Plain("\u{FEFF}"))])
    );
}

// ---------------------------------------------------------------------------
// Offsets

#[test]
fn offsets_count_bytes_after_multibyte_characters() {
    // "é" is two bytes, "€" three, "𝄞" four.
    error_at("[\"é\",]", ErrorKind::UnexpectedChar, 6);
    error_at("[\"€\" 1]", ErrorKind::UnexpectedChar, 7);
    error_at("{\"𝄞\":01}", ErrorKind::InvalidNumber, 9);
    error_at("\"é\\x\"", ErrorKind::InvalidEscape, 4);
    error_at("\"é\u{1}\"", ErrorKind::ControlCharInString, 3);
    error_at("\"€\" x", ErrorKind::TrailingContent, 6);
    error_at("[\"é\", tru", ErrorKind::UnexpectedEof, 10);
    error_at("é", ErrorKind::UnexpectedChar, 0);
    error_at("[1,é]", ErrorKind::UnexpectedChar, 3);
}

#[test]
fn offsets_count_the_skipped_bom() {
    error_at_with("\u{FEFF}[1,]", IGNORE_BOM, ErrorKind::UnexpectedChar, 6);
    error_at_with("\u{FEFF}01", IGNORE_BOM, ErrorKind::InvalidNumber, 4);
    error_at_with("\u{FEFF}\"\\q\"", IGNORE_BOM, ErrorKind::InvalidEscape, 5);
    error_at_with("\u{FEFF}1 2", IGNORE_BOM, ErrorKind::TrailingContent, 5);
    error_at_with("\u{FEFF}[", IGNORE_BOM, ErrorKind::UnexpectedEof, 4);
    error_at_with(
        "\u{FEFF}[[]]",
        IGNORE_BOM.with_max_depth(1),
        ErrorKind::DepthExceeded,
        4,
    );
}

#[test]
fn error_display_names_kind_and_offset() {
    let error = reject("[1,]");
    assert_eq!(error.to_string(), "unexpected character at byte offset 3");
}
