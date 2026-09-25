//! The checks behind the two fuzz targets, as a library so that the replay
//! tests in `tests/replay.rs` run them on stable over JSONTestSuite and the
//! hand-written seeds.
//!
//! - [`robustness`]: the tokenizer never panics; every drain is bounded and
//!   fused after an error; an error is `UnexpectedEof` exactly when its
//!   offset is the input's length, and otherwise the input cut just after
//!   the offending character gives the same items; a successful drain is
//!   one balanced JSON value whose strings satisfy the segment invariants
//!   and whose numbers are whole slices of the input; the open depth never
//!   exceeds the configured maximum; and the options change results only
//!   in the ways they are documented to.
//! - [`differential`]: xee-json against serde_json, an independent RFC 8259
//!   parser. Accept/reject must agree with serde_json's `IgnoredAny` path;
//!   where both accept, the structure, member order, decoded strings and
//!   number text must equal serde_json's `Value`. Every known difference is
//!   an entry of [`DIVERGENCES`] with its reason; the harness removes the
//!   entry's cause and then requires agreement, and any other difference
//!   panics with the input and both results. Where both reject, serde_json
//!   also witnesses the error's position ([`judge_rejection`]). A last leg
//!   re-runs an accepted text one level below its own depth
//!   ([`try_lower_depth`]).
//!
//! Every drain of the tokenizer goes through the bounded helpers of
//! `xee-json/tests/common` (at most `input.len() + 1` items, the bound
//! asserted), so a tokenizer that stops being fused fails a check instead of
//! growing without limit.

use std::collections::{BTreeSet, HashMap};

use serde::de::IgnoredAny;
use serde_json::Value;
use xee_json::{BomPolicy, Error, ErrorKind, Event, Options, Str, DEFAULT_MAX_DEPTH};

#[path = "../../xee-json/tests/common/mod.rs"]
mod common;

/// One item of a tokenizer drain.
pub type Item<'a> = Result<Event<'a>, Error>;

/// A number event: non-empty number characters, and a whole number of the
/// input (see [`check_drain`]). Public so that the tests can hand it a
/// slice a tokenizer should not deliver.
pub fn check_number(text: &str, number: &str) {
    assert!(
        !number.is_empty()
            && number
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')),
        "{text:?}: number {number:?}"
    );
    let start = (number.as_ptr() as usize)
        .checked_sub(text.as_ptr() as usize)
        .filter(|start| start + number.len() <= text.len())
        .unwrap_or_else(|| panic!("{text:?}: number {number:?} is not a slice of the input"));
    let bytes = text.as_bytes();
    let before = start.checked_sub(1).and_then(|at| bytes.get(at));
    let after = bytes.get(start + number.len());
    assert!(
        !matches!(before, Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
            && !matches!(after, Some(b'0'..=b'9')),
        "{text:?}: number {number:?} at {start} is not a whole number of the input"
    );
}

/// The events of `text` under `options`, or its first error, drained
/// through the bounded helper: at most `text.len() + 1` items, and nothing
/// after an error.
pub fn parse(text: &str, options: Options) -> Result<Vec<Event<'_>>, Error> {
    common::parse_with(text, options)
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

/// The robustness target: the whole input under the default options, and,
/// when the input has a first byte, the rest of it under the options that
/// byte selects ([`options_from_selector`]). Input that is not UTF-8 is
/// outside the tokenizer's `&str` contract and is skipped.
pub fn robustness(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        check_options(text, Options::default());
    }
    if let Some((&selector, rest)) = data.split_first() {
        if let Ok(text) = std::str::from_utf8(rest) {
            check_options(text, options_from_selector(selector));
        }
    }
}

/// A maximum depth above the default, which the selector also picks.
pub const RAISED_MAX_DEPTH: usize = 2 * DEFAULT_MAX_DEPTH;

/// The options a selector byte picks: bit 0 the byte order mark policy,
/// the rest a maximum depth of 0 to 8, [`DEFAULT_MAX_DEPTH`] or
/// [`RAISED_MAX_DEPTH`], so that short inputs reach `DepthExceeded` and
/// both BOM paths, and a deep input is also read past the default.
pub fn options_from_selector(selector: u8) -> Options {
    let bom_policy = if selector & 1 == 0 {
        BomPolicy::Reject
    } else {
        BomPolicy::Ignore
    };
    let max_depth = match (selector >> 1) % 11 {
        depth @ 0..=8 => usize::from(depth),
        9 => DEFAULT_MAX_DEPTH,
        _ => RAISED_MAX_DEPTH,
    };
    Options::new()
        .with_bom_policy(bom_policy)
        .with_max_depth(max_depth)
}

