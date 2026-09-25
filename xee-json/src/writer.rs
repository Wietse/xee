//! The writer: JSON text from calls made in document order.

use std::fmt;

use crate::number;

/// How the writer lays out the text between tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Layout {
    /// No whitespace at all outside string contents. Serialization 3.1
    /// §9.1.4 requires this when `indent` is `no`.
    #[default]
    Compact,
    /// Every array element and object member on a line of its own, indented
    /// by two spaces per open container, the closing bracket on a line of
    /// its own at the container's indentation, and one space after each
    /// `:`. An empty container stays `[]` or `{}`. No whitespace precedes
    /// the first token or follows the last. Serialization 3.1 §9.1.4 allows
    /// whitespace adjacent to structural tokens when `indent` is `yes`.
    Indented,
}

/// Which characters of a string the writer escapes, and how.
///
/// There is one profile today. The two escaping rules of F&O 3.1
/// `fn:xml-to-json` would be further variants, added with their consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum EscapeProfile {
    /// The JSON output method of Serialization 3.1 §9, which RFC 8259 §7
    /// also permits:
    ///
    /// - quotation mark, reverse solidus, **solidus**, backspace, form
    ///   feed, newline, carriage return and tab take their two-character
    ///   escapes (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`);
    /// - every other character in U+0000 to U+001F, and every character in
    ///   U+007F to U+009F, takes a six-character escape `\uHHHH`. §9 names
    ///   the range from 1; U+0000 is escaped too because RFC 8259 §7
    ///   requires it (an `xs:string` cannot hold it, but this writer takes
    ///   any Rust string);
    /// - a character that the output encoding cannot represent (see
    ///   [`Writer::with_encodable`]) takes a six-character escape, or two
    ///   of them forming a UTF-16 surrogate pair when it is outside the
    ///   Basic Multilingual Plane;
    /// - every other character is written as itself, including U+2028 and
    ///   U+2029.
    ///
    /// The hexadecimal digits of a six-character escape are **upper-case**
    /// (`\u001F`). §9 does not fix the case; upper-case is what F&O 3.1
    /// §17.5.1 `escape` uses in its examples.
    #[default]
    Serialization,
}

/// Why the writer refused a call.
///
/// A refused call writes nothing and leaves the writer as it was, so the
/// caller may continue with a valid call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WriteError {
    /// [`Writer::number`] text that is not exactly one RFC 8259 §6 number
    /// (no sign other than a leading `-`, no leading zero, no `NaN` or
    /// `INF`, no surrounding whitespace).
    InvalidNumber,
    /// A value, or the start of one, inside an object where a key is
    /// required: first in the object, or after a member's value.
    ValueWithoutKey,
    /// A key outside an object (in an array or at the top level), or
    /// directly after another key.
    UnexpectedKey,
    /// [`Writer::end_object`] directly after a key, before its value.
    KeyWithoutValue,
    /// A close that does not match the innermost open container, or a close
    /// with no container open.
    MismatchedClose,
    /// A value after the top-level value is complete: a JSON text has
    /// exactly one (RFC 8259 §2).
    SecondTopLevelValue,
    /// A call other than [`Writer::string_text`],
    /// [`Writer::string_verbatim`] and [`Writer::end_string`] while a
    /// string or key begun with [`Writer::begin_string`] or
    /// [`Writer::begin_key`] is still open.
    StringOpen,
    /// [`Writer::string_text`], [`Writer::string_verbatim`] or
    /// [`Writer::end_string`] with no string or key open.
    NoStringOpen,
    /// [`Writer::finish`] before the text is complete: no value written, a
    /// container or string still open, or a key without its value.
    Incomplete,
}

