//! Tokenizer errors: a kind and a byte offset.

use std::fmt;

/// What went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The text ended where the grammar requires more: an empty text, an
    /// unclosed container or string, or an incomplete number, literal or
    /// escape. The offset is the input's length.
    UnexpectedEof,
    /// A character that cannot appear here: for example a missing `,` or
    /// `:`, a closing bracket that does not match, a key that is not a
    /// string, or whitespace other than space, tab, line feed and carriage
    /// return (RFC 8259 §2).
    UnexpectedChar,
    /// A number that does not match the RFC 8259 §6 grammar: a leading
    /// zero, or a `-`, `.` or exponent not followed by a digit.
    InvalidNumber,
    /// A misspelled `true`, `false` or `null`. The offset is the first byte
    /// that differs.
    InvalidLiteral,
    /// A backslash not followed by one of `" \ / b f n r t u`, or `\u` not
    /// followed by four hexadecimal digits (RFC 8259 §7). The offset is the
    /// first byte that does not fit.
    InvalidEscape,
    /// An unescaped character in U+0000 to U+001F inside a string (RFC 8259
    /// §7 requires them to be escaped).
    ControlCharInString,
    /// A `[` or `{` that would nest deeper than [`Options::max_depth`]. The
    /// offset is that bracket's.
    ///
    /// [`Options::max_depth`]: crate::Options::max_depth
    DepthExceeded,
    /// Something other than whitespace after the top-level value.
    TrailingContent,
    /// A leading byte order mark under [`BomPolicy::Reject`]. The offset is
    /// 0.
    ///
    /// [`BomPolicy::Reject`]: crate::BomPolicy::Reject
    BomNotAllowed,
}

impl ErrorKind {
    fn description(self) -> &'static str {
        match self {
            ErrorKind::UnexpectedEof => "unexpected end of JSON text",
            ErrorKind::UnexpectedChar => "unexpected character",
            ErrorKind::InvalidNumber => "invalid number",
            ErrorKind::InvalidLiteral => "invalid literal",
            ErrorKind::InvalidEscape => "invalid escape sequence",
            ErrorKind::ControlCharInString => "unescaped control character in string",
            ErrorKind::DepthExceeded => "maximum nesting depth exceeded",
            ErrorKind::TrailingContent => "trailing content after the JSON value",
            ErrorKind::BomNotAllowed => "byte order mark not allowed",
        }
    }
}

/// A tokenizer error: its kind and the byte offset in the original input.
///
/// The offset counts from the start of the input as given, including a
/// byte order mark skipped under [`BomPolicy::Ignore`]. It always lies on a
/// character boundary.
///
/// [`BomPolicy::Ignore`]: crate::BomPolicy::Ignore
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Error {
    kind: ErrorKind,
    offset: usize,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, offset: usize) -> Self {
        Error { kind, offset }
    }

    /// What went wrong.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The byte offset of the first offending byte, or the input's length
    /// when the text ended too early.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at byte offset {}",
            self.kind.description(),
            self.offset
        )
    }
}

impl std::error::Error for Error {}
