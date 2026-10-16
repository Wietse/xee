use std::cell::RefCell;
use std::rc::Rc;

use ahash::HashMap;
use ahash::HashMapExt;
use arrayvec::ArrayVec;
use xot::Xot;

use crate::error;
use crate::function;
use crate::sequence;
use crate::stack;

const FRAMES_MAX: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct Frame {
    function: function::InlineFunctionId,
    base: usize,
    pub(crate) ip: usize,
}

impl Frame {
    pub(crate) fn function(&self) -> function::InlineFunctionId {
        self.function
    }
    pub(crate) fn base(&self) -> usize {
        self.base
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RegexKey {
    pattern: String,
    flags: String,
}

/// Ceiling: the bytes the compiled patterns of one evaluation's cache hold.
/// A pattern can be any string an expression computes, so nothing else
/// bounds how many distinct ones an evaluation compiles. Past the ceiling
/// a pattern is compiled for its call and not kept: it costs the time of
/// compiling it again at its next call and changes no answer, and nothing
/// is evicted. 2^26 bytes are 64 MiB.
const MAX_REGEX_CACHE_BYTES: usize = 1 << 26;

/// The reference-counted allocation a compiled pattern lives in: two counts
/// and the `Regex` value, at what the allocator takes for it.
const REGEX_RC_BYTES: usize =
    xee_regex::chunk(2 * size_of::<usize>() + size_of::<xee_regex::Regex>());

/// A bucket of the map's table: an entry's key and `Rc`, and its control
/// byte.
const REGEX_BUCKET_BYTES: usize = size_of::<(RegexKey, Rc<xee_regex::Regex>)>() + 1;

/// The share of the map's table each entry is counted for. The map (std's
/// `HashMap`, a SwissTable) doubles its buckets when it is 7/8 full, so
/// once it has more than eight it has fewer than 16/7 for each entry, and
/// a share of 16/7 buckets covers the table and its rounding.
const REGEX_TABLE_BYTES_PER_ENTRY: usize = (REGEX_BUCKET_BYTES * 16).div_ceil(7);

/// What the table is counted for with the first entry, beyond the
/// entries' shares: eight buckets and the group of control bytes that
/// follows them, which covers the table up to seven entries.
const REGEX_TABLE_FIRST_BYTES: usize = xee_regex::chunk(8 * REGEX_BUCKET_BYTES + 16);

/// The compiled patterns of one evaluation, by pattern and flags, kept
/// while the bytes they hold stay under a ceiling.
#[derive(Debug)]
struct RegexCache {
    entries: HashMap<RegexKey, Rc<xee_regex::Regex>>,
    /// What the entries hold, at what the allocator takes for each
    /// allocation (`xee_regex::chunk`): each compiled pattern's heap
    /// (`Regex::heap_bytes`), its key's pattern and flags, the `Rc` it
    /// lives in ([`REGEX_RC_BYTES`]), and the map's table
    /// ([`REGEX_TABLE_BYTES_PER_ENTRY`], [`REGEX_TABLE_FIRST_BYTES`]). The
    /// key counts: under flag `x` a long pattern of whitespace compiles to
    /// a small program, and its text would otherwise be held uncounted.
    held: usize,
    ceiling: usize,
}

impl RegexCache {
    fn new() -> Self {
        Self::with_ceiling(MAX_REGEX_CACHE_BYTES)
    }

    /// A cache that keeps at most `ceiling` bytes, for tests of the ceiling
    /// at sizes a test can reach.
    fn with_ceiling(ceiling: usize) -> Self {
        Self {
            entries: HashMap::new(),
            held: 0,
            ceiling,
        }
    }

    /// The compiled `pattern`, from the cache or compiled now, and then
    /// kept if it fits under the ceiling. A pattern that does not compile
    /// is not kept, so its error is raised again at each call.
    fn get(&mut self, pattern: &str, flags: &str) -> error::Result<Rc<xee_regex::Regex>> {
        // TODO: would be nice if we could not do to_string here but use &str
        // but unfortunately otherwise lifetime issues bubble up all the way to
        // the library bindings if we do so
        let key = RegexKey {
            pattern: pattern.to_string(),
            flags: flags.to_string(),
        };
        if let Some(regex) = self.entries.get(&key) {
            return Ok(regex.clone());
        }
        let regex = Rc::new(xee_regex::Regex::new(pattern, flags)?);
        let mut bytes = regex
            .heap_bytes()
            .saturating_add(xee_regex::chunk(key.pattern.capacity()))
            .saturating_add(xee_regex::chunk(key.flags.capacity()))
            .saturating_add(REGEX_RC_BYTES)
            .saturating_add(REGEX_TABLE_BYTES_PER_ENTRY);
        if self.entries.is_empty() {
            bytes = bytes.saturating_add(REGEX_TABLE_FIRST_BYTES);
        }
        let held = self.held.saturating_add(bytes);
        if held <= self.ceiling {
            self.held = held;
            self.entries.insert(key, regex.clone());
        }
        Ok(regex)
    }
}

#[derive(Debug)]
pub struct State<'a> {
    stack: Vec<stack::Value>,
    build_stack: Vec<BuildStackEntry>,
    frames: ArrayVec<Frame, FRAMES_MAX>,
    regex_cache: RefCell<RegexCache>,
    pub(crate) xot: &'a mut Xot,
}

#[derive(Debug)]
struct ItemBuildStackEntry {
    build_stack: Vec<sequence::Item>,
}

#[derive(Debug)]
struct BuildStackEntry {
    item: ItemBuildStackEntry,
}

impl BuildStackEntry {
    fn new() -> Self {
        Self {
            item: ItemBuildStackEntry {
                build_stack: Vec::new(),
            },
        }
    }

