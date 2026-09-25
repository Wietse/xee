//! The writer's round-trip property: whatever it writes, the tokenizer
//! reads back as the same events, with keys and strings decoding to the
//! text that was written and number text unchanged.
//!
//! Documents are generated from fixed seeds by a small std-only generator
//! (splitmix64), so a failure reproduces: its message names the
//! configuration and the document's index. Verbatim text is excluded: it
//! may make invalid JSON by design.

mod common;

use xee_json::{Decoded, EscapeProfile, Event, Layout, Segment, Str, Writer};

use common::{check_segments, parse, segments};

/// Documents per configuration.
const DOCUMENTS: usize = 1000;
/// The deepest nesting generated below the top-level value.
const MAX_DEPTH: usize = 5;

/// An owned event: what was written, and what must be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ev {
    StartObject,
    EndObject,
    StartArray,
    EndArray,
    Key(String),
    String(String),
    Number(String),
    Bool(bool),
    Null,
}

/// splitmix64: a deterministic generator, good enough to pick test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }

    fn char_in(&mut self, low: u32, high: u32) -> char {
        let cp = low + self.below(u64::from(high - low + 1)) as u32;
        char::from_u32(cp).expect("the ranges hold no surrogates")
    }
}

/// The character classes of Serialization 3.1 §9 escaping, and what the
/// predicates in the configurations tell apart.
const CLASSES: usize = 9;

/// Counts of what the generated documents covered, checked at the end so
/// a generator change cannot quietly stop covering a class.
#[derive(Debug, Default)]
struct Coverage {
    classes: [usize; CLASSES],
    empty_strings: usize,
    empty_arrays: usize,
    empty_objects: usize,
    nested: usize,
    repeated_keys: usize,
    numbers: usize,
    bools: usize,
    nulls: usize,
    top_level_scalars: usize,
}

struct Generator {
    rng: Rng,
    coverage: Coverage,
}

impl Generator {
    fn char(&mut self) -> char {
        let class = self.rng.below(CLASSES as u64) as usize;
        self.coverage.classes[class] += 1;
        match class {
            // The eight two-character escapes.
            0 => self
                .rng
                .pick(&['"', '\\', '/', '\u{8}', '\u{C}', '\n', '\r', '\t']),
            // The other C0 controls, U+0000 included.
            1 => self.rng.pick(&[
                '\u{0}', '\u{1}', '\u{7}', '\u{B}', '\u{E}', '\u{10}', '\u{1B}', '\u{1F}',
            ]),
            // Delete and the C1 controls.
            2 => self.rng.char_in(0x7F, 0x9F),
            // Line and paragraph separators.
            3 => self.rng.pick(&['\u{2028}', '\u{2029}']),
            // Outside the Basic Multilingual Plane.
            4 => self.rng.char_in(0x1_0000, 0x10_FFFF),
            // Printable ASCII.
            5 => self.rng.char_in(0x20, 0x7E),
            // The rest of Latin-1.
            6 => self.rng.char_in(0xA0, 0xFF),
            // The rest of the BMP below the surrogates.
            7 => self.rng.char_in(0x100, 0xD7FF),
            // The rest of the BMP above them.
            _ => self.rng.char_in(0xE000, 0xFFFF),
        }
    }

    fn string(&mut self) -> String {
        let len = self.rng.below(9);
        if len == 0 {
            self.coverage.empty_strings += 1;
        }
        (0..len).map(|_| self.char()).collect()
    }

    fn number(&mut self) -> String {
        self.coverage.numbers += 1;
        let mut text = String::new();
        if self.rng.below(2) == 0 {
            text.push('-');
        }
        if self.rng.below(3) == 0 {
            text.push('0');
        } else {
            text.push(self.rng.char_in(u32::from(b'1'), u32::from(b'9')));
            for _ in 0..self.rng.below(4) {
                text.push(self.rng.char_in(u32::from(b'0'), u32::from(b'9')));
            }
        }
        if self.rng.below(2) == 0 {
            text.push('.');
            for _ in 0..=self.rng.below(3) {
                text.push(self.rng.char_in(u32::from(b'0'), u32::from(b'9')));
            }
        }
        if self.rng.below(2) == 0 {
            text.push(self.rng.pick(&['e', 'E']));
            if let Some(sign) = self.rng.pick(&[None, Some('+'), Some('-')]) {
                text.push(sign);
            }
            for _ in 0..=self.rng.below(3) {
                text.push(self.rng.char_in(u32::from(b'0'), u32::from(b'9')));
            }
        }
        text
    }

