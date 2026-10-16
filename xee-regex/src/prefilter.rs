//! What a search rules out before a matcher runs, read off the parsed
//! pattern: a small set of characters every match holds one of, so that a
//! text with none of them has no match; the literal text every match
//! begins with, so that a search tries only where it occurs; and whether
//! every match begins at the start of the input, so that a search tries
//! only there. None changes an answer: a match the matchers would find
//! holds a character of the set, begins with the literal, and of an
//! anchored pattern begins at the start. Their scans are charged, so a
//! call's steps move with them, mostly down, and with them where a budget
//! refuses it.

use crate::charset::CharSet;
use crate::parse::Node;
use crate::{Budget, Error, Flags};

/// A scan reads its input in windows, each charged a step per byte before
/// it is read: the first of one byte, each next one twice as long up to
/// `CHUNK`, so that a scan is charged at most twice what it read, and a
/// long one pays for few windows. A literal's window also reads the
/// literal's length less one past its last start.
const FIRST: usize = 1;
const CHUNK: usize = 4096;

/// A required set larger than this, in characters, is not worth a scan:
/// most texts hold one of its characters.
const MAX_REQUIRED_CHARS: u64 = 16;

/// The longest literal kept, in bytes: each window of a literal's scan
/// reads its length less one past the window, which a search pays at every
/// candidate where the match then fails, so a long literal costs more than
/// it saves where it occurs often.
const MAX_PREFIX_BYTES: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct Prefilter {
    /// A set every match holds a character of.
    pub(crate) required: Option<CharSet>,
    /// The text every match begins with; empty if none is known.
    pub(crate) prefix: String,
    /// Every match begins at the start of the input.
    pub(crate) anchored: bool,
}

pub(crate) fn analyse(node: &Node, flags: &Flags) -> Prefilter {
    Prefilter {
        required: required(node),
        prefix: prefix(node).0,
        // With flag `m`, `^` also matches after a newline.
        anchored: !flags.m && anchored(node),
    }
}

/// Where the first character of `set` at or after byte `from` is, each
/// byte scanned a step, charged before it is read.
pub(crate) fn find_any(
    hay: &str,
    from: usize,
    set: &CharSet,
    budget: &mut Budget,
) -> Result<Option<usize>, Error> {
    let (mut at, mut window) = (from, FIRST);
    while at < hay.len() {
        let end = boundary(hay, at + window);
        window = (2 * window).min(CHUNK);
        budget.charge((end - at) as u64)?;
        if let Some((i, _)) = hay[at..end]
            .char_indices()
            .find(|&(_, c)| set.contains(c as u32))
        {
            return Ok(Some(at + i));
        }
        at = end;
    }
    Ok(None)
}

/// Where `literal` first occurs at or after byte `from`, each byte scanned
/// a step, charged before it is read.
pub(crate) fn find_literal(
    hay: &str,
    from: usize,
    literal: &str,
    budget: &mut Budget,
) -> Result<Option<usize>, Error> {
    let (mut at, mut window) = (from, FIRST);
    while at + literal.len() <= hay.len() {
        // A window's starts, and the literal's length past them.
        let starts = boundary(hay, at + window);
        window = (2 * window).min(CHUNK);
        let end = boundary(hay, starts + literal.len() - 1);
        budget.charge((end - at) as u64)?;
        if let Some(i) = hay[at..end].find(literal) {
            return Ok(Some(at + i));
        }
        at = starts;
    }
    Ok(None)
}