    fn push(&mut self, item: sequence::Item) {
        self.item.build_stack.push(item);
    }

    fn extend<I: Iterator<Item = sequence::Item>>(
        &mut self,
        items: impl IntoIterator<Item = sequence::Item, IntoIter = I>,
    ) {
        self.item.build_stack.extend(items);
    }
}

impl From<BuildStackEntry> for sequence::Sequence {
    fn from(build: BuildStackEntry) -> Self {
        sequence::Sequence::new(build.item.build_stack)
    }
}

impl<'a> State<'a> {
    pub(crate) fn new(xot: &'a mut Xot) -> Self {
        Self {
            stack: vec![],
            build_stack: vec![],
            frames: ArrayVec::new(),
            regex_cache: RefCell::new(RegexCache::new()),
            xot,
        }
    }

    pub(crate) fn push<T>(&mut self, sequence: T)
    where
        T: Into<sequence::Sequence>,
    {
        let sequence: sequence::Sequence = sequence.into();
        self.stack.push(sequence.into());
    }

    pub(crate) fn push_value<T>(&mut self, value: T)
    where
        T: Into<stack::Value>,
    {
        self.stack.push(value.into());
    }

    pub(crate) fn build_new(&mut self) {
        self.build_stack.push(BuildStackEntry::new());
    }

    pub(crate) fn build_push(&mut self) -> error::Result<()> {
        let value = self.pop()?;
        let build = self.build_stack.last_mut().unwrap();
        match value {
            sequence::Sequence::Empty(_) => {}
            sequence::Sequence::One(item) => build.push(item.into_item()),
            // any other sequence
            sequence => build.extend(sequence.iter()),
        }
        Ok(())
    }

    pub(crate) fn build_complete(&mut self) {
        let build = self.build_stack.pop().unwrap();
        self.stack.push(build.into());
    }

    pub(crate) fn push_var(&mut self, index: usize) {
        self.stack
            .push(self.stack[self.frame().base + index].clone());
    }

    pub(crate) fn push_closure_var(&mut self, index: usize) -> error::Result<()> {
        let function = self.function()?;
        let closure_vars = function.closure_vars();
        self.stack.push(closure_vars[index].clone());
        Ok(())
    }

