//! Helpers shared by the test binaries.
//!
//! Every helper that drains an iterator bounds it. Each tokenizer event
//! consumes at least one input byte, so a correct tokenizer yields at most
//! `input.len()` events and one error; each segment consumes at least one
//! byte of the raw text. A tokenizer that is not fused after an error would
//! otherwise yield that error forever.

#![allow(dead_code)]

use xee_json::{Decoded, Error, Event, Options, Segment, Str, Tokenizer};

/// Every item the tokenizer yields, bounded at `input.len() + 2` items;
/// more than `input.len() + 1` fails the test.
pub fn items_with(input: &str, options: Options) -> Vec<Result<Event<'_>, Error>> {
    let bound = input.len() + 2;
    let items: Vec<_> = Tokenizer::with_options(input, options)
        .take(bound)
        .collect();
    assert!(
        items.len() < bound,
        "{} items for {} input bytes: the tokenizer does not stop",
        items.len(),
        input.len()
    );
    items
}

/// [`items_with`] under the default options.
pub fn items(input: &str) -> Vec<Result<Event<'_>, Error>> {
    items_with(input, Options::default())
}

/// The events, or the first error. Fails the test if anything follows an
/// error.
pub fn parse_with(input: &str, options: Options) -> Result<Vec<Event<'_>>, Error> {
    let items = items_with(input, options);
    let count = items.len();
    let mut events = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        match item {
            Ok(event) => events.push(event),
            Err(error) => {
                assert_eq!(index + 1, count, "items after the error {error:?}");
                return Err(error);
            }
        }
    }
    Ok(events)
}

/// [`parse_with`] under the default options.
pub fn parse(input: &str) -> Result<Vec<Event<'_>>, Error> {
    parse_with(input, Options::default())
}

/// The events of a text that must be accepted.
pub fn accept(input: &str) -> Vec<Event<'_>> {
    match parse(input) {
        Ok(events) => events,
        Err(error) => panic!("{input:?} rejected: {error}"),
    }
}

/// The error of a text that must be rejected under `options`.
pub fn reject_with(input: &str, options: Options) -> Error {
    match parse_with(input, options) {
        Ok(events) => panic!("{input:?} accepted: {events:?}"),
        Err(error) => error,
    }
}

/// The error of a text that must be rejected under the default options.
pub fn reject(input: &str) -> Error {
    reject_with(input, Options::default())
}

/// The segments of an escaped string, bounded at `raw.len() + 1`.
pub fn segments<'a>(string: &Str<'a>) -> Vec<Segment<'a>> {
    match string {
        Str::Plain(text) => {
            if text.is_empty() {
                Vec::new()
            } else {
                vec![Segment::Literal(text)]
            }
        }
        Str::Escaped(escaped) => {
            let bound = escaped.raw().len() + 1;
            let segments: Vec<_> = escaped.segments().take(bound).collect();
            assert!(
                segments.len() < bound,
                "{} segments for {} raw bytes",
                segments.len(),
                escaped.raw().len()
            );
            segments
        }
    }
}

/// The decoded string as characters and lone surrogates, from the segments.
pub fn decode(string: &Str<'_>) -> Vec<Result<char, u16>> {
    let mut decoded = Vec::new();
    for segment in segments(string) {
        match segment {
            Segment::Literal(text) => decoded.extend(text.chars().map(Ok)),
            Segment::Escape {
                decoded: Decoded::Char(c),
                ..
            } => decoded.push(Ok(c)),
            Segment::Escape {
                decoded: Decoded::LoneSurrogate(unit),
                ..
            } => decoded.push(Err(unit)),
        }
    }
    decoded
}

/// The decoded string, if it has no lone surrogate.
pub fn decode_string(string: &Str<'_>) -> Option<String> {
    decode(string).into_iter().collect::<Result<_, _>>().ok()
}

/// The lone surrogates in a string, in order.
pub fn lone_surrogates(string: &Str<'_>) -> Vec<u16> {
    decode(string).into_iter().filter_map(Result::err).collect()
}

/// An independent decoder for the oracle: the raw text as UTF-16 code
/// units, every `\uXXXX` one unit, then `char::decode_utf16`, which pairs
/// surrogates and reports each unpaired one. Only valid raw text reaches
/// it.
fn oracle_decode(raw: &str) -> Vec<Result<char, u16>> {
    let mut units = Vec::new();
    let mut chars = raw.chars();
    let mut buffer = [0u16; 2];
    while let Some(c) = chars.next() {
        if c != '\\' {
            units.extend_from_slice(c.encode_utf16(&mut buffer));
            continue;
        }
        let unit = match chars.next() {
            Some('"') => u16::from(b'"'),
            Some('\\') => u16::from(b'\\'),
            Some('/') => u16::from(b'/'),
            Some('b') => 0x08,
            Some('f') => 0x0C,
            Some('n') => 0x0A,
            Some('r') => 0x0D,
            Some('t') => 0x09,
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                u16::from_str_radix(&hex, 16).expect("four hex digits")
            }
            other => panic!("invalid escape {other:?} in validated text {raw:?}"),
        };
        units.push(unit);
    }
    char::decode_utf16(units)
        .map(|r| r.map_err(|e| e.unpaired_surrogate()))
        .collect()
}