impl WriteError {
    fn description(self) -> &'static str {
        match self {
            WriteError::InvalidNumber => "not an RFC 8259 number",
            WriteError::ValueWithoutKey => "value in an object without a key",
            WriteError::UnexpectedKey => "key outside an object or after a key",
            WriteError::KeyWithoutValue => "object closed after a key without its value",
            WriteError::MismatchedClose => "close does not match the open container",
            WriteError::SecondTopLevelValue => "value after the top-level value",
            WriteError::StringOpen => "a string is still open",
            WriteError::NoStringOpen => "no string is open",
            WriteError::Incomplete => "the JSON text is incomplete",
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl std::error::Error for WriteError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Container {
    Array,
    Object,
}

/// One open container.
#[derive(Debug, Clone, Copy)]
struct Frame {
    container: Container,
    /// Whether an element or member has been started, so the next one
    /// needs a `,`.
    has_members: bool,
    /// An object only: a key has been written and its value has not.
    awaiting_value: bool,
}

/// What an open string will be when it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringRole {
    Key,
    Value,
}

/// A JSON writer: the caller makes one call per token, in document order,
/// and [`Writer::finish`] returns the text.
///
/// - **Nothing is merged or reordered.** Object members are written in the
///   order given, including members whose keys repeat; whether repeated
///   keys are allowed (Serialization 3.1 `allow-duplicate-names`,
///   SERE0022) is the caller's decision.
/// - **Numbers are the caller's text**, written as given once it is checked
///   against the RFC 8259 §6 grammar ([`WriteError::InvalidNumber`]
///   otherwise). Choosing the lexical form, and refusing values JSON cannot
///   express such as NaN, is the caller's concern.
/// - **Strings** are escaped by the [`EscapeProfile`], optionally with an
///   "is encodable" predicate ([`Writer::with_encodable`]). A string can be
///   written whole ([`Writer::string`], [`Writer::key`]) or in parts
///   ([`Writer::begin_string`] or [`Writer::begin_key`], then any mix of
///   [`Writer::string_text`] and [`Writer::string_verbatim`], then
///   [`Writer::end_string`]); the verbatim parts are written without
///   escaping, for character-map output (Serialization 3.1 §9 and §11).
/// - **Misuse is an error, never invalid output.** Each call checks that it
///   fits the grammar at that point and returns a [`WriteError`] otherwise,
///   writing nothing; [`Writer::finish`] refuses an incomplete text. So the
///   text returned by `finish` is always one JSON text (RFC 8259 §2), with
///   one exception: **verbatim text is outside that guarantee.** It is
///   written exactly as given, and a verbatim quotation mark, reverse
///   solidus or control character can make the result invalid.
/// - **No recursion and no depth limit.** The caller drives the nesting;
///   the writer keeps one small frame per open container on an explicit
///   stack.
///
/// ```
/// use xee_json::{EscapeProfile, Layout, Writer};
///
/// let mut writer = Writer::new(Layout::Compact, EscapeProfile::Serialization);
/// writer.start_object()?;
/// writer.key("a/b")?;
/// writer.start_array()?;
/// writer.number("1.50")?;
/// writer.string("say \"hi\"")?;
/// writer.end_array()?;
/// writer.key("a/b")?;
/// writer.bool(true)?;
/// writer.end_object()?;
/// assert_eq!(
///     writer.finish()?,
///     r#"{"a\/b":[1.50,"say \"hi\""],"a\/b":true}"#
/// );
/// # Ok::<(), xee_json::WriteError>(())
/// ```
pub struct Writer<'a> {
    out: String,
    layout: Layout,
    profile: EscapeProfile,
    encodable: Option<Box<dyn Fn(char) -> bool + 'a>>,
    stack: Vec<Frame>,
    string: Option<StringRole>,
    /// The top-level value has been written completely.
    complete: bool,
}

impl fmt::Debug for Writer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Writer")
            .field("out", &self.out)
            .field("layout", &self.layout)
            .field("profile", &self.profile)
            .field("encodable", &self.encodable.is_some())
            .field("stack", &self.stack)
            .field("string", &self.string)
            .field("complete", &self.complete)
            .finish()
    }
}

impl Writer<'static> {
    /// A writer with the given layout and escaping profile, for an output
    /// encoding that can represent every character (UTF-8, UTF-16).
    pub fn new(layout: Layout, profile: EscapeProfile) -> Self {
        Writer {
            out: String::new(),
            layout,
            profile,
            encodable: None,
            stack: Vec::new(),
            string: None,
            complete: false,
        }
    }
}