/// [`check_drain`] under `options`, plus what the options may and may not
/// change:
/// - of two maximum depths (these options' and the default), the lower
///   gives the items of the higher, or a prefix of them ended by
///   `DepthExceeded`;
/// - without a leading byte order mark, the BOM policy changes nothing;
/// - under [`BomPolicy::Ignore`], a leading byte order mark (not followed
///   by another) gives the items of the rest of the text, each error
///   offset 3 bytes later.
pub fn check_options(text: &str, options: Options) {
    let items = check_drain(text, options);
    if options.max_depth() != DEFAULT_MAX_DEPTH {
        let default = check_drain(text, options.with_max_depth(DEFAULT_MAX_DEPTH));
        let (lower, higher, limit) = if options.max_depth() < DEFAULT_MAX_DEPTH {
            (&items, &default, options.max_depth())
        } else {
            (&default, &items, DEFAULT_MAX_DEPTH)
        };
        if lower != higher {
            let (last, prefix) = lower.split_last().expect("a drain has an item");
            assert!(
                matches!(last, Err(e) if e.kind() == ErrorKind::DepthExceeded),
                "{text:?}: depth {limit} gave {last:?}, the higher depth {:?}",
                higher.last()
            );
            assert_eq!(
                prefix,
                higher.get(..prefix.len()).unwrap_or_default(),
                "{text:?}: a lower depth limit changed the events before its error"
            );
        }
    }
    if options.bom_policy() == BomPolicy::Ignore {
        if let Some(rest) = text.strip_prefix('\u{FEFF}') {
            if !rest.starts_with('\u{FEFF}') {
                let bom = '\u{FEFF}'.len_utf8();
                let skipped: Vec<_> = items
                    .iter()
                    .map(|item| item.map_err(|e| (e.kind(), e.offset())))
                    .collect();
                let rest_items: Vec<_> = check_drain(rest, options)
                    .iter()
                    .map(|item| item.map_err(|e| (e.kind(), e.offset() + bom)))
                    .collect();
                assert_eq!(
                    skipped, rest_items,
                    "{text:?}: a skipped BOM is not the text without it, offsets + {bom}"
                );
            }
        }
    }
    if !text.starts_with('\u{FEFF}') {
        let other = match options.bom_policy() {
            BomPolicy::Reject => BomPolicy::Ignore,
            BomPolicy::Ignore => BomPolicy::Reject,
        };
        let flipped = check_drain(text, options.with_bom_policy(other));
        assert_eq!(
            items, flipped,
            "{text:?}: the BOM policy changed a text without a BOM"
        );
    }
}

