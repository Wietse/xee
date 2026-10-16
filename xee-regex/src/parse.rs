//! A pattern parsed to a tree whose character classes are resolved to
//! sets, case variants included under flag `i`, in one of two dialects:
//! XPath 3.1's (F&O 3.1 §5.6.1, over the grammar of XSD 1.1 Part 2
//! Appendix G), or XSD 1.1's own (Appendix G alone, without flags).
//!
//! Parsing is bounded before it starts by the pattern's length, and as it
//! goes by the nesting depth, the nodes it builds and the ranges of the
//! character sets it builds, so that no pattern takes more than a bounded
//! stack, memory or time to parse.

use std::collections::BTreeSet;

use crate::charset::CharSet;
use crate::unicode;
use crate::{Error, Flags};

/// Ceiling: the pattern's length in bytes, checked before anything is
/// built from it.
pub(crate) const MAX_PATTERN_LEN: usize = 1 << 20;

/// Ceiling: nesting of groups and class subtractions. Every recursive pass
/// over the tree, parsing, compiling and dropping it, fits a 2 MiB stack
/// at this depth in a debug build (`the_nesting_ceiling_fits_a_small_stack`).
pub(crate) const MAX_DEPTH: usize = 100;

/// Ceiling: the tree's nodes (each branch, atom and quantifier).
pub(crate) const MAX_NODES: usize = 1 << 18;

/// Ceiling: the ranges in all the character sets parsing builds (each
/// written class, escape or character builds its own; `\w` is about 830
/// ranges), each set counted with `SET_OVERHEAD`, plus the case-variant
/// entries flag `i` visits.
pub(crate) const MAX_SET_RANGES: usize = 1 << 22;

/// A set's cost besides its ranges, in the same unit (its vector and the
/// node it sits in are about 8 words).
const SET_OVERHEAD: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Empty,
    Set(CharSet),
    /// `^`: the start of the input, or with flag `m` of a line.
    Bol,
    /// `$`: the end of the input, or with flag `m` of a line.
    Eol,
    Backref(usize),
    /// A group, capturing with its number or not.
    Group(Option<usize>, Box<Node>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        /// `None` for no upper bound.
        max: Option<u32>,
        greedy: bool,
    },
}

/// The dialect a pattern is read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    /// F&O 3.1 §5.6.1: XSD's grammar, with `^` and `$` as anchors
    /// (§5.6.1.1), `\$` as an escape, reluctant quantifiers (§5.6.1.2),
    /// non-capturing groups (§5.6.1.3) and back-references (§5.6.1.4); an
    /// unknown block name is an error (§5.6.1.5).
    XPath,
    /// XSD 1.1 Part 2 Appendix G alone: `^` and `$` are normal characters
    /// ([73] NormalChar), none of F&O's additions is there, and an unknown
    /// block name is the set of all characters (§G.4.2.4).
    Xsd,
}

#[derive(Debug)]
pub(crate) struct Parsed {
    pub(crate) node: Node,
    pub(crate) group_count: usize,
    /// `group_parent[n]` is the capturing group that most closely encloses
    /// group `n`, or 0; index 0 is unused.
    pub(crate) group_parent: Vec<usize>,
    /// The XSD dialect's unknown block names, each once, in the order of
    /// their UTF-8 bytes; empty in the XPath dialect, which refuses them.
    pub(crate) unknown_blocks: Vec<String>,
}

/// Parses `pattern` in the XPath 3.1 dialect under `flags`.
pub(crate) fn parse(pattern: &str, flags: &Flags) -> Result<Parsed, Error> {
    parse_in(pattern, flags, Dialect::XPath)
}

