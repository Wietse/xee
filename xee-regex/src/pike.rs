//! A PikeVM for a [`Program`] without back-references (Thompson; Cox,
//! "Regular Expression Matching: the Virtual Machine Approach"): one pass
//! over the input, at most one thread per instruction at each position,
//! threads kept in priority order, so that the match it reports is the
//! one the backtracker finds first: a match found replaces the one before
//! it, from a thread of lower priority, and ends the threads below it.
//!
//! Without back-references, a thread's future depends on its instruction,
//! its position and its progress registers, and a register matters only
//! as "the current iteration of this repetition began at this position",
//! which decides whether an iteration that ends here matched the empty
//! string and so ends the repetition. Registers are numbered by nesting
//! depth, and an inner iteration begins no earlier than the outer one, so
//! those flags read false, ..., false, true, ..., true from the outermost
//! live repetition inward: their number `s` says all of it. The closure
//! therefore tells threads apart by (instruction, `s`), and the thread
//! lists by the instruction alone, since every flag is false once a
//! character is consumed. Telling them apart by the instruction alone
//! would drop an empty iteration that a thread enters after another
//! reached the same instruction at the same position, and with it the
//! captures the backtracker reports.

use crate::compile::{Inst, Program};
use crate::{chunk, prefilter, reserve, Budget, Error, Flags};

/// An unset slot.
pub(crate) const NONE: usize = usize::MAX;

/// A set of small integers with constant-time insertion, membership and
/// clearing, which remembers the order of insertion.
struct SparseSet {
    dense: Vec<usize>,
    sparse: Vec<usize>,
    len: usize,
}

impl SparseSet {
    fn new(n: usize) -> SparseSet {
        SparseSet {
            dense: vec![0; n],
            sparse: vec![0; n],
            len: 0,
        }
    }

    /// Inserts `x`; false if it was there.
    fn insert(&mut self, x: usize) -> bool {
        let i = self.sparse[x];
        if i < self.len && self.dense[i] == x {
            return false;
        }
        self.dense[self.len] = x;
        self.sparse[x] = self.len;
        self.len += 1;
        true
    }

    fn clear(&mut self) {
        self.len = 0;
    }
}

struct Threads {
    /// The keys `pc * depths + s` visited at this position, in priority
    /// order; a consuming instruction or `Match` is keyed with `s = 0`.
    set: SparseSet,
    /// Row `row_of[pc]` holds the capture slots of the thread at `pc`.
    table: Vec<usize>,
}

/// The steps copying or clearing a row of `width` slots costs: a step per
/// eight words, which the copy moves as one block.
fn copy_cost(width: usize) -> u64 {
    width.div_ceil(8) as u64
}

enum Frame {
    Explore(usize, usize),
    Restore(usize, usize),
}

/// Scratch memory for one call, reused across its searches.
pub(crate) struct Cache {
    /// The capture slots a thread carries: none when the call asks only
    /// whether there is a match, two for its bounds, or all.
    width: usize,
    depths: usize,
    clist: Threads,
    nlist: Threads,
    stack: Vec<Frame>,
    curr: Vec<usize>,
    /// The slots of the best match found so far.
    found: Vec<usize>,
}

impl Cache {
    /// The two thread lists, each a slot row per class or `Match` and the
    /// closure's set over instructions times loop depths, two arrays; and
    /// the current thread's and the match's slots: held against the call's
    /// memory and charged a step per word, before they are allocated.
    pub(crate) fn new(prog: &Program, width: usize, budget: &mut Budget) -> Result<Cache, Error> {
        let depths = prog.regs + 1;
        let word = size_of::<usize>();
        let set = prog.insts.len().saturating_mul(depths);
        let table = prog.rows.saturating_mul(width);
        let list = chunk(set.saturating_mul(word))
            .saturating_mul(2)
            .saturating_add(chunk(table.saturating_mul(word)));
        budget.hold(
            list.saturating_mul(2)
                .saturating_add(chunk(width * word).saturating_mul(2)),
        )?;
        let words = set
            .saturating_mul(2)
            .saturating_add(table)
            .saturating_mul(2)
            .saturating_add(2 * width);
        budget.charge(words as u64)?;
        let threads = || Threads {
            set: SparseSet::new(set),
            table: vec![NONE; table],
        };
        Ok(Cache {
            width,
            depths,
            clist: threads(),
            nlist: threads(),
            stack: Vec::new(),
            curr: vec![NONE; width],
            found: vec![NONE; width],
        })
    }