/// Drains the tokenizer once, bounded, and checks every invariant of the
/// drain; returns its items.
///
/// - The drain stops within `input.len() + 1` items (checked by the
///   bounded helper) and the tokenizer is fused: `None` after the end,
///   twice.
/// - Nothing follows an error; the error offset is at most the input's
///   length and on a character boundary; `DepthExceeded` happens with
///   exactly `max_depth` containers open.
/// - Under `Reject`, a text that starts with a byte order mark gives
///   exactly `BomNotAllowed` at 0, and `BomNotAllowed` occurs nowhere else.
/// - An error is `UnexpectedEof` exactly when its offset is the input's
///   length. Any other error is decided by the character at its offset:
///   the input cut just after that character gives the same items.
/// - The open depth never exceeds `max_depth`.
/// - Every key and string satisfies the segment invariants. Every number
///   is non-empty number characters and a whole number of the input: a
///   slice of it with no number character just before and no digit just
///   after (so a sign or digit cannot be dropped from either end).
/// - An error-free drain is one balanced value.
pub fn check_drain(text: &str, options: Options) -> Vec<Item<'_>> {
    let items = common::items_with(text, options);
    if options.bom_policy() == BomPolicy::Reject && text.starts_with('\u{FEFF}') {
        assert!(
            matches!(
                items.as_slice(),
                [Err(e)] if e.kind() == ErrorKind::BomNotAllowed && e.offset() == 0
            ),
            "{text:?}: a leading BOM under Reject gave {items:?}"
        );
    }

    let mut tokenizer = xee_json::Tokenizer::with_options(text, options);
    for _ in 0..items.len() {
        tokenizer.next();
    }
    assert!(
        tokenizer.next().is_none(),
        "{text:?}: no end after the drain"
    );
    assert!(
        tokenizer.next().is_none(),
        "{text:?}: not fused after the end"
    );

    let mut depth = 0usize;
    let mut events = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item {
            Ok(event) => {
                match event {
                    Event::StartArray | Event::StartObject => {
                        depth += 1;
                        assert!(
                            depth <= options.max_depth(),
                            "{text:?}: depth {depth} past the limit {}",
                            options.max_depth()
                        );
                    }
                    Event::EndArray | Event::EndObject => {
                        depth = depth
                            .checked_sub(1)
                            .unwrap_or_else(|| panic!("{text:?}: close with nothing open"));
                    }
                    Event::Key(string) | Event::String(string) => common::check_segments(string),
                    Event::Number(number) => check_number(text, number),
                    Event::Bool(_) | Event::Null => {}
                }
                events.push(*event);
            }
            Err(error) => {
                assert_eq!(index + 1, items.len(), "{text:?}: items after {error:?}");
                assert!(
                    error.offset() <= text.len() && text.is_char_boundary(error.offset()),
                    "{text:?}: error offset {error:?}"
                );
                assert_eq!(
                    error.kind() == ErrorKind::UnexpectedEof,
                    error.offset() == text.len(),
                    "{text:?}: {error:?} (UnexpectedEof exactly at the end)"
                );
                if let Some(c) = text.get(error.offset()..).and_then(|t| t.chars().next()) {
                    let cut = &text[..error.offset() + c.len_utf8()];
                    assert_eq!(
                        common::items_with(cut, options),
                        items,
                        "{text:?}: {error:?}, but cut after its character, {cut:?} differs"
                    );
                }
                match error.kind() {
                    ErrorKind::DepthExceeded => assert_eq!(
                        depth,
                        options.max_depth(),
                        "{text:?}: {error:?} at depth {depth}"
                    ),
                    ErrorKind::BomNotAllowed => assert!(
                        error.offset() == 0
                            && options.bom_policy() == BomPolicy::Reject
                            && text.starts_with('\u{FEFF}'),
                        "{text:?}: {error:?}"
                    ),
                    _ => {}
                }
                return items;
            }
        }
    }
    if let Err(problem) = common::check_balanced(&events) {
        panic!("{text:?}: accepted, but the events are not balanced: {problem}");
    }
    items
}

// ---------------------------------------------------------------------------
// Differential
// ---------------------------------------------------------------------------

/// A known difference between xee-json and serde_json, and why it is not a
/// defect. The harness applies an entry only after measuring its cause, and
/// then removes the cause and requires agreement (see each entry's
/// `explained by`).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Divergence {
    /// A short name, used in reports.
    pub name: &'static str,
    /// Why the two parsers differ here.
    pub reason: &'static str,
    /// How the harness removes the cause before it requires agreement.
    pub explained_by: &'static str,
}

/// serde_json's `IgnoredAny` path has no nesting limit; xee-json has a
/// mandatory one.
pub const DEPTH_LIMIT: Divergence = Divergence {
    name: "depth limit",
    reason: "xee-json limits nesting (RFC 8259 §9 permits it; default 512) and \
             serde_json's IgnoredAny path skips values iteratively with no limit",
    explained_by: "xee-json failed with DepthExceeded; re-run it with a maximum \
                   depth above the input's length and require that it accepts",
};

/// serde_json's `Value` path stops at [`SERDE_VALUE_MAX_DEPTH`].
pub const VALUE_RECURSION_LIMIT: Divergence = Divergence {
    name: "Value recursion limit",
    reason: "serde_json's Value deserializer recurses and stops at a fixed limit \
             (127 open containers accepted, 128 rejected)",
    explained_by: "only when serde_json reports 'recursion limit exceeded' and \
                   the depth measured from xee-json's events is past 127; the \
                   structure is then not compared",
};

