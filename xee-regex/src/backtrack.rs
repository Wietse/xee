//! A backtracking interpreter for a [`Program`]: an explicit stack of
//! choice points and undo records, each instruction a step charged to the
//! call's budget, and the stack's growth held against the call's memory.
//! It runs the programs with back-references.

use crate::compile::{Inst, Program};
use crate::{chunk, prefilter, reserve, unicode, Budget, Error, Flags};

/// The capture slots of a match: `2n` and `2n + 1` hold where group `n`
/// starts and ends, in bytes, and group 0 is the match.
pub(crate) type Slots = Vec<Option<usize>>;

/// Three words: an instruction pushes at most one frame for each step it
/// is charged.
pub(crate) enum Frame {
    /// Resume at the instruction from the byte position.
    Alt(usize, usize),
    /// Restore a capture slot.
    Slot(usize, Option<usize>),
    /// Restore a progress register.
    Reg(usize, usize),
}

/// Scratch memory for one call, reused across its searches and start
/// positions.
pub(crate) struct Cache {
    slots: Slots,
    regs: Vec<usize>,
    stack: Vec<Frame>,
}

impl Cache {
    /// Allocating the slots and registers is charged a step each, and held.
    pub(crate) fn new(prog: &Program, budget: &mut Budget) -> Result<Cache, Error> {
        budget.charge((prog.slots + prog.regs) as u64)?;
        budget.hold(
            chunk(prog.slots * size_of::<Option<usize>>()) + chunk(prog.regs * size_of::<usize>()),
        )?;
        Ok(Cache {
            slots: vec![None; prog.slots],
            regs: vec![usize::MAX; prog.regs],
            stack: Vec::new(),
        })
    }
}

/// The leftmost match starting at or after byte offset `start`, which is
/// on a character boundary, tried only where `prefix`, the literal every
/// match begins with, occurs.
pub(crate) fn search(
    prog: &Program,
    hay: &str,
    start: usize,
    prefix: &str,
    flags: &Flags,
    budget: &mut Budget,
    cache: &mut Cache,
) -> Result<Option<Slots>, Error> {
    let mut at = start;
    loop {
        if !prefix.is_empty() {
            match prefilter::find_literal(hay, at, prefix, budget)? {
                Some(next) => at = next,
                None => return Ok(None),
            }
        }
        if run_at(prog, hay, at, flags, budget, cache)? {
            // Copying the match's slots out of the cache.
            budget.charge(prog.slots as u64)?;
            return Ok(Some(cache.slots.clone()));
        }
        match hay[at..].chars().next() {
            Some(c) => at += c.len_utf8(),
            None => return Ok(None),
        }
    }
}

fn push(stack: &mut Vec<Frame>, frame: Frame, budget: &mut Budget) -> Result<(), Error> {
    reserve(stack, 1, budget)?;
    stack.push(frame);
    Ok(())
}

/// Whether the program matches from `start`, leaving the match's slots in
/// `cache.slots`.
fn run_at(
    prog: &Program,
    hay: &str,
    start: usize,
    flags: &Flags,
    budget: &mut Budget,
    cache: &mut Cache,
) -> Result<bool, Error> {
    let bytes = hay.as_bytes();
    let Cache { slots, regs, stack } = cache;
    // Every start position sets up all slots and registers.
    budget.charge((prog.slots + prog.regs) as u64)?;
    slots.fill(None);
    regs.fill(usize::MAX);
    stack.clear();
    let mut pc = 0;
    let mut pos = start;
    loop {
        budget.charge(1)?;
        let ok = match prog.insts[pc] {
            Inst::Set(k) => match hay[pos..].chars().next() {
                Some(c)
                    if {
                        budget.charge(prog.set_costs[k])?;
                        prog.sets[k].contains(c as u32)
                    } =>
                {
                    pos += c.len_utf8();
                    pc += 1;
                    true
                }
                _ => false,
            },
            Inst::Split(a, b) => {
                push(stack, Frame::Alt(b, pos), budget)?;
                pc = a;
                true
            }
            Inst::Jmp(t) => {
                pc = t;
                true
            }
            Inst::Save(k) => {
                budget.charge(1)?;
                push(stack, Frame::Slot(k, slots[k]), budget)?;
                slots[k] = Some(pos);
                pc += 1;
                true
            }
            Inst::Bol => {
                pc += 1;
                at_line_start(bytes, pos, flags.m)
            }
            Inst::Eol => {
                pc += 1;
                at_line_end(bytes, pos, flags.m)
            }
            Inst::Backref(n) => match (slots[2 * n], slots[2 * n + 1]) {
                (Some(a), Some(b)) => {
                    // The captured text is charged whole, before it is
                    // compared.
                    budget.charge((b - a) as u64)?;
                    match backref_end(hay, a, b, pos, flags.i) {
                        Some(end) => {
                            pos = end;
                            pc += 1;
                            true
                        }
                        None => false,
                    }
                }
                // F&O 3.1 §5.6.1.4: a group that matched nothing matches
                // the empty string.
                _ => {
                    pc += 1;
                    true
                }
            },
            Inst::ProgressStart(r) => {
                budget.charge(1)?;
                push(stack, Frame::Reg(r, regs[r]), budget)?;
                regs[r] = pos;
                pc += 1;
                true
            }
            Inst::ProgressCheck(r, exit) => {
                pc = if regs[r] == pos { exit } else { pc + 1 };
                true
            }
            Inst::ClearGroups(lo, hi) => {
                budget.charge(2 * (hi - lo + 1) as u64)?;
                for (k, slot) in slots.iter_mut().enumerate().take(2 * hi + 2).skip(2 * lo) {
                    push(stack, Frame::Slot(k, *slot), budget)?;
                    *slot = None;
                }
                pc += 1;
                true
            }
            Inst::Match => return Ok(true),
        };
        if ok {
            continue;
        }
        loop {
            match stack.pop() {
                None => return Ok(false),
                Some(Frame::Slot(k, v)) => slots[k] = v,
                Some(Frame::Reg(r, v)) => regs[r] = v,
                Some(Frame::Alt(p, q)) => {
                    pc = p;
                    pos = q;
                    break;
                }
            }
        }
    }
}

/// F&O 3.1 §5.6.2: `^` matches at the start, and with `m` also after a
/// newline other than one that ends the input.
pub(crate) fn at_line_start(bytes: &[u8], pos: usize, multiline: bool) -> bool {
    pos == 0 || (multiline && bytes[pos - 1] == b'\n' && pos < bytes.len())
}

/// F&O 3.1 §5.6.2: `$` matches at the end, and with `m` before a newline,
/// and at the end only if no newline ends the input.
pub(crate) fn at_line_end(bytes: &[u8], pos: usize, multiline: bool) -> bool {
    if !multiline {
        pos == bytes.len()
    } else if pos == bytes.len() {
        bytes.last() != Some(&b'\n')
    } else {
        bytes[pos] == b'\n'
    }
}

/// Where the text `hay[a..b]` a group captured ends when it is matched
/// again at `pos`: each character the same or, with `i`, a case variant
/// (F&O 3.1 §5.6.2).
fn backref_end(hay: &str, a: usize, b: usize, pos: usize, case_blind: bool) -> Option<usize> {
    let mut p = pos;
    for c in hay[a..b].chars() {
        let d = hay[p..].chars().next()?;
        if c != d && !(case_blind && unicode::is_case_variant(c, d)) {
            return None;
        }
        p += d.len_utf8();
    }
    Some(p)
}