    /// One value's events, appended to `events`.
    fn value(&mut self, depth: usize, keys: &[String], events: &mut Vec<Ev>) {
        let kinds = if depth >= MAX_DEPTH { 4 } else { 6 };
        match self.rng.below(kinds) {
            0 => {
                let event = match self.rng.below(3) {
                    0 => {
                        self.coverage.nulls += 1;
                        Ev::Null
                    }
                    _ => {
                        self.coverage.bools += 1;
                        Ev::Bool(self.rng.below(2) == 0)
                    }
                };
                events.push(event);
            }
            1 => events.push(Ev::Number(self.number())),
            2 | 3 => events.push(Ev::String(self.string())),
            4 => self.array(depth, keys, events),
            _ => self.object(depth, keys, events),
        }
    }

    fn array(&mut self, depth: usize, keys: &[String], events: &mut Vec<Ev>) {
        if depth > 0 {
            self.coverage.nested += 1;
        }
        let len = self.rng.below(5);
        if len == 0 {
            self.coverage.empty_arrays += 1;
        }
        events.push(Ev::StartArray);
        for _ in 0..len {
            self.value(depth + 1, keys, events);
        }
        events.push(Ev::EndArray);
    }

    /// An object whose keys come from `keys`, so that they repeat.
    fn object(&mut self, depth: usize, keys: &[String], events: &mut Vec<Ev>) {
        if depth > 0 {
            self.coverage.nested += 1;
        }
        let len = self.rng.below(5);
        if len == 0 {
            self.coverage.empty_objects += 1;
        }
        let mut used = Vec::new();
        events.push(Ev::StartObject);
        for _ in 0..len {
            let key = self.rng.pick(&[0, 1, 2]);
            if used.contains(&key) {
                self.coverage.repeated_keys += 1;
            }
            used.push(key);
            events.push(Ev::Key(keys[key].clone()));
            self.value(depth + 1, keys, events);
        }
        events.push(Ev::EndObject);
    }

    /// One document: a small per-document key pool, so keys repeat, then
    /// the top-level value (a container two times in three).
    fn document(&mut self) -> Vec<Ev> {
        let keys: Vec<String> = (0..3).map(|_| self.string()).collect();
        let mut events = Vec::new();
        match self.rng.below(3) {
            0 => {
                self.coverage.top_level_scalars += 1;
                // At the depth limit, `value` picks a scalar.
                self.value(MAX_DEPTH, &keys, &mut events);
            }
            1 => self.array(0, &keys, &mut events),
            _ => self.object(0, &keys, &mut events),
        }
        events
    }
}

/// Drives the writer with `events`.
fn write(writer: &mut Writer<'_>, events: &[Ev]) {
    for event in events {
        let result = match event {
            Ev::StartObject => writer.start_object(),
            Ev::EndObject => writer.end_object(),
            Ev::StartArray => writer.start_array(),
            Ev::EndArray => writer.end_array(),
            Ev::Key(text) => writer.key(text),
            Ev::String(text) => writer.string(text),
            Ev::Number(text) => writer.number(text),
            Ev::Bool(value) => writer.bool(*value),
            Ev::Null => writer.null(),
        };
        result.unwrap_or_else(|error| panic!("{event:?} refused: {error}"));
    }
}

/// The decoded text of a key or string read back. A lone surrogate fails
/// the test: the writer only ever writes whole characters.
fn decoded(string: &Str<'_>) -> String {
    check_segments(string);
    let mut text = String::new();
    for segment in segments(string) {
        match segment {
            Segment::Literal(literal) => text.push_str(literal),
            Segment::Escape {
                decoded: Decoded::Char(c),
                ..
            } => text.push(c),
            Segment::Escape {
                decoded: Decoded::LoneSurrogate(unit),
                ..
            } => panic!("lone surrogate {unit:04X} in {:?}", string.raw()),
        }
    }
    text
}

/// Checks what §9 requires of the raw text of every written string: no
/// solidus and nothing in U+007F to U+009F is left unescaped.
fn check_raw(string: &Str<'_>) {
    let raw = string.raw();
    let mut after_backslash = false;
    for c in raw.chars() {
        if after_backslash {
            after_backslash = false;
            continue;
        }
        match c {
            '\\' => after_backslash = true,
            '/' | '\u{7F}'..='\u{9F}' => panic!("unescaped {c:?} in {raw:?}"),
            _ => {}
        }
    }
}

