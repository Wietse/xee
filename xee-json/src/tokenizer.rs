//! The pull tokenizer.

use std::iter::FusedIterator;

use crate::error::{Error, ErrorKind};
use crate::number;
use crate::string::{Escaped, Str};

/// The default maximum nesting depth: 512 open arrays and objects.
///
/// RFC 8259 §9 lets a parser limit nesting. 512 is the limit of the `json`
/// crate that Xee's `fn:parse-json` has used, so adopting this tokenizer
/// changes which texts Xee accepts only in the one case described below; in
/// particular JSONTestSuite's `i_structure_500_nested_arrays` stays accepted.
///
/// The limit counts every open container: with a limit of `L`, `L` nested
/// containers are accepted and the `L + 1`th opening bracket is an
/// [`ErrorKind::DepthExceeded`] error at that bracket. The `json` crate checks
/// its limit only before pushing a *non-empty* container, so it also accepts
/// 513 levels when the innermost one is empty (`[[…[]…]]`); this tokenizer
/// rejects that input. The difference is one level at exactly 513 and is
/// deliberate: the limit means the same for every input.
pub const DEFAULT_MAX_DEPTH: usize = 512;

/// What to do with a byte order mark (U+FEFF) at the start of the input.
///
/// A byte order mark anywhere else is never skipped: outside a string it is
/// an [`ErrorKind::UnexpectedChar`], inside one it is an ordinary character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BomPolicy {
    /// A leading byte order mark is an [`ErrorKind::BomNotAllowed`] error.
    ///
    /// The default, because the RFC 8259 §2 JSON-text grammar has no byte
    /// order mark; accepting one is an opt-in the caller makes knowingly.
    #[default]
    Reject,
    /// A leading byte order mark is skipped. RFC 8259 §8.1 allows a parser to
    /// ignore it, and F&O 3.1 §17.5.1 requires `fn:parse-json` to. Error
    /// offsets still count its three bytes.
    Ignore,
}

/// Tokenizer options: the byte order mark policy and the maximum depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Options {
    bom_policy: BomPolicy,
    max_depth: usize,
}