    pub(crate) fn set_var(&mut self, index: usize) {
        let base = self.frame().base;
        self.stack[base + index] = self.stack.pop().unwrap();
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> error::Result<sequence::Sequence> {
        self.pop_value().try_into()
    }

    #[inline]
    pub(crate) fn pop_value(&mut self) -> stack::Value {
        self.stack.pop().unwrap()
    }

    pub(crate) fn function(&self) -> error::Result<function::Function> {
        // the function is always just below the base
        let value = &self.stack[self.frame().base - 1];
        match value {
            stack::Value::Sequence(sequence) => sequence.clone().try_into(),
            stack::Value::Absent => Err(error::Error::XPDY0002),
        }
    }

    pub(crate) fn push_start_frame(&mut self, function_id: function::InlineFunctionId) {
        self.frames.push(Frame {
            function: function_id,
            ip: 0,
            base: 0,
        });
    }

    pub(crate) fn push_frame(
        &mut self,
        function_id: function::InlineFunctionId,
        arity: usize,
    ) -> error::Result<()> {
        if self.frames.len() >= self.frames.capacity() {
            return Err(error::Error::StackOverflow);
        }
        self.frames.push(Frame {
            function: function_id,
            ip: 0,
            base: self.stack.len() - arity,
        });
        Ok(())
    }

    pub(crate) fn frame(&self) -> &Frame {
        self.frames.last().unwrap()
    }

    pub(crate) fn frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    pub(crate) fn jump(&mut self, displacement: i32) {
        self.frame_mut().ip = (self.frame().ip as i32 + displacement) as usize;
    }

    pub(crate) fn callable(&self, arity: usize) -> error::Result<function::Function> {
        let value = &self.stack[self.stack.len() - (arity + 1)];
        match value {
            stack::Value::Sequence(sequence) => sequence.clone().try_into(),
            stack::Value::Absent => Err(error::Error::XPDY0002),
        }
    }

    pub(crate) fn arguments(&self, arity: usize) -> &[stack::Value] {
        &self.stack[self.stack.len() - arity..]
    }

    pub(crate) fn truncate_arguments(&mut self, arity: usize) {
        self.stack.truncate(self.stack.len() - arity);
    }

    pub(crate) fn inline_return(&mut self, start_base: usize) -> bool {
        let return_value = self.stack.pop().unwrap();

        // truncate the stack to the base
        let base = self.frame().base;
        self.stack.truncate(base);

        // pop off the function id we just called
        // for the outer main function this is the context item
        if !self.stack.is_empty() {
            self.stack.pop();
        }

        // push back return value
        self.stack.push(return_value);

        // now pop off the frame
        self.frames.pop();

        // if the start base is the same as the base we just popped off,
        // we are done
        base == start_base
    }

    pub(crate) fn top(&self) -> error::Result<sequence::Sequence> {
        self.stack.last().unwrap().try_into()
    }

    pub fn stack(&self) -> &[stack::Value] {
        &self.stack
    }

    pub fn regex(&self, pattern: &str, flags: &str) -> error::Result<Rc<xee_regex::Regex>> {
        self.regex_cache.borrow_mut().get(pattern, flags)
    }

    pub fn xot(&self) -> &Xot {
        self.xot
    }

    pub fn xot_mut(&mut self) -> &mut Xot {
        self.xot
    }
}

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    use super::*;

    /// Counts what this thread's allocations take, at what glibc's malloc
    /// takes for each (an eight-byte header, rounded up to sixteen, and no
    /// chunk under 32 bytes), kept apart from `xee_regex::chunk`, so that
    /// a test can measure what one structure holds while other tests run on
    /// their own threads.
    struct Counting;

    thread_local! {
        static CHUNKS: Cell<isize> = const { Cell::new(0) };
    }

    fn glibc_chunk(bytes: usize) -> isize {
        if bytes == 0 {
            0
        } else {
            ((bytes + 8).div_ceil(16) * 16).max(32) as isize
        }
    }

    fn count(delta: isize) {
        let _ = CHUNKS.try_with(|c| c.set(c.get() + delta));
    }

    fn chunks() -> isize {
        CHUNKS.with(Cell::get)
    }

    // SAFETY: each method passes its arguments to the system allocator's
    // matching method unchanged, and only counts the sizes.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(layout) };
            if !p.is_null() {
                count(glibc_chunk(layout.size()));
            }
            p
        }

        unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
            unsafe { System.dealloc(p, layout) };
            count(-glibc_chunk(layout.size()));
        }

        unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let q = unsafe { System.realloc(p, layout, size) };
            if !q.is_null() {
                count(glibc_chunk(size) - glibc_chunk(layout.size()));
            }
            q
        }
    }

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    /// What a cache counts once it has kept `patterns`, in that order. The
    /// count itself is checked against the allocator by
    /// `the_cache_counts_what_its_entries_hold`; the tests of the ceiling
    /// take it as given.
    fn held(patterns: &[(&str, &str)]) -> usize {
        let mut cache = RegexCache::with_ceiling(usize::MAX);
        for (pattern, flags) in patterns {
            cache.get(pattern, flags).unwrap();
        }
        assert_eq!(cache.entries.len(), patterns.len());
        cache.held
    }

    fn matches(regex: &xee_regex::Regex, hay: &str) -> bool {
        regex
            .is_match(hay, &mut xee_regex::Budget::new(1_000_000))
            .unwrap()
    }

    #[test]
    fn the_cache_counts_what_its_entries_hold() {
        // Each kind fills a cache to its ceiling, and after each insert the
        // count covers what the entries hold, from the first entry, whose
        // table is allocated with it: small patterns, where the key, the
        // `Rc` and the map's slots are most of an entry, with and without
        // flags; character sets; and a long key under flag `x`.
        type Kind = (fn(usize) -> String, &'static str);
        let kinds: [Kind; 5] = [
            (|i| format!("{i}"), "s"),
            (|i| format!("{i}"), ""),
            (|i| format!("[a-z]{{1,8}}-{i}"), ""),
            (|i| format!("\\p{{L}}{i}"), "i"),
            (|i| format!("a{}{i}", " ".repeat(1_000)), "x"),
        ];
        let ceiling = 1 << 20;
        for (pattern, flags) in kinds {
            // Once first, so that nothing done once per process is counted.
            RegexCache::with_ceiling(usize::MAX)
                .get(&pattern(0), flags)
                .unwrap();
            let name = (pattern(0), flags);
            let before = chunks();
            let holds = || usize::try_from(chunks() - before).unwrap();
            let mut cache = RegexCache::with_ceiling(ceiling);
            let mut refused = 0;
            let mut i = 0;
            while refused < 10 {
                assert!(i < 1_000_000, "{name:?}: the cache never fills");
                let kept = cache.entries.len();
                cache.get(&pattern(i), flags).unwrap();
                if cache.entries.len() == kept {
                    refused += 1;
                }
                let (holds, held) = (holds(), cache.held);
                assert!(
                    holds <= held,
                    "{name:?}: {} entries hold {holds} bytes, {held} counted",
                    cache.entries.len()
                );
                i += 1;
            }
            let holds = holds();
            // And the count is close to what they hold: it is no bound that
            // keeps far fewer entries than the ceiling allows.
            assert!(
                cache.held - holds <= holds / 8,
                "{name:?}: {} entries hold {holds} bytes, {} counted",
                cache.entries.len(),
                cache.held
            );
            assert!(cache.held <= ceiling && cache.held > ceiling / 2);
            drop(cache);
        }
    }

    #[test]
    fn the_cache_keeps_patterns_up_to_its_ceiling() {
        let (a, b, c) = (("a+", ""), ("[b-d]x", "i"), ("(c)(c)\\2", ""));
        // A byte short of the first: nothing is kept.
        let mut cache = RegexCache::with_ceiling(held(&[a]) - 1);
        cache.get(a.0, a.1).unwrap();
        assert!(cache.entries.is_empty());
        assert_eq!(cache.held, 0);
        // The first two fill it exactly.
        let mut cache = RegexCache::with_ceiling(held(&[a, b]));
        let first = cache.get(a.0, a.1).unwrap();
        let second = cache.get(b.0, b.1).unwrap();
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(cache.held, cache.ceiling);
        // Past the ceiling: compiled for the call, not kept.
        let third = cache.get(c.0, c.1).unwrap();
        let again = cache.get(c.0, c.1).unwrap();
        assert!(!Rc::ptr_eq(&third, &again));
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(cache.held, cache.ceiling);
        // Nothing is evicted: the first two are still served from it.
        assert!(Rc::ptr_eq(&first, &cache.get(a.0, a.1).unwrap()));
        assert!(Rc::ptr_eq(&second, &cache.get(b.0, b.1).unwrap()));
        // And the answers are the same on both sides of the ceiling.
        let mut unbounded = RegexCache::with_ceiling(usize::MAX);
        for (p, hays) in [(a, ["aaa", "b"]), (b, ["CX", "ax"]), (c, ["xccc", "xcc"])] {
            let kept = unbounded.get(p.0, p.1).unwrap();
            let served = cache.get(p.0, p.1).unwrap();
            for hay in hays {
                assert_eq!(matches(&kept, hay), matches(&served, hay), "{p:?} {hay}");
            }
        }
        assert_eq!(unbounded.entries.len(), 3);
        assert!(matches(&third, "xccc"));
        assert!(!matches(&third, "xcc"));
    }

    #[test]
    fn the_cache_counts_the_key() {
        // Under flag `x` a long pattern of whitespace compiles to a small
        // program: the key's text is most of what the entry holds. With
        // room for everything but the key, it is not kept.
        let pattern = format!("a{}b", " ".repeat(10_000));
        let program = xee_regex::Regex::new(&pattern, "x").unwrap().heap_bytes();
        let mut cache = RegexCache::with_ceiling(program + 2_000);
        let regex = cache.get(&pattern, "x").unwrap();
        assert!(matches(&regex, "ab"));
        assert!(cache.entries.is_empty());
        assert_eq!(cache.held, 0);
        // With room for the key, it is kept.
        let mut cache = RegexCache::with_ceiling(held(&[(&pattern, "x")]));
        cache.get(&pattern, "x").unwrap();
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn a_pattern_that_does_not_compile_is_not_kept() {
        let mut cache = RegexCache::with_ceiling(usize::MAX);
        for (pattern, flags) in [("(", ""), ("a", "z"), (&*"(".repeat(200), "")] {
            assert!(cache.get(pattern, flags).is_err());
        }
        assert!(cache.entries.is_empty());
        assert_eq!(cache.held, 0);
    }

    #[test]
    fn the_production_ceiling() {
        // The cache an evaluation's state builds.
        let mut xot = Xot::new();
        let state = State::new(&mut xot);
        assert_eq!(state.regex_cache.borrow().ceiling, 1 << 26);
    }
}