impl<'a> Writer<'a> {
    /// Sets the "is encodable" predicate: it returns whether the output
    /// encoding can represent a character. Every string character that the
    /// [`EscapeProfile`] would write as itself is passed to it, and one it
    /// rejects is escaped instead (Serialization 3.1 §9: "Escaping is also
    /// applied to any characters that cannot be represented in the selected
    /// encoding"): as `\uHHHH`, or as a surrogate pair of two such escapes
    /// outside the Basic Multilingual Plane.
    ///
    /// The writer's own output outside string contents, and every escape,
    /// is ASCII: the predicate is not consulted for it and must accept it.
    /// Verbatim text is never passed to the predicate.
    ///
    /// It replaces any earlier predicate and applies to strings written
    /// after the call.
    pub fn with_encodable<'b>(self, encodable: impl Fn(char) -> bool + 'b) -> Writer<'b> {
        Writer {
            out: self.out,
            layout: self.layout,
            profile: self.profile,
            encodable: Some(Box::new(encodable)),
            stack: self.stack,
            string: self.string,
            complete: self.complete,
        }
    }

    /// The layout.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The escaping profile.
    pub fn profile(&self) -> EscapeProfile {
        self.profile
    }

    /// The JSON text, once exactly one complete top-level value has been
    /// written; [`WriteError::Incomplete`] otherwise.
    pub fn finish(self) -> Result<String, WriteError> {
        if self.string.is_some() || !self.stack.is_empty() || !self.complete {
            return Err(WriteError::Incomplete);
        }
        Ok(self.out)
    }

    /// Writes `null`.
    pub fn null(&mut self) -> Result<(), WriteError> {
        self.check_value()?;
        self.scalar("null");
        Ok(())
    }

    /// Writes `true` or `false`.
    pub fn bool(&mut self, value: bool) -> Result<(), WriteError> {
        self.check_value()?;
        self.scalar(if value { "true" } else { "false" });
        Ok(())
    }

    /// Writes a number: `text` exactly as given, if it is exactly one RFC
    /// 8259 §6 number (for example `-0`, `1.50`, `1.0E3`, `1E400`); a
    /// [`WriteError::InvalidNumber`] otherwise.
    pub fn number(&mut self, text: &str) -> Result<(), WriteError> {
        self.check_value()?;
        if !number::is_number(text) {
            return Err(WriteError::InvalidNumber);
        }
        self.scalar(text);
        Ok(())
    }

    /// Writes a string value, escaped by the profile.
    pub fn string(&mut self, text: &str) -> Result<(), WriteError> {
        self.begin_string()?;
        self.push_escaped(text);
        self.end_open_string();
        Ok(())
    }

    /// Writes an object member's key, escaped by the profile. Its value
    /// must follow.
    pub fn key(&mut self, text: &str) -> Result<(), WriteError> {
        self.begin_key()?;
        self.push_escaped(text);
        self.end_open_string();
        Ok(())
    }

    /// Begins a string value, to be written in parts and ended with
    /// [`Writer::end_string`].
    pub fn begin_string(&mut self) -> Result<(), WriteError> {
        self.check_value()?;
        self.begin_value();
        self.out.push('"');
        self.string = Some(StringRole::Value);
        Ok(())
    }

    /// Begins an object member's key, to be written in parts and ended with
    /// [`Writer::end_string`].
    pub fn begin_key(&mut self) -> Result<(), WriteError> {
        if self.string.is_some() {
            return Err(WriteError::StringOpen);
        }
        let depth = self.stack.len();
        let Some(frame) = self.stack.last_mut() else {
            return Err(WriteError::UnexpectedKey);
        };
        if frame.container != Container::Object || frame.awaiting_value {
            return Err(WriteError::UnexpectedKey);
        }
        if frame.has_members {
            self.out.push(',');
        }
        frame.has_members = true;
        frame.awaiting_value = true;
        push_line_break(&mut self.out, self.layout, depth);
        self.out.push('"');
        self.string = Some(StringRole::Key);
        Ok(())
    }

    /// Writes part of the open string or key, escaped by the profile.
    pub fn string_text(&mut self, text: &str) -> Result<(), WriteError> {
        if self.string.is_none() {
            return Err(WriteError::NoStringOpen);
        }
        self.push_escaped(text);
        Ok(())
    }

    /// Writes part of the open string or key **verbatim**: exactly as
    /// given, not escaped and not passed to the "is encodable" predicate.
    ///
    /// This is the path for character-map output, which Serialization 3.1
    /// §9 exempts from escaping. The caller is responsible for the result:
    /// verbatim text is outside the writer's guarantee that the output is
    /// valid JSON.
    pub fn string_verbatim(&mut self, text: &str) -> Result<(), WriteError> {
        if self.string.is_none() {
            return Err(WriteError::NoStringOpen);
        }
        self.out.push_str(text);
        Ok(())
    }

    /// Ends the open string or key.
    pub fn end_string(&mut self) -> Result<(), WriteError> {
        if self.string.is_none() {
            return Err(WriteError::NoStringOpen);
        }
        self.end_open_string();
        Ok(())
    }

    /// Writes `[`.
    pub fn start_array(&mut self) -> Result<(), WriteError> {
        self.start(Container::Array, '[')
    }

    /// Writes `]`, closing the innermost container, which must be an array.
    pub fn end_array(&mut self) -> Result<(), WriteError> {
        self.end(Container::Array, ']')
    }

    /// Writes `{`.
    pub fn start_object(&mut self) -> Result<(), WriteError> {
        self.start(Container::Object, '{')
    }

    /// Writes `}`, closing the innermost container, which must be an object
    /// whose last key has its value.
    pub fn end_object(&mut self) -> Result<(), WriteError> {
        self.end(Container::Object, '}')
    }

    /// Checks that a value may start here.
    fn check_value(&self) -> Result<(), WriteError> {
        if self.string.is_some() {
            return Err(WriteError::StringOpen);
        }
        match self.stack.last() {
            None if self.complete => Err(WriteError::SecondTopLevelValue),
            None => Ok(()),
            Some(frame) => match frame.container {
                Container::Array => Ok(()),
                Container::Object if frame.awaiting_value => Ok(()),
                Container::Object => Err(WriteError::ValueWithoutKey),
            },
        }
    }

    /// Writes what precedes a value that [`Writer::check_value`] accepted:
    /// in an array, the `,` after an earlier element and the line break.
    /// (In an object, the key already wrote them.)
    fn begin_value(&mut self) {
        let depth = self.stack.len();
        let Some(frame) = self.stack.last_mut() else {
            return;
        };
        match frame.container {
            Container::Array => {
                if frame.has_members {
                    self.out.push(',');
                }
                frame.has_members = true;
                push_line_break(&mut self.out, self.layout, depth);
            }
            Container::Object => frame.awaiting_value = false,
        }
    }

    /// Records that a value has ended: at the top level, the text is then
    /// complete.
    fn end_value(&mut self) {
        if self.stack.is_empty() {
            self.complete = true;
        }
    }

    /// Writes a checked scalar token.
    fn scalar(&mut self, text: &str) {
        self.begin_value();
        self.out.push_str(text);
        self.end_value();
    }

    fn start(&mut self, container: Container, bracket: char) -> Result<(), WriteError> {
        self.check_value()?;
        self.begin_value();
        self.out.push(bracket);
        self.stack.push(Frame {
            container,
            has_members: false,
            awaiting_value: false,
        });
        Ok(())
    }

    fn end(&mut self, container: Container, bracket: char) -> Result<(), WriteError> {
        if self.string.is_some() {
            return Err(WriteError::StringOpen);
        }
        let Some(&frame) = self.stack.last() else {
            return Err(WriteError::MismatchedClose);
        };
        if frame.container != container {
            return Err(WriteError::MismatchedClose);
        }
        if frame.awaiting_value {
            return Err(WriteError::KeyWithoutValue);
        }
        self.stack.pop();
        if frame.has_members {
            push_line_break(&mut self.out, self.layout, self.stack.len());
        }
        self.out.push(bracket);
        self.end_value();
        Ok(())
    }

    /// Ends the open string: the closing quote, then `:` after a key or the
    /// end of the value.
    fn end_open_string(&mut self) {
        self.out.push('"');
        match self.string.take() {
            Some(StringRole::Key) => {
                self.out.push(':');
                if self.layout == Layout::Indented {
                    self.out.push(' ');
                }
            }
            Some(StringRole::Value) => self.end_value(),
            None => {}
        }
    }

    /// Appends `text` escaped by the profile.
    fn push_escaped(&mut self, text: &str) {
        let encodable = self.encodable.as_deref();
        match self.profile {
            EscapeProfile::Serialization => {
                for c in text.chars() {
                    push_serialization_char(&mut self.out, c, encodable);
                }
            }
        }
    }
}