impl Options {
    /// The defaults: [`BomPolicy::Reject`] and [`DEFAULT_MAX_DEPTH`].
    pub const fn new() -> Self {
        Options {
            bom_policy: BomPolicy::Reject,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    /// Sets the byte order mark policy.
    pub const fn with_bom_policy(self, bom_policy: BomPolicy) -> Self {
        Options { bom_policy, ..self }
    }

    /// Sets the maximum nesting depth: the number of arrays and objects that
    /// may be open at once. There is no unbounded setting; 0 allows only a
    /// scalar top-level value.
    pub const fn with_max_depth(self, max_depth: usize) -> Self {
        Options { max_depth, ..self }
    }

    /// The byte order mark policy.
    pub const fn bom_policy(&self) -> BomPolicy {
        self.bom_policy
    }

    /// The maximum nesting depth.
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl Default for Options {
    fn default() -> Self {
        Options::new()
    }
}

/// One step of a JSON text, in document order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// `{`
    StartObject,
    /// `}`
    EndObject,
    /// `[`
    StartArray,
    /// `]`
    EndArray,
    /// An object member's name. Its value's events follow.
    Key(Str<'a>),
    /// A string value.
    String(Str<'a>),
    /// A number, as its lexical text (RFC 8259 §6), for example `-1.5e+10`.
    Number(&'a str),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Container {
    Array,
    Object,
}

/// Where the tokenizer is in the grammar: what the next call expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing read yet: the byte order mark, then the top-level value.
    Start,
    /// Just after `[`: a value or `]`.
    ArrayFirst,
    /// Just after `{`: a key or `}`.
    ObjectFirst,
    /// After a key: `:` and then a value.
    Colon,
    /// After a value: `,` or a closing bracket inside a container, the end
    /// of the text at the top level.
    AfterValue,
    /// Done, after the end or after an error.
    Finished,
}

/// A strict JSON pull tokenizer over a `&str`; see the [crate] docs.
///
/// Each item is one [`Event`], or the first [`Error`]. The tokenizer is
/// fused: after an error or the end of the text it yields only `None`. The
/// text is valid only if the iteration reaches `None` without an error.
///
/// The tokenizer does not allocate per event; its only allocation is the
/// container stack, which never grows beyond [`Options::max_depth`].
#[derive(Debug, Clone)]
pub struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
    stack: Vec<Container>,
    options: Options,
    state: State,
}

impl<'a> Tokenizer<'a> {
    /// A tokenizer with the default [`Options`].
    pub fn new(input: &'a str) -> Self {
        Tokenizer::with_options(input, Options::new())
    }

    /// A tokenizer with the given options.
    pub fn with_options(input: &'a str, options: Options) -> Self {
        Tokenizer {
            input,
            pos: 0,
            stack: Vec::new(),
            options,
            state: State::Start,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn error(&self, kind: ErrorKind) -> Error {
        Error::new(kind, self.pos)
    }

    /// An error at the current byte: [`ErrorKind::UnexpectedEof`] at the end
    /// of the text, `kind` otherwise.
    fn error_here(&self, kind: ErrorKind) -> Error {
        if self.pos >= self.input.len() {
            self.error(ErrorKind::UnexpectedEof)
        } else {
            self.error(kind)
        }
    }

    /// The input between two byte offsets. Every offset the tokenizer
    /// slices at is the position of an ASCII byte or the end of the input,
    /// so it is a character boundary; `get` keeps that a check instead of a
    /// possible panic.
    fn text(&self, start: usize, end: usize) -> &'a str {
        self.input.get(start..end).unwrap_or_default()
    }

    /// Skips RFC 8259 §2 whitespace: space, tab, line feed, carriage return.
    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.pos += 1;
        }
    }

    fn start(&mut self) -> Result<Option<Event<'a>>, Error> {
        if self.input.starts_with('\u{FEFF}') {
            match self.options.bom_policy {
                BomPolicy::Reject => return Err(self.error(ErrorKind::BomNotAllowed)),
                BomPolicy::Ignore => self.pos = '\u{FEFF}'.len_utf8(),
            }
        }
        self.value()
    }

    /// Reads one value, or the opening bracket of one.
    fn value(&mut self) -> Result<Option<Event<'a>>, Error> {
        self.skip_whitespace();
        let event = match self.peek() {
            Some(b'[') => return self.open(Container::Array),
            Some(b'{') => return self.open(Container::Object),
            Some(b'"') => Event::String(self.string()?),
            Some(b'-' | b'0'..=b'9') => Event::Number(self.number()?),
            Some(b't') => {
                self.literal(b"true")?;
                Event::Bool(true)
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Event::Bool(false)
            }
            Some(b'n') => {
                self.literal(b"null")?;
                Event::Null
            }
            _ => return Err(self.error_here(ErrorKind::UnexpectedChar)),
        };
        self.state = State::AfterValue;
        Ok(Some(event))
    }