/// The first character boundary of `hay` at or after `at`, or its end.
fn boundary(hay: &str, at: usize) -> usize {
    let mut at = at.min(hay.len());
    while !hay.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// The number of characters in `set`.
fn size(set: &CharSet) -> u64 {
    set.ranges()
        .iter()
        .map(|&(a, b)| u64::from(b - a) + 1)
        .sum()
}

fn small(set: CharSet) -> Option<CharSet> {
    (size(&set) <= MAX_REQUIRED_CHARS).then_some(set)
}

/// A small set every match of `node` holds a character of: the smallest
/// these rules find, not always the smallest there is (`xa|ya` gives
/// {x, y}, where every match also holds an `a`).
fn required(node: &Node) -> Option<CharSet> {
    match node {
        Node::Empty | Node::Bol | Node::Eol | Node::Backref(_) => None,
        Node::Set(set) => small(set.clone()),
        Node::Group(_, inner) => required(inner),
        // Every part's set is required of the whole; the smallest serves.
        Node::Concat(parts) => parts
            .iter()
            .filter_map(required)
            .min_by_key(|set| (size(set), set.ranges().len())),
        // A match is one branch's: the union of a set from each branch,
        // if every branch has one.
        Node::Alt(branches) => branches.iter().try_fold(CharSet::empty(), |all, branch| {
            small(all.union(&required(branch)?))
        }),
        Node::Repeat { node, min, .. } => {
            if *min > 0 {
                required(node)
            } else {
                None
            }
        }
    }
}

/// Whether every match of `node` passes a `^`, which without flag `m`
/// matches only at the start of the input (F&O 3.1 §5.6.2): nothing
/// before it in the match can have consumed a character, so the match
/// begins there. A pattern these rules mark has no other match, but not
/// every such pattern is marked: `$a`, which matches nothing, is not.
fn anchored(node: &Node) -> bool {
    match node {
        Node::Bol => true,
        Node::Empty | Node::Eol | Node::Set(_) | Node::Backref(_) => false,
        Node::Group(_, inner) => anchored(inner),
        // A match passes every part.
        Node::Concat(parts) => parts.iter().any(anchored),
        // A match is one branch's.
        Node::Alt(branches) => branches.iter().all(anchored),
        // A match passes at least one iteration if the minimum is one.
        Node::Repeat { node, min, .. } => *min > 0 && anchored(node),
    }
}

/// The literal every match of `node` begins with, and whether every match
/// is exactly that literal, so that what follows extends it.
fn prefix(node: &Node) -> (String, bool) {
    match node {
        // Anchors consume nothing: the literal goes on past them.
        Node::Empty | Node::Bol | Node::Eol => (String::new(), true),
        Node::Backref(_) => (String::new(), false),
        Node::Set(set) => match set.ranges() {
            [(a, b)] if a == b => (
                char::from_u32(*a).map_or_else(String::new, String::from),
                true,
            ),
            _ => (String::new(), false),
        },
        Node::Group(_, inner) => prefix(inner),
        Node::Concat(parts) => {
            let mut out = String::new();
            for part in parts {
                let (p, whole) = prefix(part);
                out.push_str(&p);
                if !whole || out.len() > MAX_PREFIX_BYTES {
                    return (cut(out), false);
                }
            }
            (out, true)
        }
        // The branches' common start; whole only if they are one literal.
        Node::Alt(branches) => {
            let mut iter = branches.iter().map(prefix);
            let Some((mut common, mut whole)) = iter.next() else {
                return (String::new(), true);
            };
            for (p, w) in iter {
                let same = common
                    .char_indices()
                    .zip(p.chars())
                    .find(|((_, a), b)| a != b)
                    .map_or(common.len().min(p.len()), |((i, _), _)| i);
                whole = whole && w && same == common.len() && same == p.len();
                common.truncate(same);
            }
            (common, whole)
        }
        Node::Repeat { node, min, max, .. } => {
            if *min == 0 {
                return (String::new(), false);
            }
            let (p, whole) = prefix(node);
            if !whole || p.is_empty() {
                return (p, false);
            }
            // A literal repeated: as many copies as the minimum, within the
            // limit; the repetition is whole only if it is exact.
            let copies = (*min as usize).min(MAX_PREFIX_BYTES / p.len() + 1);
            let out = p.repeat(copies);
            let exact = *max == Some(*min) && copies == *min as usize;
            let whole = exact && out.len() <= MAX_PREFIX_BYTES;
            (cut(out), whole)
        }
    }
}

/// `text` cut to at most the longest literal kept, on a character
/// boundary.
fn cut(mut text: String) -> String {
    if text.len() > MAX_PREFIX_BYTES {
        let mut end = MAX_PREFIX_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use crate::Flags;

    fn filter(p: &str, f: &str) -> Prefilter {
        let mut flags = Flags::default();
        for c in f.chars() {
            match c {
                'i' => flags.i = true,
                'm' => flags.m = true,
                'q' => flags.q = true,
                _ => unreachable!(),
            }
        }
        analyse(&parse(p, &flags).unwrap().node, &flags)
    }

    fn chars(set: &Option<CharSet>) -> String {
        set.as_ref().map_or("-".into(), |s| {
            s.ranges()
                .iter()
                .flat_map(|&(a, b)| (a..=b).filter_map(char::from_u32))
                .collect()
        })
    }

    #[test]
    fn the_required_set() {
        for (p, f, want) in [
            ("(a|b)*c", "", "c"),
            ("\\w{1,255}x", "", "x"),
            (".{0,20000}x", "", "x"),
            ("abc", "", "a"),
            ("a|b", "", "ab"),
            ("(?:ab|c)d?", "", "ac"),
            ("a?", "", "-"),
            ("a|b?", "", "-"),
            ("\\w+", "", "-"),
            ("(a)\\1", "", "a"),
            ("^$", "", "-"),
            ("k", "i", "Kk\u{212A}"),
            ("[a-z]", "", "-"),
            ("x{3}y{0,2}", "", "x"),
            // At the limit of 16 characters, and past it.
            ("[a-p]", "", "abcdefghijklmnop"),
            ("[a-q]", "", "-"),
        ] {
            assert_eq!(chars(&filter(p, f).required), want, "{p}");
        }
    }

    #[test]
    fn the_prefix() {
        for (p, f, want) in [
            ("abc", "", "abc"),
            ("abc|abd", "", "ab"),
            ("^abc", "m", "abc"),
            ("a^b", "", "ab"),
            ("ab*c", "", "a"),
            ("a+b", "", "a"),
            ("a{3}b", "", "aaab"),
            ("(?:ab){2}c", "", "ababc"),
            ("a{2,3}b", "", "aa"),
            ("(a)(b)\\1c", "", "ab"),
            ("abc", "i", ""),
            (".abc", "", ""),
            ("a?b", "", ""),
            ("é😀x", "", "é😀x"),
        ] {
            assert_eq!(filter(p, f).prefix, want, "{p}");
        }
        // Kept to its limit of 16 bytes, on a character boundary: after
        // an `a`, the eighth `é` would end at byte 17.
        assert_eq!(MAX_PREFIX_BYTES, 16);
        assert_eq!(filter(&"é".repeat(200), "").prefix, "é".repeat(8));
        let cut = filter(&format!("a{}", "é".repeat(200)), "").prefix;
        assert_eq!(cut, format!("a{}", "é".repeat(7)));
        assert_eq!(filter("a{100000}", "").prefix, "a".repeat(16));
        // A literal repeated past the limit, cut inside a copy.
        assert_eq!(filter("(?:abc){100}", "").prefix, "abcabcabcabcabca");
    }

    #[test]
    fn the_anchor() {
        for (p, f, want) in [
            ("^a", "", true),
            ("^", "", true),
            ("^a", "m", false),
            ("a", "", false),
            ("()a", "", false),
            ("a$", "", false),
            ("(a)\\1", "", false),
            ("a^", "", true),
            ("$^", "", true),
            ("a*^b", "", true),
            ("^a|^b", "", true),
            ("(^a|^b)c", "", true),
            ("^a|b", "", false),
            ("(^a)+", "", true),
            ("(^a){2,3}", "", true),
            ("(^a)*", "", false),
            ("(^a)?b", "", false),
            ("(^a){0,2}", "", false),
            ("a{0}^b", "", true),
            ("(^)\\1", "", true),
            ("\\^a", "", false),
            ("^a", "q", false),
            ("[\\^]a", "", false),
        ] {
            assert_eq!(filter(p, f).anchored, want, "{p} flags {f:?}");
        }
        // The XSD dialect's patterns are anchored at both ends; `^` and
        // `$` are characters there.
        let flags = Flags::default();
        for p in ["a|b", "", "^", "x*$"] {
            let parsed = crate::parse::parse_xsd(p).unwrap();
            assert!(analyse(&parsed.node, &flags).anchored, "{p}");
        }
    }

    fn scan(op: impl FnOnce(&mut Budget) -> Result<Option<usize>, Error>) -> (Option<usize>, u64) {
        let mut budget = Budget::new(1 << 20);
        (op(&mut budget).unwrap(), budget.spent())
    }

    #[test]
    fn a_scan_is_charged_for_the_windows_it_reads() {
        let a = CharSet::single('a' as u32);
        // Windows of 1, 2, 4, ..., the last cut at the end: 101 bytes read,
        // 101 charged.
        let hay = format!("{}a", "x".repeat(100));
        assert_eq!(scan(|b| find_any(&hay, 0, &a, b)), (Some(100), 101));
        assert_eq!(scan(|b| find_any(&"x".repeat(100), 0, &a, b)), (None, 100));
        // In a long text, windows stop doubling at 4,096 bytes: the `a` at
        // 70,000 lies in the window of 4,096 from 69,631 (13 doubling
        // windows, then 15 of 4,096), read and charged to 73,727.
        let hay = format!("{}a{}", "x".repeat(70_000), "x".repeat(100_000));
        assert_eq!(scan(|b| find_any(&hay, 0, &a, b)), (Some(70_000), 73_727));
        // A window ends on a character boundary: 21 characters of two bytes
        // and the `a` at byte 40.
        let hay = format!("{}a", "é".repeat(20));
        assert_eq!(scan(|b| find_any(&hay, 0, &a, b)).0, Some(40));
        // A literal's windows each read its length less one past their
        // last start: starts 0, 1-2, 3-6, 7-14 and 15-17 read 2, 3, 5, 9
        // and 3 bytes. An occurrence in the overlap is found where it
        // starts.
        let hay = format!("{}ab", "x".repeat(16));
        assert_eq!(scan(|b| find_literal(&hay, 0, "ab", b)), (Some(16), 22));
        for k in 0..40 {
            let hay = format!("{}ab{}", "x".repeat(k), "y".repeat(5));
            assert_eq!(scan(|b| find_literal(&hay, 0, "ab", b)).0, Some(k), "{k}");
            let hay = format!("{}éab", "é".repeat(k));
            assert_eq!(
                scan(|b| find_literal(&hay, 0, "éab", b)).0,
                Some(2 * k),
                "{k}"
            );
        }
        assert_eq!(scan(|b| find_literal("xxxa", 0, "ab", b)).0, None);
        // A long scan: 99,995 starts in windows of 1 to 4,096 starts, 13,
        // then of 4,096, 23, each reading five bytes past its starts.
        let long = "x".repeat(100_000);
        assert_eq!(
            scan(|b| find_literal(&long, 0, "needle", b)),
            (None, 99_995 + 36 * 5)
        );
    }
}