/// serde_json's `Value` path rejects an escaped surrogate outside a pair,
/// which its `IgnoredAny` path and xee-json accept.
pub const VALUE_LONE_SURROGATE: Divergence = Divergence {
    name: "Value lone surrogate",
    reason: "an escaped UTF-16 surrogate outside a pair is grammatical (RFC 8259 \
             §7, §8.2): xee-json delivers it as Decoded::LoneSurrogate and \
             IgnoredAny accepts it, but a Rust String cannot hold it, so Value \
             rejects it",
    explained_by: "only when serde_json reports a lone-surrogate error and \
                   xee-json delivered a LoneSurrogate; the structure is then not \
                   compared",
};

/// With `arbitrary_precision`, serde_json keeps number text, but not
/// verbatim.
pub const NUMBER_TEXT: Divergence = Divergence {
    name: "number text normalised",
    reason: "serde_json's arbitrary_precision keeps a number's text but writes \
             the exponent marker as 'e' with an explicit sign, and stores an \
             integer that fits i64/u64 as that integer, so '-0' becomes '0'; \
             xee-json delivers the source text",
    explained_by: "apply that normalisation to xee-json's text \
                   (serde_number_text) and require equality",
};

/// serde_json's private key for a number under `arbitrary_precision`
/// (`serde_json::number::TOKEN`, crate-private).
pub const SERDE_NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// With `arbitrary_precision`, serde_json's `Value` reads an object whose
/// first key is [`SERDE_NUMBER_TOKEN`] as its internal encoding of a number.
pub const NUMBER_TOKEN_KEY: Divergence = Divergence {
    name: "Value number token key",
    reason: "serde_json's arbitrary_precision encodes a number as a map with the \
             one key \"$serde_json::private::Number\", and Value's map visitor \
             classifies an object's first key against it: an object whose first \
             key decodes to it becomes a Number or an error. RFC 8259 §4 makes it \
             an ordinary object, as xee-json and IgnoredAny read it",
    explained_by: "only when the first key of an object in xee-json's events \
                   decodes to that token; the structure is then not compared, \
                   whatever Value returns",
};

/// serde_json's `Value` keeps one member per decoded key; xee-json
/// delivers every member.
pub const DUPLICATE_KEYS: Divergence = Divergence {
    name: "Value merges duplicate keys",
    reason: "RFC 8259 §4 leaves repeated names to the implementation; xee-json \
             delivers every member in document order, while serde_json's Value \
             (preserve_order, an IndexMap) keeps the first member's position \
             and the last member's value",
    explained_by: "apply that merge to xee-json's members, keys compared after \
                   decoding, and require equality",
};

/// serde_json never skips a leading byte order mark; xee-json does under
/// `BomPolicy::Ignore`.
pub const LEADING_BOM: Divergence = Divergence {
    name: "leading BOM",
    reason: "RFC 8259 §8.1 lets a parser ignore a leading U+FEFF, and \
             BomPolicy::Ignore does; serde_json rejects it (as xee-json's \
             default BomPolicy::Reject does)",
    explained_by: "only for a text that starts with U+FEFF, which serde_json must \
                   reject: compare xee-json under Ignore on the whole text with \
                   serde_json on the text without the U+FEFF",
};

/// Every known divergence.
pub const DIVERGENCES: [&Divergence; 7] = [
    &DEPTH_LIMIT,
    &VALUE_RECURSION_LIMIT,
    &VALUE_LONE_SURROGATE,
    &NUMBER_TOKEN_KEY,
    &DUPLICATE_KEYS,
    &NUMBER_TEXT,
    &LEADING_BOM,
];

/// The deepest nesting serde_json's `Value` path accepts: its
/// `remaining_depth` starts at 128 and an open container that brings it to
/// 0 fails. Pinned by a replay test.
pub const SERDE_VALUE_MAX_DEPTH: usize = 127;

/// What one differential run found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The divergence entries applied, by name.
    pub fired: BTreeSet<&'static str>,
    /// Legs whose structure was compared with serde_json's `Value`.
    pub structure_compared: usize,
}

impl Report {
    fn fire(&mut self, divergence: &Divergence) {
        self.fired.insert(divergence.name);
    }
}

/// The differential target. Input that is not UTF-8 is skipped: it is
/// outside the tokenizer's `&str` contract, and serde_json is only given
/// `&str` (its byte-slice reader does not validate UTF-8 inside skipped
/// strings).
pub fn differential(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        differential_str(text);
        if let Err(problem) = try_lower_depth(text) {
            panic!("unexplained divergence one level down on {text:?}:\n{problem}");
        }
    }
}

