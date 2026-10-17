//! XPath 3.1 regular expressions: the dialect of F&O 3.1 §5.6 over the
//! grammar of XSD 1.1 Part 2 Appendix G, written from those texts
//! ([`Regex::new`]); and XSD 1.1's own dialect, the one of an
//! `xs:pattern` facet ([`Regex::xsd`]): Appendix G alone, without flags,
//! anchored at both ends, `^` and `$` normal characters, an unknown block
//! name every character, which [`Regex::unknown_blocks`] lists.
//!
//! A pattern is parsed, its character classes resolved to sets of Unicode
//! scalar values (the data generated from the Unicode Character Database,
//! at the version of the standard library this crate is built with), and
//! compiled to one program over characters. Parsing and compiling are
//! bounded: the pattern's length, its nesting, the nodes and character sets
//! it builds and the program's size each have a ceiling. A pattern over
//! one of the first four is refused with [`Error::Limit`]; one whose
//! program would be over a ceiling is kept uncompiled, and a call that
//! needs its program is refused with [`Error::Limit`].
//!
//! The operations of F&O 3.1 §5.6.3 to §5.6.6 run the program on a
//! PikeVM, each search linear in the input, or, when it has
//! back-references, on a backtracking matcher; the two give the same
//! answers. Each takes a [`Budget`] of steps, which every
//! search the call makes and everything it builds from a match are charged
//! to before they are done; a call that would spend more is refused with
//! [`Error::Limit`], as is one that would hold more memory than a ceiling
//! of its own allows.

mod backtrack;
mod charset;
mod compile;
mod ops;
mod parse;
mod pike;
mod prefilter;
#[rustfmt::skip]
mod tables;
mod unicode;

use std::fmt;

use backtrack::Slots;
use compile::Program;
pub use ops::{AnalyzeEntry, MatchEntry};

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

/// Why a pattern, its flags or a call are refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// FORX0001: the flags are not a string of `s`, `m`, `i`, `x` and `q`.
    InvalidFlags(String),
    /// FORX0002: the pattern is not a valid regular expression of its
    /// dialect, XPath's ([`Regex::new`]) or XSD's ([`Regex::xsd`]).
    Syntax(String),
    /// FORX0003: `fn:replace`, `fn:tokenize` or `fn:analyze-string` with a
    /// pattern that matches the empty string.
    MatchesEmptyString,
    /// FORX0004: a replacement string with a `$` not followed by a digit,
    /// or a `\` not followed by `\` or `$`; it says which, and where.
    InvalidReplacement(String),
    /// An implementation limit: the pattern is too long, nests too deeply,
    /// or builds too many nodes, character sets or instructions; or a call
    /// spends its budget or would hold more memory than a ceiling allows.
    Limit(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidFlags(flags) => write!(f, "invalid regular expression flags {flags:?}"),
            Error::Syntax(msg) => write!(f, "invalid regular expression: {msg}"),
            Error::MatchesEmptyString => {
                write!(f, "the regular expression matches the empty string")
            }
            Error::InvalidReplacement(why) => write!(f, "invalid replacement string: {why}"),
            Error::Limit(msg) => write!(f, "regular expression refused: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// Ceiling: the bytes one call holds for what it builds, counted as they
/// are reserved, before they are allocated, at what the allocator takes
/// for them ([`chunk`]): its result, the parsed replacement string, and
/// the matcher's stack and scratch memory. A match's slots and the group
/// tree's list of groups are freed with the match and as large as the
/// pattern's groups, and are not counted. Steps bound time, not memory: a
/// result's item or a stack frame costs a step and tens of bytes. 2^27
/// bytes are 128 MiB.
///
/// It bounds the chunks a call holds, not the process's resident memory:
/// glibc sometimes hands over sixteen bytes more than [`chunk`] counts
/// (a call measured at 1.016 times the ceiling), and once a process has
/// freed a large block, it raises the size from which it maps a block
/// apart, so a growing result can leave its last heap buffer resident:
/// up to that size, 32 MiB at most (a warm process measured at 1.19
/// times).
pub(crate) const MAX_CALL_BYTES: usize = 1 << 27;

/// What an allocation of `bytes` takes from the allocator: glibc's malloc
/// adds an eight-byte header, rounds up to sixteen, and gives no chunk
/// under 32 bytes, so a one-byte string takes 32. Other allocators round
/// differently, by as much. This is the size [`Regex::heap_bytes`] counts
/// each allocation at, for a caller that counts what it holds beside a
/// compiled pattern the same way.
pub const fn chunk(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        let rounded = bytes.saturating_add(8 + 15) / 16 * 16;
        if rounded < 32 {
            32
        } else {
            rounded
        }
    }
}

/// The steps one call may take. A step is an instruction a matcher visits,
/// or a position the PikeVM's search reaches; each of these costs one
/// more: setting up, saving or clearing a capture slot or a progress
/// register (the PikeVM copies or clears a row of slots at a step per
/// eight), a byte of the text a back-reference compares, a byte of the
/// replacement string, and a byte of a result and a part of it built for
/// each match; a character class costs one more for each doubling of its
/// ranges, which its test searches. Work is charged before it is done.
///
/// The caller decides what a budget covers; F&O's operations each take
/// one, so one call cannot spend another's. The memory a call holds has a
/// ceiling of its own, per call whatever its budget; what the calls return
/// is the caller's to bound across them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    limit: u64,
    left: u64,
    /// The bytes the current call holds, and may hold.
    held: usize,
    memory: usize,
    /// Whether a charge was refused.
    over: bool,
}