/// Parses `pattern` in the XSD 1.1 dialect, which has no flags, anchored
/// at both ends: XSD 1.1 Part 2 Appendix G's regular expressions are
/// "implicitly anchored at the head and tail". The tree is the pattern's
/// between a `^` and a `$`, which without flag `m` match only at the
/// start and the end of the input, so that every branch of a top-level
/// `|` is anchored, the groups keep their numbers and the minimum length
/// is the pattern's. The wrapping adds one level, a sequence of three, to
/// every recursive pass over the tree (`the_nesting_ceiling_fits_a_small_stack`
/// runs them at the nesting ceiling in this dialect too).
pub(crate) fn parse_xsd(pattern: &str) -> Result<Parsed, Error> {
    let mut parsed = parse_in(pattern, &Flags::default(), Dialect::Xsd)?;
    let node = std::mem::replace(&mut parsed.node, Node::Empty);
    parsed.node = Node::Concat(vec![Node::Bol, node, Node::Eol]);
    Ok(parsed)
}

fn parse_in(pattern: &str, flags: &Flags, dialect: Dialect) -> Result<Parsed, Error> {
    if pattern.len() > MAX_PATTERN_LEN {
        return Err(Error::Limit(format!(
            "the pattern is longer than {MAX_PATTERN_LEN} bytes"
        )));
    }
    let mut counts = Counts::default();
    if flags.q {
        // §5.6.2: every character is an ordinary one.
        let mut nodes = Vec::new();
        for c in pattern.chars() {
            counts.node()?;
            nodes.push(Node::Set(counts.literal(c, flags.i)?));
        }
        return Ok(Parsed {
            node: concat(nodes),
            group_count: 0,
            group_parent: vec![0],
            unknown_blocks: Vec::new(),
        });
    }
    let chars: Vec<char> = if flags.x {
        strip_x(pattern)
    } else {
        pattern.chars().collect()
    };
    let mut p = Parser {
        chars,
        pos: 0,
        flags,
        dialect,
        group_count: 0,
        closed: vec![true],
        open: Vec::new(),
        group_parent: vec![0],
        depth: 0,
        counts,
        unknown_blocks: BTreeSet::new(),
    };
    let node = p.regexp()?;
    if p.pos < p.chars.len() {
        // Only an unbalanced ')' stops the top-level regExp early.
        return Err(p.err("unmatched ')'"));
    }
    Ok(Parsed {
        node,
        group_count: p.group_count,
        group_parent: p.group_parent,
        unknown_blocks: p.unknown_blocks.into_iter().collect(),
    })
}

/// What parsing has built so far, against the ceilings.
#[derive(Default)]
struct Counts {
    nodes: usize,
    set_ranges: usize,
}

impl Counts {
    fn node(&mut self) -> Result<(), Error> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(Error::Limit(format!(
                "the pattern has more than {MAX_NODES} parts"
            )));
        }
        Ok(())
    }

    fn sets(&mut self, n: usize) -> Result<(), Error> {
        self.set_ranges = self.set_ranges.saturating_add(n);
        if self.set_ranges > MAX_SET_RANGES {
            return Err(Error::Limit(format!(
                "the pattern's character sets exceed {MAX_SET_RANGES} ranges"
            )));
        }
        Ok(())
    }

    /// Counts a set parsing built.
    fn built(&mut self, set: CharSet) -> Result<CharSet, Error> {
        self.sets(set.ranges().len() + SET_OVERHEAD)?;
        Ok(set)
    }

    /// A set's case closure under flag `i`, its work counted.
    fn closure(&mut self, set: CharSet, i: bool) -> Result<CharSet, Error> {
        if !i {
            return Ok(set);
        }
        let (closed, visited) = unicode::case_closure(&set);
        self.sets(visited)?;
        Ok(closed)
    }

    /// The set a normal character stands for (with flag `i`, with its
    /// case variants), counted.
    fn literal(&mut self, c: char, i: bool) -> Result<CharSet, Error> {
        let set = self.closure(CharSet::single(c as u32), i)?;
        self.built(set)
    }
}

