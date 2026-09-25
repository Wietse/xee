#![warn(missing_docs)]
#![forbid(unsafe_code)]

//! A strict JSON pull tokenizer.
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
//! [RFC 8259]: https://www.rfc-editor.org/rfc/rfc8259

mod error;
mod string;
mod tokenizer;

pub use error::{Error, ErrorKind};
pub use string::{Decoded, Escaped, Segment, Segments, Str};
pub use tokenizer::{BomPolicy, Event, Options, Tokenizer, DEFAULT_MAX_DEPTH};
