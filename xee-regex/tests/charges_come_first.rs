//! Each operation charges the work it does for a match, and after the last
//! one, before it does it. Refused at the charge for a large piece of
//! work, a call has allocated next to nothing: the step counts the unit
//! tests pin cannot show the order, and the allocations can. One test, so
//! that no other test allocates while it measures.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use xee_regex::{Budget, Error, Regex};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::SeqCst) + bytes;
    PEAK.fetch_max(live, Ordering::SeqCst);
}

// SAFETY: each method passes its arguments to the system allocator's
// matching method unchanged, and only counts the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
            grew(size);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The size of the large piece of work in each case.
const N: usize = 1 << 20;

/// What a refused call may have allocated at most at any one time.
const SMALL: usize = 1 << 16;

/// The most `f` held at once beyond what was live when it began.
fn peak<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let base = LIVE.load(Ordering::SeqCst);
    PEAK.store(base, Ordering::SeqCst);
    let out = f();
    (out, PEAK.load(Ordering::SeqCst) - base)
}

#[test]
fn a_call_refused_at_a_charge_has_not_done_its_work() {
    let x = "x".repeat(N);
    // Each case: the call, and the steps it charges after the large piece,
    // so that a budget of all its steps but those and one more refuses it
    // at that piece; or, for a piece charged first, its size.
    type Op = Box<dyn Fn(&mut Budget) -> Result<usize, Error>>;
    enum At {
        After(u64),
        First(u64),
    }
    let r = |p: &str| Regex::new(p, "").unwrap();
    let (a, comma) = (r("a"), r(","));
    let cases: Vec<(&str, Op, At)> = vec![
        (
            "replace, the text before a match",
            Box::new({
                let (a, hay) = (r("a"), format!("{x}a"));
                move |b| a.replace(&hay, "$0", b).map(|s| s.len())
            }),
            At::After(0),
        ),
        (
            "replace, the text after the last match",
            Box::new({
                let hay = format!("a{x}");
                move |b| a.replace(&hay, "$0", b).map(|s| s.len())
            }),
            At::After(0),
        ),
        (
            "tokenize, a token before a match",
            Box::new({
                let (comma, hay) = (r(","), format!("{x},"));
                // The empty token after the match costs a step.
                move |b| comma.tokenize(&hay, b).map(|t| t.len())
            }),
            At::After(1),
        ),
        (
            "tokenize, the token after the last match",
            Box::new({
                let hay = format!(",{x}");
                move |b| comma.tokenize(&hay, b).map(|t| t.len())
            }),
            At::After(0),
        ),
        (
            "analyze, the text before a match",
            Box::new({
                let (a, hay) = (r("a"), format!("{x}a"));
                move |b| a.analyze(&hay, b).map(|e| e.len())
            }),
            At::After(0),
        ),
        (
            "analyze, the text in a match's groups",
            Box::new({
                // One match of 2^20 characters, its group's text and the
                // back-reference's: the backtracker's stack stays small.
                let (twice, hay) = (r("(x{1024})\\1{1023}"), "x".repeat(N));
                move |b| twice.analyze(&hay, b).map(|e| e.len())
            }),
            // The group looked at and the sort of one, after the text.
            At::After(2),
        ),
        (
            "analyze, the text after the last match",
            Box::new({
                let (a, hay) = (r("a"), format!("a{x}"));
                move |b| a.analyze(&hay, b).map(|e| e.len())
            }),
            At::After(0),
        ),
        (
            "replace, the replacement string",
            Box::new({
                let (group, repl) = (r("(a)"), "$1x".repeat(N / 3));
                move |b| group.replace("b", &repl, b).map(|s| s.len())
            }),
            // Read and parsed first: its bytes.
            At::First(3 * (N / 3) as u64),
        ),
    ];
    for (name, op, at) in &cases {
        let mut whole = Budget::new(u64::MAX);
        op(&mut whole).unwrap_or_else(|e| panic!("{name}: {e}"));
        let steps = whole.spent();
        let mut short = Budget::new(match at {
            At::After(after) => steps - after - 1,
            At::First(size) => size - 1,
        });
        let (refused, held) = peak(|| op(&mut short));
        assert!(
            matches!(refused, Err(Error::Limit(_))),
            "{name}: {refused:?}"
        );
        assert!(
            held < SMALL,
            "{name}: {held} bytes allocated before the refusal"
        );
        // A control: the whole budget answers, and allocates the work.
        let mut enough = Budget::new(steps);
        let (answered, held) = peak(|| op(&mut enough));
        assert!(answered.is_ok(), "{name}");
        assert!(held >= N, "{name}: the control allocated {held} bytes");
    }
}
