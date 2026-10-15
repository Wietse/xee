//! F&O 3.1's operations on a compiled pattern: `fn:matches` (§5.6.3),
//! `fn:replace` (§5.6.4), `fn:tokenize` (§5.6.5) and
//! `fn:analyze-string` (§5.6.6). Each charges what it builds from a match
//! to the call's budget before it builds it, so that no result's time
//! passes the budget, and holds what it builds against the call's memory
//! ceiling before it allocates it; a match's own slots, and the list of
//! its groups the tree is built from, are as large as the pattern's
//! groups and freed with the match.

use crate::backtrack::Slots;
use crate::{copy, reserve, reserve_str, Budget, Call, Error, Need, Regex};

/// An item of `fn:analyze-string`'s result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyzeEntry {
    /// `fn:match`: the matched text, with its groups.
    Match(Vec<MatchEntry>),
    /// `fn:non-match`: text between matches.
    NonMatch(String),
}

/// The content of a match or a group in `fn:analyze-string`'s result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchEntry {
    String(String),
    /// `fn:group`, with its number.
    Group {
        nr: usize,
        value: Vec<MatchEntry>,
    },
}

/// A part of a replacement string (F&O 3.1 §5.6.4): literal text, as a
/// byte range of the replacement, or a group's capture.
enum Repl {
    Lit(usize, usize),
    Group(usize),
}