/// The events read back from `text`, through the bounded drain.
fn read_back(text: &str) -> Vec<Ev> {
    let events = match parse(text) {
        Ok(events) => events,
        Err(error) => panic!("{text:?} rejected: {error}"),
    };
    events
        .iter()
        .map(|event| match event {
            Event::StartObject => Ev::StartObject,
            Event::EndObject => Ev::EndObject,
            Event::StartArray => Ev::StartArray,
            Event::EndArray => Ev::EndArray,
            Event::Key(key) => {
                check_raw(key);
                Ev::Key(decoded(key))
            }
            Event::String(string) => {
                check_raw(string);
                Ev::String(decoded(string))
            }
            Event::Number(text) => Ev::Number((*text).to_string()),
            Event::Bool(value) => Ev::Bool(*value),
            Event::Null => Ev::Null,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum Encoding {
    /// Every character encodable (UTF-8, UTF-16).
    All,
    /// US-ASCII.
    Ascii,
    /// ISO-8859-1.
    Latin1,
}

fn round_trip(layout: Layout, encoding: Encoding, seed: u64) {
    let mut generator = Generator {
        rng: Rng(seed),
        coverage: Coverage::default(),
    };
    for index in 0..DOCUMENTS {
        let events = generator.document();
        let writer = Writer::new(layout, EscapeProfile::Serialization);
        let mut writer = match encoding {
            Encoding::All => writer.with_encodable(|_| true),
            Encoding::Ascii => writer.with_encodable(|c| c.is_ascii()),
            Encoding::Latin1 => writer.with_encodable(|c| u32::from(c) <= 0xFF),
        };
        let context = format!("{layout:?} {encoding:?} seed {seed:#x} document {index}");
        write(&mut writer, &events);
        let text = writer
            .finish()
            .unwrap_or_else(|error| panic!("{context}: {error}"));
        assert_eq!(read_back(&text), events, "{context}: {text:?}");
        match encoding {
            Encoding::All => {}
            Encoding::Ascii => assert!(text.is_ascii(), "{context}: {text:?}"),
            Encoding::Latin1 => assert!(
                text.chars().all(|c| u32::from(c) <= 0xFF),
                "{context}: {text:?}"
            ),
        }
        if let Layout::Compact = layout {
            assert_eq!(
                common::whitespace_outside_strings(&text),
                0,
                "{context}: {text:?}"
            );
        }
    }
    let coverage = &generator.coverage;
    assert!(coverage.classes.iter().all(|&n| n > 0), "{coverage:?}");
    for (what, n) in [
        ("empty strings", coverage.empty_strings),
        ("empty arrays", coverage.empty_arrays),
        ("empty objects", coverage.empty_objects),
        ("nested containers", coverage.nested),
        ("repeated keys", coverage.repeated_keys),
        ("numbers", coverage.numbers),
        ("booleans", coverage.bools),
        ("nulls", coverage.nulls),
        ("top-level scalars", coverage.top_level_scalars),
    ] {
        assert!(n > 0, "no {what}: {coverage:?}");
    }
}

#[test]
fn compact_all_encodable() {
    round_trip(Layout::Compact, Encoding::All, 0x5EED_0001);
}

#[test]
fn compact_ascii_only() {
    round_trip(Layout::Compact, Encoding::Ascii, 0x5EED_0002);
}

#[test]
fn compact_latin1_only() {
    round_trip(Layout::Compact, Encoding::Latin1, 0x5EED_0003);
}

#[test]
fn indented_all_encodable() {
    round_trip(Layout::Indented, Encoding::All, 0x5EED_0004);
}

#[test]
fn indented_ascii_only() {
    round_trip(Layout::Indented, Encoding::Ascii, 0x5EED_0005);
}

#[test]
fn indented_latin1_only() {
    round_trip(Layout::Indented, Encoding::Latin1, 0x5EED_0006);
}

/// The writer without a predicate writes like the one whose predicate
/// accepts everything.
#[test]
fn no_predicate_is_all_encodable() {
    let mut generator = Generator {
        rng: Rng(0x5EED_0007),
        coverage: Coverage::default(),
    };
    for _ in 0..200 {
        let events = generator.document();
        let mut plain = Writer::new(Layout::Compact, EscapeProfile::Serialization);
        let mut all =
            Writer::new(Layout::Compact, EscapeProfile::Serialization).with_encodable(|_| true);
        write(&mut plain, &events);
        write(&mut all, &events);
        assert_eq!(plain.finish().unwrap(), all.finish().unwrap());
    }
}
