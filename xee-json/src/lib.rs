#![warn(missing_docs)]
#![forbid(unsafe_code)]

//! A strict JSON pull tokenizer and a JSON writer.
//!
//! # Tokenizer
//!
//! [`Tokenizer`] reads a JSON text ([RFC 8259] §2) from a `&str` and yields
//! [`Event`]s in document order. It is an [`Iterator`] of
//! `Result<Event, Error>`:
//!
//! - **Document order, nothing merged.** Every object member is delivered as
//!   a [`Event::Key`] followed by its value, in source order, including
//!   members whose keys repeat. What a repeated key means is the consumer's
//!   decision (F&O 3.1 §17.5.1 `duplicates`, for example).
//! - **Strings borrow.** A string or key without escape sequences arrives as
//!   [`Str::Plain`], a slice of the input. One with escapes arrives as
//!   [`Str::Escaped`]: the validated source text between the quotes, decoded
//!   lazily by [`Escaped::segments`]. Each escape keeps its source text, and
//!   an escaped surrogate that is not part of a pair is delivered as
//!   [`Decoded::LoneSurrogate`] rather than rejected or replaced (RFC 8259
//!   §8.2 leaves that behaviour unpredictable; F&O 3.1 §17.5.1 requires such
//!   characters to reach its `fallback` function). The tokenizer therefore
//!   never promises a decoded Rust `String`: each consumer states its own
//!   policy for a lone surrogate.
//! - **Numbers are text.** [`Event::Number`] carries the lexical form,
//!   validated against the RFC 8259 §6 grammar. Range and precision are the
//!   consumer's concern (RFC 8259 §9).
//! - **Bounded nesting.** Containers are tracked on an explicit stack, never
//!   by recursion, and nesting beyond [`Options::max_depth`] is an error
//!   (RFC 8259 §9 permits the limit). The default is [`DEFAULT_MAX_DEPTH`];
//!   there is no unbounded setting.
//! - **Byte order mark.** A leading U+FEFF is rejected or skipped according
//!   to [`BomPolicy`] (RFC 8259 §8.1 allows a parser to ignore it).
//! - **Errors.** Every [`Error`] has an [`ErrorKind`] and the byte offset of
//!   the first offending byte in the original input (the input's length when
//!   the text ends too early).
//!
//! The tokenizer is fused: after it yields an error, or after the end of the
//! text, every later call to `next` returns `None`. A text is valid only when
//! the iterator has been drained to `None` without an error: trailing content
//! after the top-level value is reported after that value's events.
//!
//! ```
//! use xee_json::{Event, Str, Tokenizer};
//!
//! let events: Vec<Event> = Tokenizer::new(r#"{"a": [1, "x"]}"#)
//!     .collect::<Result<_, _>>()
//!     .unwrap();
//! assert_eq!(
//!     events,
//!     [
//!         Event::StartObject,
//!         Event::Key(Str::Plain("a")),
//!         Event::StartArray,
//!         Event::Number("1"),
//!         Event::String(Str::Plain("x")),
//!         Event::EndArray,
//!         Event::EndObject,
//!     ]
//! );
//! ```
//!
//! # Writer
//!
//! [`Writer`] produces a JSON text from calls made in document order, one
//! per token, and returns it as a `String`:
//!
//! - **Ordered members.** Keys are written in the order given, and a
//!   repeated key is written again: nothing is merged.
//! - **Number text from the caller**, checked against the RFC 8259 §6
//!   grammar and written as given.
//! - **Escaping by a named profile**, [`EscapeProfile::Serialization`]: the
//!   JSON output method of Serialization 3.1 §9, which escapes the solidus
//!   and U+007F to U+009F as well as what RFC 8259 §7 requires, with
//!   upper-case hexadecimal digits.
//! - **An "is encodable" predicate** ([`Writer::with_encodable`]): a string
//!   character the output encoding cannot represent is escaped (outside
//!   the Basic Multilingual Plane, as a surrogate pair of escapes).
//! - **Verbatim text** ([`Writer::string_verbatim`]) inside a string, for
//!   character-map output, which is written unescaped.
//! - **[`Layout::Compact`]** (no whitespace outside strings) or
//!   **[`Layout::Indented`]**.
//! - **Misuse is refused** with a [`WriteError`] and writes nothing, so the
//!   finished text is always one valid JSON text, verbatim text aside.
//!
//! What the tokenizer reads back from the writer's output (verbatim text
//! aside) is the same sequence of events, with every string decoding to the
//! text that was written.
//!
//! [RFC 8259]: https://www.rfc-editor.org/rfc/rfc8259

mod error;
mod number;
mod string;
mod tokenizer;
mod writer;

pub use error::{Error, ErrorKind};
pub use string::{Decoded, Escaped, Segment, Segments, Str};
pub use tokenizer::{BomPolicy, Event, Options, Tokenizer, DEFAULT_MAX_DEPTH};
pub use writer::{EscapeProfile, Layout, WriteError, Writer};