/// Checks the segment invariants of one string:
/// - the literal texts and escape sources concatenate to the raw text;
/// - no literal is empty or contains a backslash, and every source is one
///   escape (2, 6 or 12 bytes starting with a backslash);
/// - a `Plain` string has no backslash and an `Escaped` one has one;
/// - the decoded sequence equals the UTF-16 oracle's, lone surrogates
///   included.
pub fn check_segments(string: &Str<'_>) {
    let raw = string.raw();
    match string {
        Str::Plain(text) => assert!(!text.contains('\\'), "plain {text:?}"),
        Str::Escaped(escaped) => assert!(escaped.raw().contains('\\'), "escaped {raw:?}"),
    }
    let mut joined = String::new();
    for segment in segments(string) {
        match segment {
            Segment::Literal(text) => {
                assert!(!text.is_empty(), "empty literal in {raw:?}");
                assert!(!text.contains('\\'), "literal {text:?} in {raw:?}");
                joined.push_str(text);
            }
            Segment::Escape { decoded, source } => {
                assert!(source.starts_with('\\'), "source {source:?} in {raw:?}");
                let expected_len = match decoded {
                    Decoded::Char(c) if source.len() == 12 => {
                        assert!(u32::from(c) >= 0x10000, "pair {source:?} gave {c:?}");
                        12
                    }
                    _ if source.starts_with("\\u") => 6,
                    _ => 2,
                };
                assert_eq!(source.len(), expected_len, "source {source:?} in {raw:?}");
                joined.push_str(source);
            }
        }
    }
    assert_eq!(joined, raw, "segments do not reassemble the raw text");
    assert_eq!(decode(string), oracle_decode(raw), "decoding {raw:?}");
}

/// Checks that an event sequence is one balanced JSON value: brackets
/// match, and object members alternate key and value.
pub fn check_balanced(events: &[Event<'_>]) -> Result<(), String> {
    #[derive(PartialEq)]
    enum Open {
        Array,
        /// An object, and whether a key awaits its value.
        Object(bool),
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut values_at_top = 0usize;
    for (index, event) in events.iter().enumerate() {
        let fail = |what: &str| Err(format!("event {index} {event:?}: {what}"));
        if let Event::Key(_) = event {
            match stack.last_mut() {
                Some(Open::Object(awaiting)) if !*awaiting => *awaiting = true,
                _ => return fail("key outside an object or in value position"),
            }
            continue;
        }
        if let Event::EndArray | Event::EndObject = event {
            match (stack.pop(), event) {
                (Some(Open::Array), Event::EndArray)
                | (Some(Open::Object(false)), Event::EndObject) => {}
                _ => return fail("unmatched close"),
            }
        } else {
            // A value, or the start of one.
            match stack.last_mut() {
                None => values_at_top += 1,
                Some(Open::Array) => {}
                Some(Open::Object(awaiting)) => {
                    if !*awaiting {
                        return fail("value without a key");
                    }
                    *awaiting = false;
                }
            }
            match event {
                Event::StartArray => stack.push(Open::Array),
                Event::StartObject => stack.push(Open::Object(false)),
                _ => {}
            }
        }
    }
    if !stack.is_empty() {
        return Err(format!("{} containers left open", stack.len()));
    }
    if values_at_top != 1 {
        return Err(format!("{values_at_top} top-level values"));
    }
    Ok(())
}

/// Every string and key in an event sequence.
pub fn strings<'a, 'e>(events: &'e [Event<'a>]) -> impl Iterator<Item = Str<'a>> + 'e {
    events.iter().filter_map(|event| match event {
        Event::Key(s) | Event::String(s) => Some(*s),
        _ => None,
    })
}

/// The number of RFC 8259 §2 whitespace characters outside string
/// contents in a JSON text. The text must be valid JSON: inside a string,
/// a backslash is taken to start a two-character or `\u` escape.
pub fn whitespace_outside_strings(text: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut after_backslash = false;
    for c in text.chars() {
        if in_string {
            if after_backslash {
                after_backslash = false;
            } else if c == '\\' {
                after_backslash = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if matches!(c, ' ' | '\t' | '\n' | '\r') {
            count += 1;
        }
    }
    count
}