    fn open(&mut self, container: Container) -> Result<Option<Event<'a>>, Error> {
        if self.stack.len() >= self.options.max_depth {
            return Err(self.error(ErrorKind::DepthExceeded));
        }
        self.stack.push(container);
        self.pos += 1;
        Ok(Some(match container {
            Container::Array => {
                self.state = State::ArrayFirst;
                Event::StartArray
            }
            Container::Object => {
                self.state = State::ObjectFirst;
                Event::StartObject
            }
        }))
    }

    /// Consumes the closing bracket at the current byte.
    fn close(&mut self) -> Result<Option<Event<'a>>, Error> {
        let event = match self.stack.pop() {
            Some(Container::Array) => Event::EndArray,
            Some(Container::Object) => Event::EndObject,
            // Callers close only an open container; this keeps a logic
            // error an error rather than a panic.
            None => return Err(self.error(ErrorKind::UnexpectedChar)),
        };
        self.pos += 1;
        self.state = State::AfterValue;
        Ok(Some(event))
    }

    fn array_first(&mut self) -> Result<Option<Event<'a>>, Error> {
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            return self.close();
        }
        self.value()
    }

    fn object_first(&mut self) -> Result<Option<Event<'a>>, Error> {
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            return self.close();
        }
        self.key()
    }

    fn key(&mut self) -> Result<Option<Event<'a>>, Error> {
        self.skip_whitespace();
        if self.peek() != Some(b'"') {
            return Err(self.error_here(ErrorKind::UnexpectedChar));
        }
        let key = self.string()?;
        self.state = State::Colon;
        Ok(Some(Event::Key(key)))
    }

    fn colon(&mut self) -> Result<Option<Event<'a>>, Error> {
        self.skip_whitespace();
        if self.peek() != Some(b':') {
            return Err(self.error_here(ErrorKind::UnexpectedChar));
        }
        self.pos += 1;
        self.value()
    }

    fn after_value(&mut self) -> Result<Option<Event<'a>>, Error> {
        self.skip_whitespace();
        let Some(&container) = self.stack.last() else {
            // The top-level value is complete: only the end may follow.
            if self.pos < self.input.len() {
                return Err(self.error(ErrorKind::TrailingContent));
            }
            return Ok(None);
        };
        match (container, self.peek()) {
            (_, Some(b',')) => {
                self.pos += 1;
                match container {
                    Container::Array => self.value(),
                    Container::Object => self.key(),
                }
            }
            (Container::Array, Some(b']')) | (Container::Object, Some(b'}')) => self.close(),
            _ => Err(self.error_here(ErrorKind::UnexpectedChar)),
        }
    }

    /// Reads the string whose opening quote is the current byte.
    fn string(&mut self) -> Result<Str<'a>, Error> {
        self.pos += 1;
        let start = self.pos;
        let mut escaped = false;
        loop {
            match self.peek() {
                Some(b'"') => {
                    let raw = self.text(start, self.pos);
                    self.pos += 1;
                    return Ok(if escaped {
                        Str::Escaped(Escaped::new(raw))
                    } else {
                        Str::Plain(raw)
                    });
                }
                Some(b'\\') => {
                    escaped = true;
                    self.escape()?;
                }
                Some(0x00..=0x1F) => return Err(self.error(ErrorKind::ControlCharInString)),
                // Any other byte, including those of multi-byte characters:
                // only the ASCII bytes above end or change the scan.
                Some(_) => self.skip_plain(),
                None => return Err(self.error(ErrorKind::UnexpectedEof)),
            }
        }
    }

    /// Advances past the run of bytes, starting at the current one, that
    /// cannot end or change a string scan (see [`is_string_special`]): eight
    /// bytes at a time while a whole word has none, then byte by byte. It
    /// stops at the first special byte or at the end of the input, exactly
    /// where a byte-by-byte scan stops, so error offsets do not change. The
    /// word test may only err towards reporting a special byte: that ends
    /// the fast part early and the byte scan decides, whereas a missed one
    /// would be skipped.
    fn skip_plain(&mut self) {
        let rest = self.input.as_bytes().get(self.pos..).unwrap_or_default();
        let mut skipped = 0;
        for word in rest.as_chunks::<8>().0 {
            if word_has_string_special(u64::from_le_bytes(*word)) {
                break;
            }
            skipped += 8;
        }
        let tail = rest.get(skipped..).unwrap_or_default();
        skipped += tail
            .iter()
            .position(|&byte| is_string_special(byte))
            .unwrap_or(tail.len());
        self.pos += skipped;
    }

    /// Validates the escape whose backslash is the current byte (RFC 8259
    /// §7). Surrogates are not paired here: every `\uXXXX` is valid, and
    /// [`Escaped::segments`] pairs them.
    fn escape(&mut self) -> Result<(), Error> {
        self.pos += 1;
        match self.peek() {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                self.pos += 1;
                Ok(())
            }
            Some(b'u') => {
                self.pos += 1;
                for _ in 0..4 {
                    match self.peek() {
                        Some(byte) if byte.is_ascii_hexdigit() => self.pos += 1,
                        _ => return Err(self.error_here(ErrorKind::InvalidEscape)),
                    }
                }
                Ok(())
            }
            _ => Err(self.error_here(ErrorKind::InvalidEscape)),
        }
    }

    /// Reads the number that starts at the current byte (RFC 8259 §6). The
    /// number ends at the first byte that cannot continue it; what follows
    /// is checked by the next state.
    ///
    /// The grammar is [`number::scan`], which the writer shares. An error is
    /// at the first byte that does not fit: [`ErrorKind::InvalidNumber`]
    /// (a leading zero such as `01` or `-00` is one at its second digit), or
    /// [`ErrorKind::UnexpectedEof`] when the text ends where a digit is
    /// required.
    fn number(&mut self) -> Result<&'a str, Error> {
        let start = self.pos;
        match number::scan(self.input.as_bytes(), start) {
            Ok(end) => {
                self.pos = end;
                Ok(self.text(start, end))
            }
            Err(offset) => {
                self.pos = offset;
                Err(self.error_here(ErrorKind::InvalidNumber))
            }
        }
    }

    /// Reads `literal`, whose first byte is the current byte.
    fn literal(&mut self, literal: &[u8]) -> Result<(), Error> {
        for &expected in literal {
            if self.peek() != Some(expected) {
                return Err(self.error_here(ErrorKind::InvalidLiteral));
            }
            self.pos += 1;
        }
        Ok(())
    }
}

