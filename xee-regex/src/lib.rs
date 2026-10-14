//! XPath 3.1 regular expressions: the dialect of F&O 3.1 §5.6 over the
//! grammar of XSD 1.1 Part 2 Appendix G, written from those texts.
//!
//! A pattern is parsed, its character classes resolved to sets of Unicode
//! scalar values (the data generated from the Unicode Character Database,
//! at the version of the standard library this crate is built with), and
//! compiled to one program over characters. Parsing and compiling are
//! bounded: the pattern's length, its nesting, the nodes and character sets
//! it builds and the program's size each have a ceiling, past which the
//! pattern is refused with [`Error::Limit`].

mod charset;
mod compile;
mod parse;
#[rustfmt::skip]
mod tables;
mod unicode;

use std::fmt;

use compile::Program;

/// The version of Unicode the character data comes from: categories,
/// blocks and case variants.
pub const UNICODE_VERSION: (u8, u8, u8) = tables::UNICODE_VERSION;

// Flag `i` takes its case variants from the tables, `fn:lower-case` and
// `fn:upper-case` from the standard library (F&O 3.1 §5.6.2 defines the
// first through the second): a toolchain whose Unicode differs does not
// build this crate until the tables are regenerated for it
// (`tests/generate_tables.rs`).
const _: () = assert!(
    UNICODE_VERSION.0 == char::UNICODE_VERSION.0
        && UNICODE_VERSION.1 == char::UNICODE_VERSION.1
        && UNICODE_VERSION.2 == char::UNICODE_VERSION.2,
    "xee-regex's Unicode tables are for another Unicode version than the toolchain's: regenerate src/tables.rs"
);

/// Why a pattern or its flags are refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// FORX0001: the flags are not a string of `s`, `m`, `i`, `x` and `q`.
    InvalidFlags(String),
    /// FORX0002: the pattern is not a valid regular expression.
    Syntax(String),
    /// An implementation limit: the pattern is too long, nests too deeply,
    /// or builds too many nodes, character sets or instructions.
    Limit(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidFlags(flags) => write!(f, "invalid regular expression flags {flags:?}"),
            Error::Syntax(msg) => write!(f, "invalid regular expression: {msg}"),
            Error::Limit(msg) => write!(f, "regular expression refused: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// F&O 3.1 §5.6.2's flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    pub(crate) s: bool,
    pub(crate) m: bool,
    pub(crate) i: bool,
    pub(crate) x: bool,
    pub(crate) q: bool,
}

impl Flags {
    fn parse(flags: &str) -> Result<Flags, Error> {
        let mut f = Flags::default();
        for c in flags.chars() {
            match c {
                's' => f.s = true,
                'm' => f.m = true,
                'i' => f.i = true,
                'x' => f.x = true,
                'q' => f.q = true,
                _ => return Err(Error::InvalidFlags(flags.to_string())),
            }
        }
        if f.q {
            // §5.6.2: with `q`, the `m`, `s` and `x` flags have no effect.
            f.m = false;
            f.s = false;
            f.x = false;
        }
        Ok(f)
    }
}

/// A compiled XPath regular expression.
#[derive(Debug)]
pub struct Regex {
    #[expect(
        dead_code,
        reason = "read by the matchers, which come in a later change"
    )]
    flags: Flags,
    group_count: usize,
    /// The length in characters of the shortest string the pattern
    /// matches: an input shorter than it does not match, whether or not
    /// the program was compiled.
    min_len: u64,
    /// `None` when the program is over a ceiling: such a pattern can only
    /// answer that an input too short for it does not match.
    program: Option<Program>,
}

impl Regex {
    /// Parses and compiles `pattern` in the XPath 3.1 dialect with F&O 3.1
    /// §5.6.2's `flags`.
    ///
    /// A pattern whose program would be over the instruction ceiling is
    /// accepted, so that it answers that an input shorter than its minimum
    /// length does not match (`matches('aaa', 'a{2147483647}')` is false);
    /// any other use of it is refused.
    pub fn new(pattern: &str, flags: &str) -> Result<Regex, Error> {
        let flags = Flags::parse(flags)?;
        let mut parsed = parse::parse(pattern, &flags)?;
        let analysis = compile::analyse(&parsed.node);
        let program = compile::compile(&mut parsed.node, parsed.group_count, &analysis).ok();
        Ok(Regex {
            flags,
            group_count: parsed.group_count,
            min_len: analysis.min_len,
            program,
        })
    }

    /// The number of capturing groups.
    pub fn group_count(&self) -> usize {
        self.group_count
    }

    /// The length in characters of the shortest string the pattern
    /// matches, saturating at `u64::MAX`.
    pub fn min_len(&self) -> u64 {
        self.min_len
    }

    /// Whether the program fits the instruction ceiling.
    pub fn is_compiled(&self) -> bool {
        self.program.is_some()
    }

    /// The compiled program as a listing, one instruction per line, for
    /// tests and debugging; its format is not stable.
    #[doc(hidden)]
    pub fn program_listing(&self) -> Option<String> {
        self.program.as_ref().map(|p| p.to_string())
    }
}

#[cfg(test)]
mod tests;