/// [`differential`] on a text; panics on a divergence no entry explains,
/// and otherwise reports which entries it applied.
pub fn differential_str(text: &str) -> Report {
    match try_differential(text) {
        Ok(report) => report,
        Err(problem) => panic!("unexplained divergence on {text:?}:\n{problem}"),
    }
}

/// [`differential_str`], returning an unexplained divergence as an error.
///
/// Two legs: the default options (BomPolicy::Reject, the default depth)
/// against serde_json on the same text; and, for a text with a leading
/// byte order mark, BomPolicy::Ignore against serde_json on the text
/// without it ([`LEADING_BOM`]).
pub fn try_differential(text: &str) -> Result<Report, String> {
    let mut report = Report::default();
    compare(text, text, Options::default(), &mut report)?;
    if let Some(rest) = text.strip_prefix('\u{FEFF}') {
        if serde_json::from_str::<IgnoredAny>(text).is_ok() {
            return Err(
                "serde_json accepted a leading U+FEFF: the LEADING_BOM entry is stale".into(),
            );
        }
        report.fire(&LEADING_BOM);
        compare(
            text,
            rest,
            Options::default().with_bom_policy(BomPolicy::Ignore),
            &mut report,
        )?;
    }
    Ok(report)
}

/// The depth leg of [`differential`]. For a text both parsers accept whose
/// events nest `d > 0` containers deep, xee-json under a maximum depth of
/// `d - 1` must fail with `DepthExceeded`, and [`DEPTH_LIMIT`] must explain
/// that against serde_json's acceptance with the same events as the
/// default leg. Returns the maximum depth used, or `None` when the leg does
/// not apply (a rejected text, or no container).
pub fn try_lower_depth(text: &str) -> Result<Option<usize>, String> {
    let Ok(events) = common::parse_with(text, Options::default()) else {
        return Ok(None);
    };
    let Some(depth) = nesting(&events).checked_sub(1) else {
        return Ok(None);
    };
    let serde = serde_json::from_str::<IgnoredAny>(text).map(|_| ());
    let options = Options::default().with_max_depth(depth);
    let mut report = Report::default();
    let lowered = judge_accept(
        text,
        options,
        common::parse_with(text, options),
        serde.map_err(|e| e.to_string()),
        &mut report,
    )?;
    if !report.fired.contains(DEPTH_LIMIT.name) {
        return Err(format!(
            "xee-json at maximum depth {depth} gave {lowered:?} on a text {} deep",
            depth + 1
        ));
    }
    if lowered.as_deref() != Some(events.as_slice()) {
        return Err(format!(
            "xee-json without the depth limit gave other events than at the default: {lowered:?}"
        ));
    }
    Ok(Some(depth))
}