impl Budget {
    pub fn new(steps: u64) -> Budget {
        Budget::with_memory(steps, MAX_CALL_BYTES)
    }

    /// A budget whose calls may hold at most `bytes`, for tests of the
    /// ceiling at sizes a test can reach.
    #[doc(hidden)]
    pub fn with_memory(steps: u64, bytes: usize) -> Budget {
        Budget {
            limit: steps,
            left: steps,
            held: 0,
            memory: bytes,
            over: false,
        }
    }

    /// The steps spent so far; all of them once the call was refused at a
    /// charge.
    pub fn spent(&self) -> u64 {
        self.limit - self.left
    }

    /// Whether a charge was refused: a call would have taken more than the
    /// steps left. A call that spends its last step and is then refused for
    /// its memory has spent all of them too, so [`Budget::spent`] does not
    /// tell the two refusals apart; on a budget made for one call, this
    /// does. A budget's steps are spent across every call it covers, and
    /// this stays set for its later calls as they stay spent, whether
    /// those calls are refused or answer.
    pub fn over_budget(&self) -> bool {
        self.over
    }

    pub(crate) fn charge(&mut self, steps: u64) -> Result<(), Error> {
        if self.left < steps {
            self.left = 0;
            self.over = true;
            return Err(Error::Limit(format!(
                "the call would take more than its budget of {} steps",
                self.limit
            )));
        }
        self.left -= steps;
        Ok(())
    }

    /// Reserves `bytes` more of the call's memory.
    pub(crate) fn hold(&mut self, bytes: usize) -> Result<(), Error> {
        match self.held.checked_add(bytes) {
            Some(held) if held <= self.memory => {
                self.held = held;
                Ok(())
            }
            _ => Err(Error::Limit(format!(
                "the call would hold more than {} bytes of memory",
                self.memory
            ))),
        }
    }
}

/// Makes room for `more` items in `v`, at least doubling its capacity as
/// `Vec` does, with the bytes the growth takes held first.
pub(crate) fn reserve<T>(v: &mut Vec<T>, more: usize, budget: &mut Budget) -> Result<(), Error> {
    let need = v.len().saturating_add(more);
    if need > v.capacity() {
        let cap = need.max(v.capacity().saturating_mul(2));
        let size = size_of::<T>();
        budget.hold(chunk(cap.saturating_mul(size)) - chunk(v.capacity() * size))?;
        v.reserve_exact(cap - v.len());
    }
    Ok(())
}

/// [`reserve`] for the bytes of a string.
pub(crate) fn reserve_str(s: &mut String, more: usize, budget: &mut Budget) -> Result<(), Error> {
    let need = s.len().saturating_add(more);
    if need > s.capacity() {
        let cap = need.max(s.capacity().saturating_mul(2));
        budget.hold(chunk(cap) - chunk(s.capacity()))?;
        s.reserve_exact(cap - s.len());
    }
    Ok(())
}

/// A copy of `text`, its allocation held first.
pub(crate) fn copy(text: &str, budget: &mut Budget) -> Result<String, Error> {
    budget.hold(chunk(text.len()))?;
    Ok(text.to_string())
}

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

