//! `Regex::heap_bytes` covers what `Regex::new` and `Regex::xsd` keep:
//! after a pattern is compiled, the bytes still allocated are at most the
//! size it reports,
//! and the size is each allocation still live at what glibc's allocator
//! takes for it, exactly. One test, so that no other test allocates while
//! it measures.
//!
//! `chunk` here is the oracle for the crate's `xee_regex::chunk`, kept
//! apart from it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use xee_regex::{Error, Regex};

struct Counting;

/// The bytes asked for and not yet freed, and the same allocations at
/// what the allocator takes for them.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static CHUNKS: AtomicUsize = AtomicUsize::new(0);

/// What glibc's malloc takes for `bytes`: an eight-byte header, rounded up
/// to sixteen, and no chunk under 32 bytes.
fn chunk(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        ((bytes + 8).div_ceil(16) * 16).max(32)
    }
}

// SAFETY: each method passes its arguments to the system allocator's
// matching method unchanged, and only counts the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::SeqCst);
            CHUNKS.fetch_add(chunk(layout.size()), Ordering::SeqCst);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
        CHUNKS.fetch_sub(chunk(layout.size()), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
            LIVE.fetch_add(size, Ordering::SeqCst);
            CHUNKS.fetch_sub(chunk(layout.size()), Ordering::SeqCst);
            CHUNKS.fetch_add(chunk(size), Ordering::SeqCst);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn the_size_covers_what_a_compiled_pattern_keeps() {
    let cases: &[(String, &str)] = &[
        ("a".into(), ""),
        // Groups, and their parents.
        ("((a)(b)((c)(d)))+".into(), ""),
        // A large program: 600,000 instructions and more.
        ("\\p{L}{0,300000}x".into(), ""),
        // The sets: each class its own, some of hundreds of ranges.
        (
            "\\w\\p{L}\\p{Nd}[\\p{L}\\p{N}]\\P{IsBasicLatin}[^\\p{Lu}]\\i\\c".into(),
            "",
        ),
        // Negated sets that touch U+0000 or U+10FFFF, built with room for
        // one range more than they keep.
        ("\\P{C}[^\\p{C}]\\P{Cn}x".into(), ""),
        // Case variants under flag `i`.
        ("[a-z\u{3B1}-\u{3C9}]+k".into(), "i"),
        // Back-references, which run on the backtracker.
        ("(a)(b+)\\2\\1".into(), ""),
        // A literal pattern under flag `q`, and the anchors and `.` under
        // flags `s` and `m`.
        ("a.b*(".into(), "q"),
        ("^a.b$".into(), "sm"),
        // The prefilter: a prefix, and a set every match holds one of.
        ("abcdefghijklmnop(?:q|r)+".into(), ""),
        ("[xyz](?:a|b)".into(), ""),
        // Over the instruction ceiling: no program is kept.
        ("a{2000000}".into(), ""),
        // A long pattern under flag `x` keeps a small program.
        (format!("a{}b", " ".repeat(100_000)), "x"),
    ];
    // The XSD dialect, `None` for flags: its program between anchors, and
    // the unknown block names it lists, each its own string.
    let xsd: &[String] = &[
        "(a)|b(c)".into(),
        "^$\\p{IsBasicLatin}+".into(),
        "\\p{IsNoSuchBlock}\\P{IsOther}[\\p{IsNoSuchBlock}\\p{Is-}]x".into(),
        (0..2000)
            .map(|k| format!("\\p{{Is0x{k}}}"))
            .collect::<String>(),
        "a{2000000}".into(),
    ];
    let compile = |pattern: &str, flags: Option<&str>| -> Result<Regex, Error> {
        match flags {
            Some(flags) => Regex::new(pattern, flags),
            None => Regex::xsd(pattern),
        }
    };
    let all = cases
        .iter()
        .map(|(p, f)| (p, Some(*f)))
        .chain(xsd.iter().map(|p| (p, None)));
    for (pattern, flags) in all {
        // Once first, so that nothing compiling does once per process is
        // counted.
        drop(compile(pattern, flags).unwrap());
        let (live, chunks) = (LIVE.load(Ordering::SeqCst), CHUNKS.load(Ordering::SeqCst));
        let regex = compile(pattern, flags).unwrap();
        let kept = LIVE.load(Ordering::SeqCst) - live;
        let kept_chunks = CHUNKS.load(Ordering::SeqCst) - chunks;
        let size = regex.heap_bytes();
        let name: String = pattern.chars().take(40).collect();
        if flags.is_none() && pattern.contains("NoSuch") {
            assert_eq!(regex.unknown_blocks(), ["-", "NoSuchBlock", "Other"]);
        }
        assert!(kept <= size, "{name}: {kept} bytes kept, {size} reported");
        assert_eq!(size, kept_chunks, "{name}: {kept} bytes kept");
        drop(regex);
    }
}
