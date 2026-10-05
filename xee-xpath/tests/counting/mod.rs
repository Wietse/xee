// A counting global allocator for the test binaries that measure cost by
// allocation: each binary that declares `mod counting;` gets its own. The
// counts are per thread, so tests running in parallel do not disturb them.
//
// Not every binary reads every measure.
#![allow(dead_code, reason = "each test binary uses only some of these")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    // bytes allocated in all
    static BYTES: Cell<u64> = const { Cell::new(0) };
    // bytes allocated and not yet freed, and the most of those at once
    static LIVE: Cell<i64> = const { Cell::new(0) };
    static PEAK: Cell<i64> = const { Cell::new(0) };
}

fn count(allocated: usize, freed: usize) {
    let _ = BYTES.try_with(|c| c.set(c.get() + allocated as u64));
    let _ = LIVE.try_with(|live| {
        live.set(live.get() + allocated as i64 - freed as i64);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(live.get())));
    });
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size(), 0);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(0, layout.size());
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size, layout.size());
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Clone, Copy, Debug)]
pub enum Measure {
    // the bytes allocated while `f` runs
    Allocated,
    // the most bytes held at once while `f` runs, above what was held
    // before
    Peak,
}

pub fn bytes(measure: Measure, f: impl FnOnce()) -> u64 {
    let before = BYTES.with(|c| c.get());
    let live = LIVE.with(|c| c.get());
    PEAK.with(|c| c.set(live));
    f();
    match measure {
        Measure::Allocated => BYTES.with(|c| c.get()) - before,
        Measure::Peak => (PEAK.with(|c| c.get()) - live) as u64,
    }
}