/// Whether `byte` ends or changes the scan of a string: the closing quote,
/// the backslash of an escape, or a control character U+0000 to U+001F,
/// which RFC 8259 §7 does not allow unescaped. Every other byte, including
/// each byte of a multi-byte character, is part of a literal run.
fn is_string_special(byte: u8) -> bool {
    matches!(byte, b'"' | b'\\' | 0x00..=0x1F)
}

/// 0x01 in each byte of a word.
const ONES: u64 = u64::MAX / 0xFF;
/// 0x80 in each byte of a word.
const HIGHS: u64 = ONES << 7;

/// Non-zero exactly when some byte of `word` is below `bound` (at most
/// 0x80). Subtracting `bound` from each byte sets its high bit when the
/// byte is below `bound`; `!word` discards bytes whose high bit was
/// already set. A borrow can also mark a byte above one that is below
/// `bound`, but only when such a byte exists, so the answer to "is any
/// byte below `bound`" is exact.
fn bytes_below(word: u64, bound: u8) -> u64 {
    word.wrapping_sub(ONES * u64::from(bound)) & !word & HIGHS
}

/// Whether any of the eight bytes of `word` is [`is_string_special`]: a
/// control character is below 0x20, and XOR with a repeated byte turns
/// that byte into zero, which is below 1.
fn word_has_string_special(word: u64) -> bool {
    let controls = bytes_below(word, 0x20);
    let quotes = bytes_below(word ^ (ONES * u64::from(b'"')), 1);
    let backslashes = bytes_below(word ^ (ONES * u64::from(b'\\')), 1);
    controls | quotes | backslashes != 0
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Result<Event<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let step = match self.state {
            State::Finished => return None,
            State::Start => self.start(),
            State::ArrayFirst => self.array_first(),
            State::ObjectFirst => self.object_first(),
            State::Colon => self.colon(),
            State::AfterValue => self.after_value(),
        };
        match step {
            Ok(Some(event)) => Some(Ok(event)),
            Ok(None) => {
                self.state = State::Finished;
                None
            }
            Err(error) => {
                self.state = State::Finished;
                Some(Err(error))
            }
        }
    }
}

impl FusedIterator for Tokenizer<'_> {}
