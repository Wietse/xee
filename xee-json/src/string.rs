//! Strings and keys as the tokenizer delivers them, and their lazy decoding.

use std::iter::FusedIterator;

/// A JSON string or object key, borrowed from the input.
///
/// Equality compares the source text, not the decoded value: `"\u0061"` and
/// `"a"` are different `Str`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Str<'a> {
    /// A string with no escape sequences. The slice between the quotes is
    /// its value.
    Plain(&'a str),
    /// A string with at least one escape sequence; decode it with
    /// [`Escaped::segments`].
    Escaped(Escaped<'a>),
}

impl<'a> Str<'a> {
    /// The source text between the quotes, with escapes as written.
    pub fn raw(&self) -> &'a str {
        match self {
            Str::Plain(text) => text,
            Str::Escaped(escaped) => escaped.raw(),
        }
    }
}

/// A string with escape sequences: the source text between the quotes,
/// already validated by the tokenizer.
///
/// Only the tokenizer creates it, so [`Escaped::segments`] always works on
/// text that matches the RFC 8259 §7 string grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escaped<'a> {
    raw: &'a str,
}

impl<'a> Escaped<'a> {
    pub(crate) fn new(raw: &'a str) -> Self {
        Escaped { raw }
    }

    /// The source text between the quotes, with escapes as written.
    pub fn raw(&self) -> &'a str {
        self.raw
    }

    /// The string's segments in order: literal runs and decoded escapes.
    ///
    /// Concatenating each [`Segment::Literal`] text and each escape's
    /// `source` gives back [`Escaped::raw`] exactly.
    pub fn segments(&self) -> Segments<'a> {
        Segments { rest: self.raw }
    }
}

/// One piece of an escaped string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
    /// A non-empty run of source text without escapes; it is its own value.
    Literal(&'a str),
    /// One escape sequence and its value.
    Escape {
        /// The decoded value.
        decoded: Decoded,
        /// The escape as written: two characters (`\n`), six (`\u00e9`), or
        /// twelve for a surrogate pair (`\uD834\uDD1E`).
        source: &'a str,
    },
}

/// The value of one escape sequence.
///
/// This cannot be a `char`: a JSON escape may name a UTF-16 surrogate that is
/// not part of a pair, and a surrogate is not a Unicode scalar value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decoded {
    /// A character, including one written as a surrogate pair.
    Char(char),
    /// An escaped UTF-16 surrogate (U+D800 to U+DFFF) that is not part of a
    /// pair: a high surrogate not immediately followed by an escaped low
    /// one, or a low surrogate not immediately preceded by an escaped high
    /// one (RFC 8259 §8.2).
    LoneSurrogate(u16),
}

/// The iterator returned by [`Escaped::segments`].
///
/// A high surrogate escape pairs only with an immediately following
/// `\uDC00`–`\uDFFF` escape. A high surrogate followed by another high
/// surrogate is lone, and the second is then considered afresh, so it can
/// pair with the escape after it. This is the pairing rule of UTF-16
/// decoding (`char::decode_utf16`).
#[derive(Debug, Clone)]
pub struct Segments<'a> {
    rest: &'a str,
}

impl<'a> Iterator for Segments<'a> {
    type Item = Segment<'a>;

    fn next(&mut self) -> Option<Segment<'a>> {
        let bytes = self.rest.as_bytes();
        let first = *bytes.first()?;
        let (decoded, len) = if first == b'\\' {
            match escape_at(bytes) {
                Some((decoded, len)) => (Some(decoded), len),
                None => {
                    // Unreachable for text the tokenizer validated; ending
                    // the iteration keeps this panic-free regardless.
                    self.rest = "";
                    return None;
                }
            }
        } else {
            let len = bytes
                .iter()
                .position(|&b| b == b'\\')
                .unwrap_or(bytes.len());
            (None, len)
        };
        // `len` ends at a backslash, after an ASCII escape, or at the end,
        // so it is a character boundary; `split_at_checked` makes that a
        // check rather than an assumption.
        let Some((head, tail)) = self.rest.split_at_checked(len) else {
            self.rest = "";
            return None;
        };
        self.rest = tail;
        Some(match decoded {
            Some(decoded) => Segment::Escape {
                decoded,
                source: head,
            },
            None => Segment::Literal(head),
        })
    }
}

impl FusedIterator for Segments<'_> {}

/// Decodes the escape at the start of `bytes`: its value and its length in
/// bytes. `None` if `bytes` does not start with a valid escape.
fn escape_at(bytes: &[u8]) -> Option<(Decoded, usize)> {
    if bytes.first() != Some(&b'\\') {
        return None;
    }
    let simple = match *bytes.get(1)? {
        b'"' => '"',
        b'\\' => '\\',
        b'/' => '/',
        b'b' => '\u{8}',
        b'f' => '\u{c}',
        b'n' => '\n',
        b'r' => '\r',
        b't' => '\t',
        b'u' => return unicode_escape_at(bytes),
        _ => return None,
    };
    Some((Decoded::Char(simple), 2))
}

/// Decodes a `\uXXXX` escape at the start of `bytes`, pairing a high
/// surrogate with an immediately following low-surrogate escape.
fn unicode_escape_at(bytes: &[u8]) -> Option<(Decoded, usize)> {
    let unit = hex4(bytes.get(2..6)?)?;
    match unit {
        0xD800..=0xDBFF => {
            if let Some(low) = low_surrogate_escape(bytes.get(6..12)) {
                let scalar =
                    0x10000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
                return Some((Decoded::Char(char::from_u32(scalar)?), 12));
            }
            Some((Decoded::LoneSurrogate(unit), 6))
        }
        0xDC00..=0xDFFF => Some((Decoded::LoneSurrogate(unit), 6)),
        _ => Some((Decoded::Char(char::from_u32(u32::from(unit))?), 6)),
    }
}

/// The code unit of `bytes` if it is exactly a `\uDC00`–`\uDFFF` escape.
fn low_surrogate_escape(bytes: Option<&[u8]>) -> Option<u16> {
    let (prefix, digits) = bytes?.split_at_checked(2)?;
    if prefix != b"\\u" {
        return None;
    }
    hex4(digits).filter(|unit| (0xDC00..=0xDFFF).contains(unit))
}

/// Four hexadecimal digits, either case, as a code unit.
fn hex4(digits: &[u8]) -> Option<u16> {
    if digits.len() != 4 {
        return None;
    }
    digits.iter().try_fold(0u16, |acc, &b| {
        let digit = char::from(b).to_digit(16)?;
        Some((acc << 4) | u16::try_from(digit).ok()?)
    })
}