/// Flag `x` (F&O 3.1 §5.6.2): the whitespace characters #x9, #xA, #xD and
/// #x20 are removed before matching, except inside a character class
/// expression. An escape outside a class may be split by them (`\ s` is
/// `\s`).
fn strip_x(pattern: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(pattern.len());
    let mut depth = 0usize;
    let mut escape_pending = false;
    for c in pattern.chars() {
        let ws = matches!(c, '\t' | '\n' | '\r' | ' ');
        if depth == 0 && ws {
            continue;
        }
        out.push(c);
        if escape_pending {
            escape_pending = false;
        } else if c == '\\' {
            escape_pending = true;
        } else if c == '[' {
            depth += 1;
        } else if c == ']' && depth > 0 {
            depth -= 1;
        }
    }
    out
}

fn concat(mut nodes: Vec<Node>) -> Node {
    match nodes.len() {
        0 => Node::Empty,
        1 => nodes.pop().expect("one node"),
        _ => Node::Concat(nodes),
    }
}

struct Parser<'f> {
    chars: Vec<char>,
    pos: usize,
    flags: &'f Flags,
    dialect: Dialect,
    group_count: usize,
    /// Whether each group has closed (index 0 stands for the whole match).
    closed: Vec<bool>,
    /// The capturing groups open at `pos`, innermost last.
    open: Vec<usize>,
    group_parent: Vec<usize>,
    depth: usize,
    counts: Counts,
    /// The XSD dialect's unknown block names so far: a set, so that a
    /// pattern of many costs time n log n in them, not a scan per name.
    unknown_blocks: BTreeSet<String>,
}

/// A single-character escape, a class escape or a back-reference, as found
/// inside or outside a character class expression.
enum Escape {
    Single(char),
    Class(CharSet),
    Backref(usize),
}