/// The bounds of slot pair `n`, if both are set.
fn span(slots: &Slots, n: usize) -> Option<(usize, usize)> {
    match (slots[2 * n], slots[2 * n + 1]) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

/// The bounds of the match, which a match always sets.
fn bounds(slots: &Slots) -> (usize, usize) {
    span(slots, 0).expect("a match sets group 0")
}

impl Regex {
    /// `fn:matches`: whether the pattern matches some substring of `hay`.
    pub fn is_match(&self, hay: &str, budget: &mut Budget) -> Result<bool, Error> {
        Ok(self
            .find(&mut Call::new(budget, Need::Exists), hay, 0)?
            .is_some())
    }

    /// `fn:replace`: each non-overlapping match, leftmost first, replaced
    /// by `replacement`, in which `$N` stands for group N and `\$` and
    /// `\\` for `$` and `\` (with flag `q`, the replacement is literal).
    pub fn replace(
        &self,
        hay: &str,
        replacement: &str,
        budget: &mut Budget,
    ) -> Result<String, Error> {
        if self.matches_empty() {
            return Err(Error::MatchesEmptyString);
        }
        let mut call = Call::new(budget, Need::Bounds);
        // Reading the replacement string, charged before it is parsed.
        call.budget.charge(replacement.len() as u64)?;
        let repl = self.replacement(replacement, call.budget)?;
        if repl.iter().any(|r| matches!(r, Repl::Group(n) if *n > 0)) {
            call.need = Need::Groups;
        }
        let part = |r: &Repl, slots: &Slots| match *r {
            Repl::Lit(a, b) => &replacement[a..b],
            Repl::Group(n) => span(slots, n).map_or("", |(a, b)| &hay[a..b]),
        };
        let mut out = String::new();
        let mut last = 0;
        self.for_each_match(&mut call, hay, |slots, budget| {
            let (s, e) = bounds(slots);
            // The bytes this match adds, and a step per part of the
            // replacement, charged and held before they are built.
            let size = s - last + repl.iter().map(|r| part(r, slots).len()).sum::<usize>();
            budget.charge((repl.len() + size) as u64)?;
            reserve_str(&mut out, size, budget)?;
            out.push_str(&hay[last..s]);
            for r in &repl {
                out.push_str(part(r, slots));
            }
            last = e;
            Ok(())
        })?;
        // The text after the last match, charged before it is copied.
        call.budget.charge((hay.len() - last) as u64)?;
        reserve_str(&mut out, hay.len() - last, call.budget)?;
        out.push_str(&hay[last..]);
        Ok(out)
    }

    /// `fn:tokenize`: the substrings of `hay` between the matches. An
    /// empty `hay` gives no substring, whatever the pattern (F&O 3.1
    /// §5.6.5).
    pub fn tokenize(&self, hay: &str, budget: &mut Budget) -> Result<Vec<String>, Error> {
        if hay.is_empty() {
            return Ok(Vec::new());
        }
        if self.matches_empty() {
            return Err(Error::MatchesEmptyString);
        }
        let mut call = Call::new(budget, Need::Bounds);
        let mut out = Vec::new();
        let mut last = 0;
        self.for_each_match(&mut call, hay, |slots, budget| {
            let (s, e) = bounds(slots);
            // A step for the token and one per byte it copies.
            budget.charge((1 + s - last) as u64)?;
            reserve(&mut out, 1, budget)?;
            out.push(copy(&hay[last..s], budget)?);
            last = e;
            Ok(())
        })?;
        call.budget.charge((1 + hay.len() - last) as u64)?;
        reserve(&mut out, 1, call.budget)?;
        out.push(copy(&hay[last..], call.budget)?);
        Ok(out)
    }

    /// `fn:analyze-string`: `hay` as an alternation of matches, with their
    /// groups, and the text between them.
    pub fn analyze(&self, hay: &str, budget: &mut Budget) -> Result<Vec<AnalyzeEntry>, Error> {
        if self.matches_empty() {
            return Err(Error::MatchesEmptyString);
        }
        let mut call = Call::new(budget, Need::Groups);
        let mut out = Vec::new();
        let mut last = 0;
        self.for_each_match(&mut call, hay, |slots, budget| {
            let (s, e) = bounds(slots);
            // The text before and in the match, copied once; the group tree
            // is charged as it is built.
            budget.charge((1 + e - last) as u64)?;
            if s > last {
                reserve(&mut out, 1, budget)?;
                out.push(AnalyzeEntry::NonMatch(copy(&hay[last..s], budget)?));
            }
            let tree = self.group_tree(hay, slots, budget)?;
            reserve(&mut out, 1, budget)?;
            out.push(AnalyzeEntry::Match(tree));
            last = e;
            Ok(())
        })?;
        let rest = hay.len() - last;
        if rest > 0 {
            call.budget.charge(1 + rest as u64)?;
            reserve(&mut out, 1, call.budget)?;
            out.push(AnalyzeEntry::NonMatch(copy(&hay[last..], call.budget)?));
        }
        Ok(out)
    }

    /// Calls `f` with the slots of each successive non-overlapping match
    /// and the call's budget, which `f` charges for its work before it
    /// does it.
    pub(crate) fn for_each_match(
        &self,
        call: &mut Call,
        hay: &str,
        mut f: impl FnMut(&Slots, &mut Budget) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut pos = 0;
        while pos <= hay.len() {
            let Some(slots) = self.find(call, hay, pos)? else {
                break;
            };
            let (s, e) = bounds(&slots);
            f(&slots, call.budget)?;
            if e > s {
                pos = e;
            } else {
                // Not reached by the operations, which refuse a pattern
                // that matches the empty string: past an empty match, the
                // next search starts a character later.
                match hay[e..].chars().next() {
                    Some(c) => pos = e + c.len_utf8(),
                    None => break,
                }
            }
        }
        Ok(())
    }

    /// The replacement string's parts (F&O 3.1 §5.6.4), their memory held
    /// as they are added. After a `$`, N is all the digits; while N > S
    /// and N > 9 its last digit is literal; S < N <= 9 is the empty string.
    fn replacement(&self, repl: &str, budget: &mut Budget) -> Result<Vec<Repl>, Error> {
        fn add(out: &mut Vec<Repl>, part: Repl, budget: &mut Budget) -> Result<(), Error> {
            reserve(out, 1, budget)?;
            out.push(part);
            Ok(())
        }
        let mut out = Vec::new();
        if self.flags.q {
            if !repl.is_empty() {
                add(&mut out, Repl::Lit(0, repl.len()), budget)?;
            }
            return Ok(out);
        }
        let bytes = repl.as_bytes();
        // The largest N the stripping keeps: a group's number, or a single
        // digit, which always counts.
        let most = (self.group_count as u64).max(9);
        // Where the literal text not yet added begins.
        let mut lit = 0;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    if !matches!(bytes.get(i + 1), Some(b'\\' | b'$')) {
                        return Err(Error::InvalidReplacement(format!(
                            "a \\ at byte {i} is not followed by \\ or $"
                        )));
                    }
                    if i > lit {
                        add(&mut out, Repl::Lit(lit, i), budget)?;
                    }
                    // The escaped character is the literal.
                    lit = i + 1;
                    i += 2;
                }
                b'$' => {
                    let digits = i + 1;
                    let mut end = digits;
                    // A prefix's number never decreases as a digit is
                    // added, so the longest prefix within `most` is the N
                    // the stripping leaves.
                    let (mut value, mut n, mut kept) = (0u64, 0u64, digits);
                    while end < bytes.len() && bytes[end].is_ascii_digit() {
                        if value <= most {
                            value = value * 10 + u64::from(bytes[end] - b'0');
                            if value <= most {
                                (n, kept) = (value, end + 1);
                            }
                        }
                        end += 1;
                    }
                    if end == digits {
                        return Err(Error::InvalidReplacement(format!(
                            "a $ at byte {i} is not followed by a digit"
                        )));
                    }
                    if i > lit {
                        add(&mut out, Repl::Lit(lit, i), budget)?;
                    }
                    if n <= self.group_count as u64 {
                        add(&mut out, Repl::Group(n as usize), budget)?;
                    }
                    // The stripped digits begin the next literal text.
                    lit = kept;
                    i = end;
                }
                _ => i += 1,
            }
        }
        if bytes.len() > lit {
            add(&mut out, Repl::Lit(lit, bytes.len()), budget)?;
        }
        Ok(out)
    }

    /// The match's content as text and `fn:group` entries (F&O 3.1
    /// §5.6.6), its memory held as it is built. A group nests under its
    /// nearest enclosing group that took part in the match and whose
    /// capture contains it: a capture from an earlier iteration of an
    /// enclosing repetition lies outside it. Siblings never overlap, since
    /// every capture lies on the one path the match took through the input.
    fn group_tree(
        &self,
        hay: &str,
        slots: &Slots,
        budget: &mut Budget,
    ) -> Result<Vec<MatchEntry>, Error> {
        // Each group that took part, as (parent, start, end, number), so
        // that a group's children are a run in the order they are emitted.
        // It is freed with the match, and no longer than the groups. A step
        // for each group looked at, and one for each enclosing group its
        // walk up visits, charged as the walk goes: a group nested in the
        // last capture of its parent stops at once, so a deep pattern pays
        // for the walks it takes, not for its depth.
        budget.charge(self.group_count as u64)?;
        let mut placed = Vec::new();
        for n in 1..=self.group_count {
            let Some((a, b)) = span(slots, n) else {
                continue;
            };
            let mut p = self.group_parent[n];
            while p != 0 {
                budget.charge(1)?;
                match span(slots, p) {
                    Some((pa, pb)) if pa <= a && b <= pb => break,
                    _ => p = self.group_parent[p],
                }
            }
            placed.push((p, a, b, n));
        }
        // Sorting them: k (1 + log2 k) steps for k groups.
        let k = placed.len();
        budget.charge((k * (1 + k.max(1).ilog2() as usize)) as u64)?;
        placed.sort_unstable();
        fn emit(
            hay: &str,
            placed: &[(usize, usize, usize, usize)],
            parent: usize,
            (s, e): (usize, usize),
            budget: &mut Budget,
        ) -> Result<Vec<MatchEntry>, Error> {
            let from = placed.partition_point(|&(p, ..)| p < parent);
            let to = placed.partition_point(|&(p, ..)| p <= parent);
            let mut out = Vec::new();
            let mut cursor = s;
            for &(_, a, b, n) in &placed[from..to] {
                debug_assert!(
                    cursor <= a && b <= e,
                    "groups overlap or leave their parent"
                );
                if a > cursor {
                    reserve(&mut out, 1, budget)?;
                    out.push(MatchEntry::String(copy(&hay[cursor..a], budget)?));
                }
                let value = emit(hay, placed, n, (a, b), budget)?;
                reserve(&mut out, 1, budget)?;
                out.push(MatchEntry::Group { nr: n, value });
                cursor = b;
            }
            if cursor < e {
                reserve(&mut out, 1, budget)?;
                out.push(MatchEntry::String(copy(&hay[cursor..e], budget)?));
            }
            Ok(out)
        }
        emit(hay, &placed, 0, bounds(slots), budget)
    }
}

#[cfg(test)]
mod tests;