/// A compiled regular expression, of the XPath dialect ([`Regex::new`])
/// or of XSD's ([`Regex::xsd`]).
#[derive(Debug)]
pub struct Regex {
    flags: Flags,
    group_count: usize,
    /// `group_parent[n]` is the capturing group that most closely encloses
    /// group `n`, or 0.
    group_parent: Vec<usize>,
    /// The length in characters of the shortest string the pattern
    /// matches: an input shorter than it does not match, whether or not
    /// the program was compiled.
    min_len: u64,
    /// `None` when the program is over a ceiling: such a pattern can only
    /// answer that an input too short for it does not match.
    program: Option<Program>,
    /// The XSD dialect's unknown block names, each once, sorted.
    unknown_blocks: Vec<String>,
    matcher: Matcher,
    /// What a search rules out before a matcher runs.
    prefilter: prefilter::Prefilter,
    prefilter_on: bool,
}

/// Which matcher runs a program without back-references.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matcher {
    /// The PikeVM; a program with back-references runs on the backtracker.
    Pike,
    /// The backtracker, for tests that compare the two.
    Backtrack,
}

/// What a call needs from each match: whether there is one, its bounds,
/// or every group's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Need {
    Exists,
    Bounds,
    Groups,
}

/// One call's budget and the matchers' scratch memory.
struct Call<'b> {
    budget: &'b mut Budget,
    need: Need,
    backtrack: Option<backtrack::Cache>,
    pike: Option<pike::Cache>,
}

impl<'b> Call<'b> {
    /// A call starts holding no memory, whatever its budget held before.
    fn new(budget: &'b mut Budget, need: Need) -> Call<'b> {
        budget.held = 0;
        Call {
            budget,
            need,
            backtrack: None,
            pike: None,
        }
    }
}

impl Regex {
    /// Parses and compiles `pattern` in the XPath 3.1 dialect with F&O 3.1
    /// §5.6.2's `flags`.
    ///
    /// A pattern whose program would be over a ceiling, of instructions or
    /// of nodes compiled, is accepted, so that it answers that an input
    /// shorter than its minimum length does not match
    /// (`matches('aaa', 'a{2147483647}')` is false); a call that needs its
    /// program is refused with [`Error::Limit`]. One that ends before it
    /// needs the program ends as it would otherwise: FORX0003
    /// ([`Error::MatchesEmptyString`]) for a pattern that matches the empty
    /// string, FORX0004 ([`Error::InvalidReplacement`]) for a replacement
    /// string that is not one, and [`tokenize`](Regex::tokenize) of the
    /// empty string.
    ///
    /// Fails with [`Error::InvalidFlags`] for flags outside F&O 3.1
    /// §5.6.2, [`Error::Syntax`] for a pattern that is not one of the
    /// dialect, and [`Error::Limit`] for one over a ceiling of parsing: its
    /// length, its nesting, or the nodes or character sets it builds.
    pub fn new(pattern: &str, flags: &str) -> Result<Regex, Error> {
        let flags = Flags::parse(flags)?;
        let parsed = parse::parse(pattern, &flags)?;
        Ok(Regex::build(parsed, flags))
    }

    /// Parses and compiles `pattern` in XSD 1.1's regular-expression
    /// dialect (XSD 1.1 Part 2 Appendix G), the one of an `xs:pattern`
    /// facet, which has no flags.
    ///
    /// The pattern is anchored at both ends, as Appendix G's expressions
    /// are implicitly: it matches a string only as a whole, every branch of
    /// a top-level `|` included (`a|b` does not match `ax`), so that
    /// [`is_match`](Regex::is_match) tests a value against a facet; the
    /// other operations run on it too. Its groups are its own parentheses,
    /// numbered as written. `^` and `$` are normal characters, and none of
    /// F&O 3.1 §5.6.1's additions is part of it: reluctant quantifiers,
    /// non-capturing groups `(?:`, back-references and the escape `\$` are
    /// syntax errors. `.` is every character but #xA and #xD, and nothing
    /// is case-blind. `\p{IsX}` and `\P{IsX}` with a well-formed but
    /// unrecognized block name X each match every character (§G.4.2.4),
    /// where the XPath dialect refuses them; [`unknown_blocks`](Regex::unknown_blocks)
    /// lists those names. A range whose end precedes its start is refused,
    /// as in the XPath dialect (XSD 1.0 §F.1). Without back-references a
    /// pattern of this dialect always runs on the PikeVM. The ceilings are
    /// those of [`Regex::new`], and so is a program over one: kept
    /// uncompiled, and refused with [`Error::Limit`] by a call that needs
    /// it, as there.
    ///
    /// Fails with [`Error::Syntax`] for a pattern that is not one of the
    /// dialect, and [`Error::Limit`] for one over a ceiling of parsing;
    /// never with [`Error::InvalidFlags`], since it takes no flags.
    pub fn xsd(pattern: &str) -> Result<Regex, Error> {
        let parsed = parse::parse_xsd(pattern)?;
        Ok(Regex::build(parsed, Flags::default()))
    }