    /// The slots of the match the last search found.
    pub(crate) fn found(&self) -> &[usize] {
        &self.found
    }
}

/// Whether there is a match starting at or after byte offset `start`,
/// which is on a character boundary; the leftmost's tracked slots are
/// then [`Cache::found`] (`NONE` where unset). While no thread is alive, the
/// search goes on where `prefix`, the literal every match begins with,
/// next occurs; when `anchored`, every match begins at `start`, and the
/// search ends there.
#[expect(
    clippy::too_many_arguments,
    reason = "what the prefilter found is passed apart from the program"
)]
pub(crate) fn search(
    prog: &Program,
    hay: &str,
    start: usize,
    prefix: &str,
    anchored: bool,
    flags: &Flags,
    budget: &mut Budget,
    cache: &mut Cache,
) -> Result<bool, Error> {
    let (width, depths) = (cache.width, cache.depths);
    cache.clist.set.clear();
    cache.nlist.set.clear();
    let mut matched = false;
    let mut pos = start;
    loop {
        if !matched && !prefix.is_empty() && cache.clist.set.len == 0 {
            match prefilter::find_literal(hay, pos, prefix, budget)? {
                Some(next) => pos = next,
                None => return Ok(false),
            }
        }
        // Each position the search reaches is a step, threads or none.
        budget.charge(1)?;
        // No thread begins past an anchored start, so the search ends when
        // the last one dies.
        if !matched && (pos == start || !anchored) {
            // A new thread at this position, below every earlier one.
            budget.charge(copy_cost(width))?;
            cache.curr.fill(NONE);
            closure(prog, hay, pos, flags, budget, cache, 0, true)?;
        }
        if cache.clist.set.len == 0 {
            break;
        }
        let next = hay[pos..].chars().next();
        let next_pos = pos + next.map_or(0, char::len_utf8);
        for i in 0..cache.clist.set.len {
            let key = cache.clist.set.dense[i];
            budget.charge(1)?;
            if !key.is_multiple_of(depths) {
                continue;
            }
            let pc = key / depths;
            match prog.insts[pc] {
                Inst::Set(k) => {
                    let Some(c) = next else {
                        continue;
                    };
                    budget.charge(prog.set_costs[k])?;
                    if !prog.sets[k].contains(c as u32) {
                        continue;
                    }
                    budget.charge(copy_cost(width))?;
                    let row = prog.row_of[pc] * width;
                    let from = &cache.clist.table[row..row + width];
                    let next_pc = pc + 1;
                    // `Match` never follows a class at once: the match's
                    // end is saved first.
                    if matches!(prog.insts[next_pc], Inst::Set(_)) {
                        // The thread waits at the next class at once: no
                        // closure to follow, its visit a step.
                        budget.charge(1)?;
                        if cache.nlist.set.insert(next_pc * depths) {
                            let to = prog.row_of[next_pc] * width;
                            cache.nlist.table[to..to + width].copy_from_slice(from);
                        }
                    } else {
                        cache.curr.copy_from_slice(from);
                        closure(prog, hay, next_pos, flags, budget, cache, next_pc, false)?;
                    }
                }
                Inst::Match => {
                    budget.charge(copy_cost(width))?;
                    let row = prog.row_of[pc] * width;
                    cache
                        .found
                        .copy_from_slice(&cache.clist.table[row..row + width]);
                    matched = true;
                    if width == 0 {
                        return Ok(true);
                    }
                    // Every later thread has lower priority.
                    break;
                }
                // Visited on the way; not a thread.
                _ => {}
            }
        }
        if next.is_none() {
            break;
        }
        pos = next_pos;
        std::mem::swap(&mut cache.clist, &mut cache.nlist);
        cache.nlist.set.clear();
    }
    Ok(matched)
}

/// Pushes a frame on the closure's stack, its growth held first.
fn push(stack: &mut Vec<Frame>, budget: &mut Budget, frame: Frame) -> Result<(), Error> {
    reserve(stack, 1, budget)?;
    stack.push(frame);
    Ok(())
}

