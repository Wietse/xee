//! The parsed tree compiled to one program over characters, which both
//! matchers run. Counted repetition is expanded; where the body can match
//! the empty string, each optional iteration (one past the minimum, in a
//! loop or in a counted copy) carries a progress check, so that an optional
//! iteration that matched the empty string ends the repetition. Required
//! iterations run regardless, and a counted repetition's last copy carries
//! none, since the repetition ends after it anyway. `X{0,n}` then matches
//! nothing `X*` does not, as `X{n,}` matches what `X` n times and then
//! `X*` does.

use std::collections::HashMap;
use std::fmt;

use crate::charset::CharSet;
use crate::parse::Node;

/// Ceiling: instructions after expansion. A larger program is refused,
/// not run another way.
pub(crate) const MAX_INSTS: usize = 1 << 20;

/// Ceiling: nodes compiled, counting each copy of a counted repetition.
/// Copies of a node that emits no instruction cost time and nothing else:
/// `(?:(?:(?:(?:){1000}){1000}){1000}){1000}` is 10^12 visits.
pub(crate) const MAX_VISITS: usize = 8 * MAX_INSTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Inst {
    /// Consume a character in the set `sets[k]`.
    Set(usize),
    /// Try the first target, then the second.
    Split(usize, usize),
    Jmp(usize),
    /// Record the position in a capture slot: `2n` opens group `n`, `2n + 1`
    /// closes it, and group 0 is the match.
    Save(usize),
    Bol,
    Eol,
    Backref(usize),
    /// An optional iteration of a repetition whose body can match the empty
    /// string starts: remember the position in the repetition's register.
    ProgressStart(usize),
    /// The iteration ends: if it matched the empty string, it ends the
    /// repetition and execution continues at the second target, keeping its
    /// captures; otherwise at the next instruction.
    ProgressCheck(usize, usize),
    /// Unset the captures of groups `lo..=hi`: a quantifier took no
    /// iteration.
    ClearGroups(usize, usize),
    Match,
}

#[derive(Debug)]
pub(crate) struct Program {
    pub(crate) insts: Vec<Inst>,
    pub(crate) sets: Vec<CharSet>,
    /// Capture slots: two for the match and two per group.
    pub(crate) slots: usize,
    /// Progress registers: one per nesting level of checked iterations (a
    /// loop's whose body can match the empty string, or such a counted
    /// repetition's optional copies but the last), shared by siblings and by
    /// the copies of one counted repetition. A register is live only inside
    /// one checked iteration's body, and the next is entered only after it
    /// is left. A repetition nested in a counted repetition's last copy,
    /// where the copies' register is no longer live, takes that register's
    /// number.
    pub(crate) regs: usize,
    /// A back-reference anywhere: the program needs the backtracker.
    pub(crate) has_backrefs: bool,
}

/// The program is over a ceiling.
#[derive(Debug)]
pub(crate) struct TooBig;

/// What compiling needs to know of the tree, from one pass over it.
pub(crate) struct Analysis {
    /// The length in characters of the shortest string the pattern
    /// matches, saturating.
    pub(crate) min_len: u64,
    /// Each repetition body's `RepeatInfo`, by the body's address.
    bodies: HashMap<*const Node, RepeatInfo>,
}

/// A repetition body's lowest and highest capturing group, and whether it
/// can match the empty string.
type RepeatInfo = (Option<(usize, usize)>, bool);

/// One pass over the tree, bottom up: its minimum length, and each
/// repetition body's groups and whether it can match the empty string,
/// so that compiling a body's copies walks it no further.
pub(crate) fn analyse(node: &Node) -> Analysis {
    fn walk(
        node: &Node,
        bodies: &mut HashMap<*const Node, RepeatInfo>,
    ) -> (u64, Option<(usize, usize)>) {
        let join = |a: Option<(usize, usize)>, b: Option<(usize, usize)>| match (a, b) {
            (Some((l1, h1)), Some((l2, h2))) => Some((l1.min(l2), h1.max(h2))),
            (x, None) | (None, x) => x,
        };
        match node {
            Node::Empty | Node::Bol | Node::Eol | Node::Backref(_) => (0, None),
            Node::Set(_) => (1, None),
            Node::Group(nr, inner) => {
                let (len, groups) = walk(inner, bodies);
                (len, join(nr.map(|n| (n, n)), groups))
            }
            Node::Concat(v) => v.iter().fold((0, None), |(len, groups), n| {
                let (l, g) = walk(n, bodies);
                (len.saturating_add(l), join(groups, g))
            }),
            Node::Alt(v) => {
                let mut len = u64::MAX;
                let mut groups = None;
                for n in v {
                    let (l, g) = walk(n, bodies);
                    len = len.min(l);
                    groups = join(groups, g);
                }
                (if v.is_empty() { 0 } else { len }, groups)
            }
            Node::Repeat {
                node: body, min, ..
            } => {
                let (len, groups) = walk(body, bodies);
                bodies.insert(&**body as *const Node, (groups, len == 0));
                (u64::from(*min).saturating_mul(len), groups)
            }
        }
    }
    let mut bodies = HashMap::new();
    let (min_len, _) = walk(node, &mut bodies);
    Analysis { min_len, bodies }
}