    fn build(mut parsed: parse::Parsed, flags: Flags) -> Regex {
        let analysis = compile::analyse(&parsed.node);
        // Read off the tree before compiling moves its character sets out.
        let prefilter = prefilter::analyse(&parsed.node, &flags);
        let program = compile::compile(&mut parsed.node, parsed.group_count, &analysis).ok();
        Regex {
            flags,
            group_count: parsed.group_count,
            group_parent: parsed.group_parent,
            min_len: analysis.min_len,
            program,
            unknown_blocks: parsed.unknown_blocks,
            matcher: Matcher::Pike,
            prefilter,
            prefilter_on: true,
        }
    }

    /// Runs programs without back-references on `matcher`, for tests
    /// that compare the two.
    #[doc(hidden)]
    pub fn set_matcher(&mut self, matcher: Matcher) {
        self.matcher = matcher;
    }

    /// Turns the prefilter off, for tests that compare the answers with
    /// it and without it.
    #[doc(hidden)]
    pub fn set_prefilter(&mut self, on: bool) {
        self.prefilter_on = on;
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

    /// The unknown block names an XSD-dialect pattern ([`Regex::xsd`])
    /// uses, each X of a `\p{IsX}` or `\P{IsX}` that names no block this
    /// crate knows (the Unicode blocks of [`UNICODE_VERSION`], and XSD 1.1
    /// §G.4.2.3's three Unicode 3.1 names), without the `Is`: each once,
    /// in the order of their UTF-8 bytes. Each such escape matches every
    /// character, and XSD 1.1 §G.4.2.4 asks that a processor "should issue
    /// a warning if they encounter a regular expression using a block name
    /// they do not recognize": the names are for that warning. Always empty
    /// for a pattern of the XPath dialect ([`Regex::new`]), which refuses
    /// an unknown block name (F&O 3.1 §5.6.1.5, FORX0002).
    pub fn unknown_blocks(&self) -> &[String] {
        &self.unknown_blocks
    }

    /// Whether the program fits the compile ceilings, of instructions and
    /// of nodes compiled.
    pub fn is_compiled(&self) -> bool {
        self.program.is_some()
    }

    /// The bytes the compiled pattern keeps on the heap, for a caller that
    /// bounds what it holds of them, such as a cache of compiled patterns.
    /// It counts every heap allocation a `Regex` keeps: the program's
    /// instructions, its character sets and each set's ranges, the sets'
    /// costs and the instructions' rows in the thread lists, each group's
    /// parent, the prefilter's set and prefix, and the unknown block names
    /// and their list; each at what glibc's
    /// allocator takes for it (an eight-byte header, rounded up to sixteen,
    /// at least 32 bytes), which is at least the size asked for. Not the
    /// `Regex` value itself, which its owner holds, nor what parsing and
    /// compiling held for a while and freed, which the compile ceilings
    /// bound. A pattern over a compile ceiling keeps no program.
    pub fn heap_bytes(&self) -> usize {
        chunk(self.group_parent.capacity() * size_of::<usize>())
            + self.program.as_ref().map_or(0, Program::heap_bytes)
            + self
                .prefilter
                .required
                .as_ref()
                .map_or(0, charset::CharSet::heap_bytes)
            + chunk(self.prefilter.prefix.capacity())
            + chunk(self.unknown_blocks.capacity() * size_of::<String>())
            + self
                .unknown_blocks
                .iter()
                .map(|name| chunk(name.capacity()))
                .sum::<usize>()
    }

    /// The compiled program as a listing, one instruction per line, for
    /// tests and debugging; its format is not stable.
    #[doc(hidden)]
    pub fn program_listing(&self) -> Option<String> {
        self.program.as_ref().map(|p| p.to_string())
    }

    /// The leftmost match starting at or after byte offset `start`, which
    /// is on a character boundary.
    fn find(&self, call: &mut Call, hay: &str, start: usize) -> Result<Option<Slots>, Error> {
        // A character is at least one byte: a bound no search pays for.
        if self.min_len > (hay.len() - start) as u64 {
            return Ok(None);
        }
        let Some(prog) = &self.program else {
            // Counting the characters left reads their bytes.
            call.budget.charge((hay.len() - start) as u64)?;
            if self.min_len > hay[start..].chars().count() as u64 {
                return Ok(None);
            }
            return Err(Error::Limit(
                "the pattern's program is over a ceiling, and the input is long enough for it"
                    .into(),
            ));
        };
        // An anchored pattern matches nowhere past the start, and there
        // only if the text begins with its literal, which comparing reads
        // a step a byte of.
        let anchored = self.prefilter_on && self.prefilter.anchored;
        if anchored {
            let literal = self.prefilter.prefix.as_str();
            if start > 0 {
                return Ok(None);
            }
            call.budget.charge(literal.len() as u64)?;
            if !hay.starts_with(literal) {
                return Ok(None);
            }
        }
        // A text without a character of the required set has no match. The
        // scan stops at the first; a match holds one and so ends past it,
        // and the next search starts there: a call reads each byte once,
        // and is charged at most twice what it reads.
        if let (true, Some(set)) = (self.prefilter_on, &self.prefilter.required) {
            if prefilter::find_any(hay, start, set, call.budget)?.is_none() {
                return Ok(None);
            }
        }
        // The literal's scan skips to where a match could begin; an
        // anchored pattern's was compared above.
        let prefix = if self.prefilter_on && !anchored {
            self.prefilter.prefix.as_str()
        } else {
            ""
        };
        if prog.has_backrefs || self.matcher == Matcher::Backtrack {
            let cache = match &mut call.backtrack {
                Some(cache) => cache,
                None => call
                    .backtrack
                    .insert(backtrack::Cache::new(prog, call.budget)?),
            };
            return backtrack::search(
                prog,
                hay,
                start,
                prefix,
                anchored,
                &self.flags,
                call.budget,
                cache,
            );
        }
        let width = match call.need {
            Need::Exists => 0,
            Need::Bounds => 2,
            Need::Groups => prog.slots,
        };
        let cache = match &mut call.pike {
            Some(cache) => cache,
            None => call
                .pike
                .insert(pike::Cache::new(prog, width, call.budget)?),
        };
        if !pike::search(
            prog,
            hay,
            start,
            prefix,
            anchored,
            &self.flags,
            call.budget,
            cache,
        )? {
            return Ok(None);
        }
        // The slots the call tracked, the others unset.
        call.budget.charge(prog.slots as u64)?;
        let mut slots: Slots = vec![None; prog.slots];
        for (slot, &at) in slots.iter_mut().zip(cache.found()) {
            *slot = (at != pike::NONE).then_some(at);
        }
        Ok(Some(slots))
    }

    /// The slots of each successive match, for tests that compare the
    /// matchers.
    #[doc(hidden)]
    pub fn match_slots(
        &self,
        hay: &str,
        budget: &mut Budget,
    ) -> Result<Vec<Vec<Option<usize>>>, Error> {
        let mut call = Call::new(budget, Need::Groups);
        let mut out = Vec::new();
        self.for_each_match(&mut call, hay, |slots, budget| {
            budget.charge(slots.len() as u64)?;
            reserve(&mut out, 1, budget)?;
            budget.hold(chunk(slots.len() * size_of::<Option<usize>>()))?;
            out.push(slots.clone());
            Ok(())
        })?;
        Ok(out)
    }

    /// Whether the pattern matches the empty string (FORX0003), which needs
    /// no search: on the empty input only a character class can fail, since
    /// `^` and `$` hold there in every mode and a back-reference compares
    /// an empty capture, so the pattern matches it exactly when some path
    /// through it meets no class, when its minimum length is zero.
    fn matches_empty(&self) -> bool {
        self.min_len == 0
    }
}

#[cfg(test)]
mod tests;