impl Parser<'_> {
    fn err(&self, msg: &str) -> Error {
        Error::Syntax(format!("{msg} at character {}", self.pos))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Error::Limit(format!(
                "the pattern nests deeper than {MAX_DEPTH} levels"
            )));
        }
        Ok(())
    }

    /// regExp ::= branch ( '|' branch )*
    fn regexp(&mut self) -> Result<Node, Error> {
        let mut branches = vec![self.branch()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.branch()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().expect("one branch")
        } else {
            Node::Alt(branches)
        })
    }

    /// branch ::= piece*
    fn branch(&mut self) -> Result<Node, Error> {
        self.counts.node()?;
        let mut pieces = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            pieces.push(self.piece()?);
        }
        Ok(concat(pieces))
    }

    /// piece ::= atom quantifier?, with F&O 3.1 §5.6.1.2's reluctant
    /// quantifiers (a `?` after the quantifier) in the XPath dialect. In
    /// the XSD dialect such a `?` begins the next piece, where it is not
    /// an atom.
    fn piece(&mut self) -> Result<Node, Error> {
        let atom = self.atom()?;
        let (min, max) = match self.peek() {
            Some('?') => (0, Some(1)),
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('{') => {
                self.pos += 1;
                // `quantity` leaves `pos` on the closing '}'.
                self.quantity()?
            }
            _ => return Ok(atom),
        };
        self.pos += 1;
        let greedy = if self.dialect == Dialect::XPath && self.peek() == Some('?') {
            self.pos += 1;
            false
        } else {
            true
        };
        self.counts.node()?;
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        })
    }

    /// A count, saturated to `u32::MAX`, and its digits without leading
    /// zeros, so that XSD's `n <= m` is compared on the numbers as written.
    fn number(&mut self) -> Option<(u32, String)> {
        let start = self.pos;
        let mut n: u64 = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            n = (n * 10 + u64::from(d)).min(u64::from(u32::MAX));
            self.pos += 1;
        }
        let digits: String = self.chars[start..self.pos].iter().collect();
        let digits = digits.trim_start_matches('0').to_string();
        (self.pos > start).then_some((n as u32, digits))
    }

    /// quantity ::= quantRange | quantMin | QuantExact, after the '{'.
    fn quantity(&mut self) -> Result<(u32, Option<u32>), Error> {
        let (min, min_digits) = self.number().ok_or_else(|| self.err("expected a count"))?;
        let max = match self.peek() {
            Some('}') => Some(min),
            Some(',') => {
                self.pos += 1;
                if self.peek() == Some('}') {
                    None
                } else {
                    let (m, m_digits) =
                        self.number().ok_or_else(|| self.err("expected a count"))?;
                    if (m_digits.len(), &m_digits) < (min_digits.len(), &min_digits) {
                        return Err(self.err("a quantifier {n,m} with n greater than m"));
                    }
                    Some(m)
                }
            }
            _ => return Err(self.err("a malformed quantifier")),
        };
        if self.peek() != Some('}') {
            return Err(self.err("a malformed quantifier"));
        }
        Ok((min, max))
    }

    /// atom ::= NormalChar | charClass | ( '(' regExp ')' ), with F&O 3.1
    /// §5.6.1's `^`, `$`, back-references and non-capturing groups in the
    /// XPath dialect. In the XSD dialect `^` and `$` are normal characters
    /// and `(?` opens a group whose first piece begins with `?`, an error.
    fn atom(&mut self) -> Result<Node, Error> {
        self.counts.node()?;
        let c = self.peek().expect("branch stops at the end");
        match c {
            '(' => {
                self.pos += 1;
                self.enter()?;
                let capturing = self.dialect == Dialect::Xsd
                    || !(self.peek() == Some('?') && self.peek_at(1) == Some(':'));
                let nr = if capturing {
                    self.group_count += 1;
                    let n = self.group_count;
                    self.group_parent
                        .push(self.open.last().copied().unwrap_or(0));
                    self.closed.push(false);
                    self.open.push(n);
                    Some(n)
                } else {
                    self.pos += 2;
                    None
                };
                let inner = self.regexp()?;
                if self.peek() != Some(')') {
                    return Err(self.err("a missing ')'"));
                }
                self.pos += 1;
                if let Some(n) = nr {
                    self.open.pop();
                    self.closed[n] = true;
                }
                self.depth -= 1;
                Ok(Node::Group(nr, Box::new(inner)))
            }
            '[' => {
                let set = self.class_expr()?;
                Ok(Node::Set(self.counts.built(set)?))
            }
            '.' => {
                self.pos += 1;
                // Without flag `s`, every character but #xA and #xD.
                let set = if self.flags.s {
                    CharSet::all()
                } else {
                    CharSet::all().difference(&CharSet::from_ranges(vec![(0xA, 0xA), (0xD, 0xD)]))
                };
                Ok(Node::Set(self.counts.built(set)?))
            }
            '^' if self.dialect == Dialect::XPath => {
                self.pos += 1;
                Ok(Node::Bol)
            }
            '$' if self.dialect == Dialect::XPath => {
                self.pos += 1;
                Ok(Node::Eol)
            }
            '\\' => match self.escape(false)? {
                Escape::Single(ch) => Ok(Node::Set(self.counts.literal(ch, self.flags.i)?)),
                Escape::Class(set) => Ok(Node::Set(self.counts.built(set)?)),
                Escape::Backref(n) => Ok(Node::Backref(n)),
            },
            '?' | '*' | '+' | '{' | '}' | ']' => Err(self.err(&format!("an unexpected '{c}'"))),
            // `branch` stops at '|' and ')', so neither reaches here.
            c => {
                self.pos += 1;
                Ok(Node::Set(self.counts.literal(c, self.flags.i)?))
            }
        }
    }

    /// At a backslash: a SingleCharEsc, a charClassEsc, or (in the XPath
    /// dialect, outside a class, `in_class` false) a back-reference.
    fn escape(&mut self, in_class: bool) -> Result<Escape, Error> {
        self.pos += 1;
        let c = self
            .peek()
            .ok_or_else(|| self.err("a trailing backslash"))?;
        self.pos += 1;
        match c {
            'n' => Ok(Escape::Single('\n')),
            'r' => Ok(Escape::Single('\r')),
            't' => Ok(Escape::Single('\t')),
            // [84] SingleCharEsc; F&O 3.1 §5.6.1.1 adds `$` in the XPath
            // dialect.
            '\\' | '|' | '.' | '?' | '*' | '+' | '(' | ')' | '{' | '}' | '-' | '[' | ']' | '^' => {
                Ok(Escape::Single(c))
            }
            '$' if self.dialect == Dialect::XPath => Ok(Escape::Single(c)),
            's' | 'S' | 'i' | 'I' | 'c' | 'C' | 'd' | 'D' | 'w' | 'W' => {
                let set = unicode::class_escape(c.to_ascii_lowercase()).expect("matched above");
                Ok(Escape::Class(if c.is_ascii_uppercase() {
                    set.negate()
                } else {
                    set.clone()
                }))
            }
            'p' | 'P' => {
                if self.peek() != Some('{') {
                    return Err(self.err("'{' expected after \\p"));
                }
                self.pos += 1;
                let start = self.pos;
                while self.peek().is_some_and(|ch| ch != '}') {
                    self.pos += 1;
                }
                if self.peek() != Some('}') {
                    return Err(self.err("an unterminated \\p{"));
                }
                let name: String = self.chars[start..self.pos].iter().collect();
                self.pos += 1;
                let set = if let Some(block) = name.strip_prefix("Is") {
                    // IsBlock ::= 'Is' [a-zA-Z0-9#x2D]+
                    if block.is_empty()
                        || !block
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                    {
                        return Err(self.err("a malformed block name"));
                    }
                    match unicode::block(block) {
                        Some(set) => set,
                        // XSD 1.1 §G.4.2.4: `\p{IsX}` and `\P{IsX}` with an
                        // unrecognized X each denote the set of all
                        // characters, so `\P` is not its complement here.
                        None if self.dialect == Dialect::Xsd => {
                            if !self.unknown_blocks.contains(block) {
                                self.unknown_blocks.insert(block.to_string());
                            }
                            return Ok(Escape::Class(CharSet::all()));
                        }
                        // F&O 3.1 §5.6.1.5: FORX0002.
                        None => return Err(self.err("an unknown block name")),
                    }
                } else {
                    unicode::category(&name)
                        .ok_or_else(|| self.err("an unknown category"))?
                        .clone()
                };
                Ok(Escape::Class(if c == 'P' { set.negate() } else { set }))
            }
            '1'..='9' if !in_class && self.dialect == Dialect::XPath => {
                // F&O 3.1 §5.6.1: the longest run of digits that names a
                // group before this point; that group must have closed.
                let mut n = c.to_digit(10).expect("a digit") as usize;
                while let Some(d) = self.peek().and_then(|ch| ch.to_digit(10)) {
                    let longer = n * 10 + d as usize;
                    if longer > self.group_count {
                        break;
                    }
                    n = longer;
                    self.pos += 1;
                }
                if n > self.group_count || !self.closed[n] {
                    return Err(self.err("a back-reference to a group not closed before it"));
                }
                Ok(Escape::Backref(n))
            }
            _ => Err(self.err(&format!("an invalid escape '\\{c}'"))),
        }
    }

    /// At '[': charClassExpr ::= '[' charGroup ']', with XSD 1.1 §G.4.1's
    /// subtraction and hyphen rules, and flag `i` per F&O 3.1 §5.6.2.
    fn class_expr(&mut self) -> Result<CharSet, Error> {
        self.pos += 1;
        self.enter()?;
        let negated = if self.peek() == Some('^') {
            self.pos += 1;
            true
        } else {
            false
        };
        // The parts' ranges, merged once at the end: a union per part
        // re-sorts the whole set each time, quadratic in the parts.
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut parts = 0usize;
        loop {
            let c = self
                .peek()
                .ok_or_else(|| self.err("an unterminated character class"))?;
            match c {
                ']' => {
                    if parts == 0 {
                        return Err(self.err("an empty character group"));
                    }
                    self.pos += 1;
                    self.depth -= 1;
                    let set = CharSet::from_ranges(ranges);
                    return Ok(if negated { set.negate() } else { set });
                }
                '[' => return Err(self.err("an unescaped '[' in a character class")),
                '-' if self.peek_at(1) == Some('[') => {
                    if parts == 0 {
                        return Err(self.err("a subtraction from an empty group"));
                    }
                    self.pos += 1;
                    let subtracted = self.class_expr()?;
                    if self.peek() != Some(']') {
                        return Err(self.err("']' expected after a subtraction"));
                    }
                    self.pos += 1;
                    self.depth -= 1;
                    let set = CharSet::from_ranges(ranges);
                    let base = if negated { set.negate() } else { set };
                    return Ok(base.difference(&subtracted));
                }
                _ => {
                    let part = self.class_part()?;
                    let part = self.counts.built(part)?;
                    ranges.extend_from_slice(part.ranges());
                    parts += 1;
                }
            }
        }
    }

    /// One charGroupPart: a singleChar, a charRange or a charClassEsc.
    fn class_part(&mut self) -> Result<CharSet, Error> {
        let i = self.flags.i;
        let (first, escaped) = if self.peek() == Some('\\') {
            match self.escape(true)? {
                Escape::Single(ch) => (ch, true),
                Escape::Class(set) => return Ok(set),
                Escape::Backref(_) => unreachable!("escape(true) takes no digits"),
            }
        } else {
            let c = self.peek().expect("class_expr checked");
            self.pos += 1;
            (c, false)
        };
        let single = |p: &mut Self, c: char| p.counts.closure(CharSet::single(c as u32), i);
        if self.peek() != Some('-') {
            return single(self, first);
        }
        match self.peek_at(1) {
            // '-[': a subtraction follows, which the caller parses.
            Some('[') => single(self, first),
            // '-]': the hyphen is a character.
            Some(']') => {
                self.pos += 1;
                Ok(single(self, first)?.union(&CharSet::single('-' as u32)))
            }
            // '--[': the first hyphen is a character, then a subtraction.
            Some('-') if self.peek_at(2) == Some('[') => {
                self.pos += 1;
                Ok(single(self, first)?.union(&CharSet::single('-' as u32)))
            }
            Some(_) => {
                if first == '-' && !escaped {
                    return Err(self.err("an unescaped '-' starting a range"));
                }
                self.pos += 1;
                let end = if self.peek() == Some('\\') {
                    match self.escape(true)? {
                        Escape::Single(ch) => ch,
                        _ => return Err(self.err("a class escape ending a range")),
                    }
                } else {
                    let e = self.peek().expect("peek_at(1) was a character");
                    if e == '-' || e == '[' {
                        return Err(self.err("an invalid range end"));
                    }
                    self.pos += 1;
                    e
                };
                // XSD 1.0 §F.1, which QT3 re00090, re00093 and re00801 pin.
                if end < first {
                    return Err(self.err("a range whose end precedes its start"));
                }
                self.counts
                    .closure(CharSet::range(first as u32, end as u32), i)
            }
            None => Err(self.err("an unterminated character class")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counters_refuse_one_past_their_ceilings() {
        let mut counts = Counts::default();
        for _ in 0..MAX_NODES {
            counts.node().unwrap();
        }
        assert!(matches!(counts.node(), Err(Error::Limit(_))));
        let mut counts = Counts::default();
        counts.sets(MAX_SET_RANGES).unwrap();
        assert!(matches!(counts.sets(1), Err(Error::Limit(_))));
        // A set counts its ranges and its overhead.
        let mut counts = Counts::default();
        counts.built(CharSet::range(1, 2)).unwrap();
        assert_eq!(counts.set_ranges, 1 + SET_OVERHEAD);
    }
}