/// Compiles the tree. Each distinct character set moves from the tree into
/// the program, which the copies of a counted repetition share, so the
/// sets are held once.
pub(crate) fn compile(
    node: &mut Node,
    group_count: usize,
    analysis: &Analysis,
) -> Result<Program, TooBig> {
    let mut c = Compiler {
        insts: Vec::new(),
        sets: Vec::new(),
        set_index: HashMap::new(),
        bodies: &analysis.bodies,
        visits: 0,
        regs: 0,
        loop_depth: 0,
        has_backrefs: false,
    };
    c.push(Inst::Save(0))?;
    c.node(node)?;
    c.push(Inst::Save(1))?;
    c.push(Inst::Match)?;
    c.insts.shrink_to_fit();
    Ok(Program {
        insts: c.insts,
        sets: c.sets,
        slots: 2 * (group_count + 1),
        regs: c.regs,
        has_backrefs: c.has_backrefs,
    })
}

struct Compiler<'a> {
    insts: Vec<Inst>,
    sets: Vec<CharSet>,
    /// A set's index in `sets`, by its address in the parsed tree: the
    /// copies of a counted repetition share one set, so memory follows the
    /// pattern and the instruction count, not their product.
    set_index: HashMap<*const CharSet, usize>,
    /// Each repetition body's `RepeatInfo` (`analyse`).
    bodies: &'a HashMap<*const Node, RepeatInfo>,
    visits: usize,
    regs: usize,
    loop_depth: usize,
    has_backrefs: bool,
}