/// Follows the empty-width instructions from `pc` at `pos` with the slots
/// in `cache.curr`, adding a thread for each consuming instruction and
/// `Match` reached, in priority order, to the current list (`into_clist`)
/// or the next one. The closure starts with no iteration begun at `pos`
/// (`s = 0`): a new thread, or one that has just consumed a character.
#[expect(
    clippy::too_many_arguments,
    reason = "the closure's state is passed apart from the cache it borrows"
)]
fn closure(
    prog: &Program,
    hay: &str,
    pos: usize,
    flags: &Flags,
    budget: &mut Budget,
    cache: &mut Cache,
    pc: usize,
    into_clist: bool,
) -> Result<(), Error> {
    let bytes = hay.as_bytes();
    let (width, depths) = (cache.width, cache.depths);
    let Cache {
        clist,
        nlist,
        stack,
        curr,
        ..
    } = cache;
    let list = if into_clist { clist } else { nlist };
    push(stack, budget, Frame::Explore(pc, 0))?;
    while let Some(frame) = stack.pop() {
        let (pc, s) = match frame {
            Frame::Explore(pc, s) => (pc, s),
            Frame::Restore(k, v) => {
                curr[k] = v;
                continue;
            }
        };
        budget.charge(1)?;
        let consuming = matches!(prog.insts[pc], Inst::Set(_) | Inst::Match);
        let key = pc * depths + if consuming { 0 } else { s };
        if !list.set.insert(key) {
            continue;
        }
        match prog.insts[pc] {
            Inst::Set(_) | Inst::Match => {
                budget.charge(copy_cost(width))?;
                let row = prog.row_of[pc] * width;
                list.table[row..row + width].copy_from_slice(curr);
            }
            Inst::Split(a, b) => {
                push(stack, budget, Frame::Explore(b, s))?;
                push(stack, budget, Frame::Explore(a, s))?;
            }
            Inst::Jmp(t) => push(stack, budget, Frame::Explore(t, s))?,
            Inst::Save(k) => {
                if k < width {
                    budget.charge(1)?;
                    push(stack, budget, Frame::Restore(k, curr[k]))?;
                    curr[k] = pos;
                }
                push(stack, budget, Frame::Explore(pc + 1, s))?;
            }
            Inst::Bol => {
                if crate::backtrack::at_line_start(bytes, pos, flags.m) {
                    push(stack, budget, Frame::Explore(pc + 1, s))?;
                }
            }
            Inst::Eol => {
                if crate::backtrack::at_line_end(bytes, pos, flags.m) {
                    push(stack, budget, Frame::Explore(pc + 1, s))?;
                }
            }
            // An iteration begins here: the innermost repetition's flag is
            // set. The closure's invariant: no more iterations have begun
            // at this position than repetitions enclose this one.
            Inst::ProgressStart(r) => {
                debug_assert!(s <= r, "{s} iterations begun here, {r} enclosing");
                push(stack, budget, Frame::Explore(pc + 1, s + 1))?;
            }
            // The repetition's own iteration ends; it is the innermost live
            // one, so its flag is the last. Begun here, it matched the empty
            // string, which ends the repetition and its register with it.
            // Otherwise every flag is false, and the repetition goes on.
            Inst::ProgressCheck(_, exit) => {
                if s > 0 {
                    push(stack, budget, Frame::Explore(exit, s - 1))?;
                } else {
                    push(stack, budget, Frame::Explore(pc + 1, 0))?;
                }
            }
            Inst::ClearGroups(lo, hi) => {
                let (a, b) = ((2 * lo).min(width), (2 * hi + 2).min(width));
                // A restore frame for each slot cleared: a step each.
                budget.charge((b - a) as u64)?;
                for (k, slot) in curr.iter_mut().enumerate().take(b).skip(a) {
                    push(stack, budget, Frame::Restore(k, *slot))?;
                    *slot = NONE;
                }
                push(stack, budget, Frame::Explore(pc + 1, s))?;
            }
            Inst::Backref(_) => {
                unreachable!("a program with back-references runs on the backtracker")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