/// In [`Layout::Indented`], a line break and the indentation for `depth`
/// open containers; nothing in [`Layout::Compact`].
fn push_line_break(out: &mut String, layout: Layout, depth: usize) {
    if layout == Layout::Indented {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    }
}

/// Appends one string character under [`EscapeProfile::Serialization`].
fn push_serialization_char(out: &mut String, c: char, encodable: Option<&dyn Fn(char) -> bool>) {
    let short = match c {
        '"' => '"',
        '\\' => '\\',
        '/' => '/',
        '\u{8}' => 'b',
        '\u{C}' => 'f',
        '\n' => 'n',
        '\r' => 'r',
        '\t' => 't',
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' => {
            push_unicode_escapes(out, c);
            return;
        }
        _ if encodable.is_some_and(|encodable| !encodable(c)) => {
            push_unicode_escapes(out, c);
            return;
        }
        _ => {
            out.push(c);
            return;
        }
    };
    out.push('\\');
    out.push(short);
}

/// Appends `c` as six-character escapes of its UTF-16 code units: one
/// escape in the Basic Multilingual Plane, a surrogate pair of two outside
/// it.
fn push_unicode_escapes(out: &mut String, c: char) {
    let mut units = [0u16; 2];
    for &unit in c.encode_utf16(&mut units).iter() {
        out.push('\\');
        out.push('u');
        for shift in [12, 8, 4, 0] {
            let digit = char::from_digit(u32::from((unit >> shift) & 0xF), 16).unwrap_or('0');
            out.push(digit.to_ascii_uppercase());
        }
    }
}