impl Compiler<'_> {
    fn push(&mut self, inst: Inst) -> Result<usize, TooBig> {
        if self.insts.len() >= MAX_INSTS {
            return Err(TooBig);
        }
        self.insts.push(inst);
        Ok(self.insts.len() - 1)
    }

    fn here(&self) -> usize {
        self.insts.len()
    }

    fn node(&mut self, node: &mut Node) -> Result<(), TooBig> {
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return Err(TooBig);
        }
        match node {
            Node::Empty => {}
            Node::Set(s) => {
                // The first copy takes the set out of the tree; the others
                // find it by the address it leaves behind.
                let next = self.sets.len();
                let k = *self.set_index.entry(s as *const CharSet).or_insert(next);
                if k == next {
                    self.sets.push(std::mem::take(s));
                }
                self.push(Inst::Set(k))?;
            }
            Node::Bol => {
                self.push(Inst::Bol)?;
            }
            Node::Eol => {
                self.push(Inst::Eol)?;
            }
            Node::Backref(n) => {
                self.has_backrefs = true;
                self.push(Inst::Backref(*n))?;
            }
            Node::Group(Some(n), inner) => {
                let n = *n;
                self.push(Inst::Save(2 * n))?;
                self.node(inner)?;
                self.push(Inst::Save(2 * n + 1))?;
            }
            Node::Group(None, inner) => self.node(inner)?,
            Node::Concat(v) => {
                for n in v {
                    self.node(n)?;
                }
            }
            Node::Alt(v) => {
                let mut jumps = Vec::new();
                let last = v.len().saturating_sub(1);
                for (k, alt) in v.iter_mut().enumerate() {
                    if k < last {
                        let split = self.push(Inst::Split(0, 0))?;
                        self.node(alt)?;
                        jumps.push(self.push(Inst::Jmp(0))?);
                        let next = self.here();
                        self.insts[split] = Inst::Split(split + 1, next);
                    } else {
                        self.node(alt)?;
                    }
                }
                let end = self.here();
                for j in jumps {
                    self.insts[j] = Inst::Jmp(end);
                }
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => self.repeat(node, *min, *max, *greedy)?,
        }
        Ok(())
    }

    fn repeat(
        &mut self,
        node: &mut Node,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    ) -> Result<(), TooBig> {
        let key: *const Node = node;
        let (groups, can_be_empty) = self.bodies[&key];
        // A quantifier that takes no iteration unsets the groups inside it:
        // the exit taken before any iteration clears them.
        let zero_clear = match groups {
            Some((lo, hi)) if min == 0 => Some(Inst::ClearGroups(lo, hi)),
            _ => None,
        };
        for _ in 0..min {
            self.node(node)?;
        }
        let order = |taken: usize, skipped: usize| {
            if greedy {
                Inst::Split(taken, skipped)
            } else {
                Inst::Split(skipped, taken)
            }
        };
        match max {
            None => {
                // With `zero_clear`, the first split enters the body or takes
                // the clearing exit; the loop's own split never clears.
                let first = match zero_clear {
                    Some(_) => Some(self.push(Inst::Split(0, 0))?),
                    None => None,
                };
                let split = self.push(Inst::Split(0, 0))?;
                let body = split + 1;
                let reg = if can_be_empty {
                    let r = self.loop_depth;
                    self.regs = self.regs.max(r + 1);
                    self.push(Inst::ProgressStart(r))?;
                    Some(r)
                } else {
                    None
                };
                self.loop_depth += usize::from(reg.is_some());
                let compiled = self.node(node);
                self.loop_depth -= usize::from(reg.is_some());
                compiled?;
                let check = match reg {
                    Some(r) => Some(self.push(Inst::ProgressCheck(r, 0))?),
                    None => None,
                };
                self.push(Inst::Jmp(split))?;
                let clear = self.here();
                if let Some(c) = zero_clear {
                    self.push(c)?;
                }
                let end = self.here();
                if let (Some(c), Some(r)) = (check, reg) {
                    self.insts[c] = Inst::ProgressCheck(r, end);
                }
                self.insts[split] = order(body, end);
                if let Some(first) = first {
                    self.insts[first] = order(body, clear);
                }
            }
            Some(max) => {
                // The optional copies share one register, as sibling loops
                // do: each copy's body is entered only after the previous
                // one's is left. The last copy needs no check: the
                // repetition ends after it whether it matched the empty
                // string or not.
                let reg = can_be_empty.then_some(self.loop_depth);
                let mut splits = Vec::new();
                let mut checks = Vec::new();
                for k in min..max {
                    let checked = reg.filter(|_| k + 1 < max);
                    let split = self.push(Inst::Split(0, 0))?;
                    if let Some(r) = checked {
                        self.push(Inst::ProgressStart(r))?;
                    }
                    let body = self.here();
                    self.loop_depth += usize::from(checked.is_some());
                    let compiled = self.node(node);
                    self.loop_depth -= usize::from(checked.is_some());
                    compiled?;
                    if self.here() == body {
                        // The body emits nothing (and so has no group): a
                        // split between two equal targets would only make
                        // a backtracker try the rest twice.
                        self.insts.truncate(split);
                        break;
                    }
                    if let Some(r) = checked {
                        checks.push(self.push(Inst::ProgressCheck(r, 0))?);
                    }
                    splits.push(split);
                }
                if splits.is_empty() {
                    // No optional copy, as in `{0}` or `{n}`: nothing to
                    // skip and, the body never running, nothing to clear.
                    return Ok(());
                }
                if let Some(r) = reg.filter(|_| !checks.is_empty()) {
                    self.regs = self.regs.max(r + 1);
                }
                // With `zero_clear`, the first split's exit clears; the
                // later ones jump past that block.
                let (clear, end) = match zero_clear {
                    Some(c) => {
                        let jmp = self.push(Inst::Jmp(0))?;
                        let clear = self.here();
                        self.push(c)?;
                        let end = self.here();
                        self.insts[jmp] = Inst::Jmp(end);
                        (clear, end)
                    }
                    None => (self.here(), self.here()),
                };
                for (k, s) in splits.into_iter().enumerate() {
                    self.insts[s] = order(s + 1, if k == 0 { clear } else { end });
                }
                // An optional copy that matched the empty string ends the
                // repetition, keeping its captures, as a loop's iteration
                // does.
                if let Some(r) = reg {
                    for c in checks {
                        self.insts[c] = Inst::ProgressCheck(r, end);
                    }
                }
            }
        }
        Ok(())
    }
}

/// A listing for tests and debugging, its format not stable: a header,
/// each set once, then one instruction per line. Its size is that of the
/// sets plus a line per instruction.
impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "slots {} registers {}{}",
            self.slots,
            self.regs,
            if self.has_backrefs {
                " back-references"
            } else {
                ""
            }
        )?;
        for (k, set) in self.sets.iter().enumerate() {
            let ranges: Vec<String> = set
                .ranges()
                .iter()
                .map(|&(a, b)| {
                    if a == b {
                        format!("{a:X}")
                    } else {
                        format!("{a:X}-{b:X}")
                    }
                })
                .collect();
            writeln!(f, "set {k} = [{}]", ranges.join(" "))?;
        }
        for (pc, inst) in self.insts.iter().enumerate() {
            write!(f, "{pc}: ")?;
            match *inst {
                Inst::Set(k) => writeln!(f, "set {k}")?,
                Inst::Split(a, b) => writeln!(f, "split {a} {b}")?,
                Inst::Jmp(a) => writeln!(f, "jmp {a}")?,
                Inst::Save(s) => writeln!(f, "save {s}")?,
                Inst::Bol => writeln!(f, "bol")?,
                Inst::Eol => writeln!(f, "eol")?,
                Inst::Backref(n) => writeln!(f, "backref {n}")?,
                Inst::ProgressStart(r) => writeln!(f, "progress-start {r}")?,
                Inst::ProgressCheck(r, exit) => writeln!(f, "progress-check {r} {exit}")?,
                Inst::ClearGroups(lo, hi) => writeln!(f, "clear {lo} {hi}")?,
                Inst::Match => writeln!(f, "match")?,
            }
        }
        Ok(())
    }
}