/// The most containers open at once in an event sequence.
fn nesting(events: &[Event<'_>]) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0;
    for event in events {
        match event {
            Event::StartArray | Event::StartObject => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            Event::EndArray | Event::EndObject => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// One leg: xee-json on `xee_text` under `options` against serde_json on
/// `serde_text`.
fn compare(
    xee_text: &str,
    serde_text: &str,
    options: Options,
    report: &mut Report,
) -> Result<(), String> {
    let xee = common::parse_with(xee_text, options);
    let xee_error = xee.as_ref().err().copied();
    let serde = serde_json::from_str::<IgnoredAny>(serde_text).map(|_| ());
    let events = judge_accept(
        xee_text,
        options,
        xee,
        serde.map_err(|e| e.to_string()),
        report,
    )?;
    let Some(events) = events else {
        // Both reject. On the same text, serde_json also witnesses where.
        return match xee_error {
            Some(error) if xee_text == serde_text => judge_rejection(xee_text, error),
            _ => Ok(()),
        };
    };
    judge_value(&events, serde_json::from_str::<Value>(serde_text), report)
}

/// serde_json as an outside witness of where xee-json rejects a text. An
/// error other than `UnexpectedEof` or `DepthExceeded` says that the text
/// cut just after the character at its offset cannot be completed, so
/// serde_json must reject that cut, and not as merely unfinished
/// (`Category::Eof`). One exemption is serde_json's own: it reads a `\u`
/// escape's four bytes before it checks them, so a bad hex digit fewer than
/// four bytes after the `u` is an end of input to it. Public so that the
/// tests can hand it an error at the wrong offset.
pub fn judge_rejection(text: &str, error: Error) -> Result<(), String> {
    if matches!(
        error.kind(),
        ErrorKind::UnexpectedEof | ErrorKind::DepthExceeded
    ) {
        return Ok(());
    }
    let offset = error.offset();
    let Some(c) = text.get(offset..).and_then(|rest| rest.chars().next()) else {
        return Err(format!("xee-json: {error} is not at a character"));
    };
    if error.kind() == ErrorKind::InvalidEscape && in_hex_escape(&text[..offset]) {
        return Ok(());
    }
    let cut = &text[..offset + c.len_utf8()];
    match serde_json::from_str::<IgnoredAny>(cut) {
        Err(e) if e.classify() != serde_json::error::Category::Eof => Ok(()),
        verdict => Err(format!(
            "xee-json: {error}, so {cut:?} cannot be completed\nserde_json on {cut:?}: {}",
            match verdict {
                Ok(_) => "accepted".to_string(),
                Err(e) => format!("unfinished: {e}"),
            }
        )),
    }
}

/// Whether `before` ends inside a `\u` escape: the escape's backslash (one
/// not itself escaped), `u`, and at most three hex digits.
fn in_hex_escape(before: &str) -> bool {
    let bytes = before.as_bytes();
    (0..=3).any(|digits| {
        let Some(u) = bytes.len().checked_sub(digits + 1) else {
            return false;
        };
        let backslashes = bytes[..u].iter().rev().take_while(|&&b| b == b'\\').count();
        bytes[u] == b'u' && bytes[u + 1..].iter().all(u8::is_ascii_hexdigit) && backslashes % 2 == 1
    })
}

/// The accept/reject leg. `Ok(Some(events))` when both accept (the events
/// under the options that accepted), `Ok(None)` when both reject, an error
/// for a divergence no entry explains. Public so that the tests can hand it
/// a verdict serde_json would not give and see an entry refuse it.
pub fn judge_accept<'a>(
    xee_text: &'a str,
    options: Options,
    xee: Result<Vec<Event<'a>>, Error>,
    serde: Result<(), String>,
    report: &mut Report,
) -> Result<Option<Vec<Event<'a>>>, String> {
    match (xee, serde) {
        (Ok(events), Ok(())) => Ok(Some(events)),
        (Err(_), Err(_)) => Ok(None),
        (Err(error), Ok(())) if error.kind() == ErrorKind::DepthExceeded => {
            // DEPTH_LIMIT: remove the cause, then require agreement.
            let unlimited = options.with_max_depth(xee_text.len() + 1);
            match common::parse_with(xee_text, unlimited) {
                Ok(events) => {
                    common::check_balanced(&events).map_err(|problem| {
                        format!("xee-json without the depth limit: unbalanced events: {problem}")
                    })?;
                    report.fire(&DEPTH_LIMIT);
                    Ok(Some(events))
                }
                Err(again) => Err(format!(
                    "xee-json: {error} (and {again} without the depth limit)\nserde_json: accepted"
                )),
            }
        }
        (xee, serde) => Err(format!(
            "xee-json: {}\nserde_json: {}",
            match xee {
                Ok(_) => "accepted".to_string(),
                Err(error) => error.to_string(),
            },
            match serde {
                Ok(()) => "accepted".to_string(),
                Err(error) => error,
            }
        )),
    }
}

/// What the harness measures on xee-json's side of the `Value` leg.
#[derive(Debug, Default)]
struct Facts {
    max_depth: usize,
    lone_surrogate: bool,
    number_token_key: bool,
}

impl Facts {
    fn of(events: &[Event<'_>]) -> Facts {
        let mut facts = Facts::default();
        // One entry per open container: for an object, whether its first
        // key has been read.
        let mut open: Vec<Option<bool>> = Vec::new();
        for event in events {
            match event {
                Event::StartArray => open.push(None),
                Event::StartObject => open.push(Some(false)),
                Event::EndArray | Event::EndObject => {
                    open.pop();
                }
                Event::Key(key) => {
                    let decoded = common::decode(key);
                    facts.lone_surrogate |= decoded.iter().any(Result::is_err);
                    if let Some(Some(first_read)) = open.last_mut() {
                        if !*first_read {
                            *first_read = true;
                            facts.number_token_key |= decoded
                                .iter()
                                .copied()
                                .eq(SERDE_NUMBER_TOKEN.chars().map(Ok));
                        }
                    }
                }
                Event::String(string) => {
                    facts.lone_surrogate |= !common::lone_surrogates(string).is_empty();
                }
                Event::Number(_) | Event::Bool(_) | Event::Null => {}
            }
            facts.max_depth = facts.max_depth.max(open.len());
        }
        facts
    }
}

/// The `Value` leg, for a text both parsers accept: serde_json's `Value`
/// must have the same structure, member order, decoded strings and
/// (normalised) number text as xee-json's events, with repeated keys
/// merged ([`DUPLICATE_KEYS`]). A `Value` error is explained only by
/// [`VALUE_RECURSION_LIMIT`] or [`VALUE_LONE_SURROGATE`], each with its
/// precondition measured on the events; [`NUMBER_TOKEN_KEY`] skips the
/// leg. Public for the same reason as [`judge_accept`].
pub fn judge_value(
    events: &[Event<'_>],
    value: Result<Value, serde_json::Error>,
    report: &mut Report,
) -> Result<(), String> {
    let facts = Facts::of(events);
    if facts.number_token_key {
        report.fire(&NUMBER_TOKEN_KEY);
        return Ok(());
    }
    let value = match value {
        Ok(value) => value,
        Err(error) => {
            let message = error.to_string();
            if message.starts_with("recursion limit exceeded")
                && facts.max_depth > SERDE_VALUE_MAX_DEPTH
            {
                report.fire(&VALUE_RECURSION_LIMIT);
                return Ok(());
            }
            if (message.starts_with("lone leading surrogate in hex escape")
                || message.starts_with("unexpected end of hex escape"))
                && facts.lone_surrogate
            {
                report.fire(&VALUE_LONE_SURROGATE);
                return Ok(());
            }
            return Err(format!(
                "xee-json: accepted ({facts:?})\nserde_json IgnoredAny: accepted\nserde_json Value: {message}"
            ));
        }
    };
    if facts.max_depth > SERDE_VALUE_MAX_DEPTH || facts.lone_surrogate {
        return Err(format!(
            "serde_json Value accepted a text with {facts:?}: an entry's pin is stale"
        ));
    }
    let ours = flatten_events(events, report)?;
    let theirs = flatten_value(&value);
    if ours != theirs {
        let at = ours
            .iter()
            .zip(&theirs)
            .position(|(a, b)| a != b)
            .unwrap_or(ours.len().min(theirs.len()));
        return Err(format!(
            "structure differs at item {at}:\nxee-json:   {:?}\nserde_json: {:?}",
            ours.get(at),
            theirs.get(at)
        ));
    }
    report.structure_compared += 1;
    Ok(())
}

/// A JSON value as a flat, owned sequence, so both sides compare without
/// recursion: brackets, then keys, strings and numbers as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flat {
    /// `{`
    StartObject,
    /// `}`
    EndObject,
    /// `[`
    StartArray,
    /// `]`
    EndArray,
    /// A decoded key.
    Key(String),
    /// A decoded string.
    String(String),
    /// Number text.
    Number(String),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
}

fn decoded(string: &Str<'_>) -> Result<String, String> {
    common::decode_string(string)
        .ok_or_else(|| format!("{string:?} has a lone surrogate after the check for one"))
}

/// xee-json's events as [`Flat`] items, strings decoded, numbers in
/// serde_json's normal form ([`NUMBER_TEXT`]), and each object's members
/// merged as serde_json's `Value` merges them ([`DUPLICATE_KEYS`]): one
/// member per decoded key, at the first member's position, with the last
/// member's value. Built with an explicit stack; each finished value is
/// moved into its parent.
fn flatten_events(events: &[Event<'_>], report: &mut Report) -> Result<Vec<Flat>, String> {
    enum Frame {
        Array(Vec<Flat>),
        Object {
            members: Vec<(String, Vec<Flat>)>,
            index: HashMap<String, usize>,
            key: Option<String>,
        },
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = None;
    for event in events {
        let value = match event {
            Event::StartArray => {
                stack.push(Frame::Array(vec![Flat::StartArray]));
                continue;
            }
            Event::StartObject => {
                stack.push(Frame::Object {
                    members: Vec::new(),
                    index: HashMap::new(),
                    key: None,
                });
                continue;
            }
            Event::Key(key) => {
                let Some(Frame::Object { key: pending, .. }) = stack.last_mut() else {
                    return Err(format!("{event:?} outside an object"));
                };
                *pending = Some(decoded(key)?);
                continue;
            }
            Event::EndArray => match stack.pop() {
                Some(Frame::Array(mut items)) => {
                    items.push(Flat::EndArray);
                    items
                }
                _ => return Err("unmatched EndArray".to_string()),
            },
            Event::EndObject => match stack.pop() {
                Some(Frame::Object { members, .. }) => {
                    let mut items = vec![Flat::StartObject];
                    for (key, value) in members {
                        items.push(Flat::Key(key));
                        items.extend(value);
                    }
                    items.push(Flat::EndObject);
                    items
                }
                _ => return Err("unmatched EndObject".to_string()),
            },
            Event::String(string) => vec![Flat::String(decoded(string)?)],
            Event::Number(number) => {
                let normal = serde_number_text(number);
                if normal != *number {
                    report.fire(&NUMBER_TEXT);
                }
                vec![Flat::Number(normal)]
            }
            Event::Bool(b) => vec![Flat::Bool(*b)],
            Event::Null => vec![Flat::Null],
        };
        match stack.last_mut() {
            None => top = Some(value),
            Some(Frame::Array(items)) => items.extend(value),
            Some(Frame::Object {
                members,
                index,
                key,
            }) => {
                let key = key.take().ok_or("a member value without a key")?;
                match index.get(&key).and_then(|&at| members.get_mut(at)) {
                    Some(member) => {
                        report.fire(&DUPLICATE_KEYS);
                        member.1 = value;
                    }
                    None => {
                        index.insert(key.clone(), members.len());
                        members.push((key, value));
                    }
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err(format!("{} containers left open", stack.len()));
    }
    top.ok_or_else(|| "no value".to_string())
}

/// serde_json's `Value` as [`Flat`] items, members in map order (source
/// order under `preserve_order`), walked with an explicit stack.
pub fn flatten_value(value: &Value) -> Vec<Flat> {
    enum Frame<'v> {
        Array(std::slice::Iter<'v, Value>),
        Object(serde_json::map::Iter<'v>),
    }
    let mut flat = Vec::new();
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut next = Some(value);
    loop {
        if let Some(value) = next.take() {
            match value {
                Value::Null => flat.push(Flat::Null),
                Value::Bool(b) => flat.push(Flat::Bool(*b)),
                Value::Number(n) => flat.push(Flat::Number(n.to_string())),
                Value::String(s) => flat.push(Flat::String(s.clone())),
                Value::Array(items) => {
                    flat.push(Flat::StartArray);
                    stack.push(Frame::Array(items.iter()));
                }
                Value::Object(members) => {
                    flat.push(Flat::StartObject);
                    stack.push(Frame::Object(members.iter()));
                }
            }
        }
        let Some(frame) = stack.last_mut() else {
            return flat;
        };
        match frame {
            Frame::Array(items) => match items.next() {
                Some(item) => next = Some(item),
                None => {
                    stack.pop();
                    flat.push(Flat::EndArray);
                }
            },
            Frame::Object(members) => match members.next() {
                Some((key, member)) => {
                    flat.push(Flat::Key(key.clone()));
                    next = Some(member);
                }
                None => {
                    stack.pop();
                    flat.push(Flat::EndObject);
                }
            },
        }
    }
}

/// A valid number's text as serde_json's `arbitrary_precision` stores it
/// ([`NUMBER_TEXT`]): an integer that fits `u64` (non-negative) or `i64`
/// (negative) as that integer's decimal form; otherwise the source text
/// with the exponent marker as `e` and an explicit exponent sign.
pub fn serde_number_text(number: &str) -> String {
    if let Some(digits) = number.strip_prefix('-') {
        if digits.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(integer) = number.parse::<i64>() {
                return integer.to_string();
            }
        }
    } else if number.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(integer) = number.parse::<u64>() {
            return integer.to_string();
        }
    }
    let mut normal = String::with_capacity(number.len() + 1);
    let mut after_marker = false;
    for c in number.chars() {
        if after_marker && c != '+' && c != '-' {
            normal.push('+');
        }
        after_marker = false;
        if c == 'e' || c == 'E' {
            normal.push('e');
            after_marker = true;
        } else {
            normal.push(c);
        }
    }
    normal
}
