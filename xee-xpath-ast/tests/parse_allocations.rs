// Parse cost measured as the number of heap allocations made on the parsing
// thread, at nesting depth n, 2n and 4n. Linear work doubles the increase;
// copying the parsed subtree at every level quadruples it. The count does not
// depend on machine load or build profile, so this catches at a depth of a
// few hundred what a wall-clock deadline in a debug build cannot reach.
//
// The counting allocator is this test binary's global allocator, and the
// count is per thread, so tests running in parallel do not disturb it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use xee_xpath_ast::{ast, Namespaces, Pattern, VariableNames};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(|c| c.get());
    f();
    ALLOCATIONS.with(|c| c.get()) - before
}

// Runs `parse` at sizes n, 2n and 4n on a thread with a large stack (debug
// frames are big), after one parse that pays for whatever is initialized on
// first use, and fails unless the allocations grew at most linearly. The
// differences cancel the parser's fixed cost: linear work gives a ratio of
// 2, quadratic work 4.
fn assert_linear(what: &'static str, parse: fn(usize) -> bool) {
    let counts = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            assert!(parse(1), "{what} at size 1 failed to parse");
            let n = 50;
            [n, 2 * n, 4 * n].map(|size| {
                allocations(|| assert!(parse(size), "{what} at size {size} failed to parse"))
            })
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        counts[0] < counts[1] && counts[1] < counts[2],
        "{what}: allocations do not grow with the size: {counts:?}"
    );
    let ratio = (counts[2] - counts[1]) as f64 / (counts[1] - counts[0]) as f64;
    eprintln!("{what}: ratio {ratio:.2}");
    assert!(
        ratio < 3.0,
        "{what}: allocations grew {ratio:.2} times as fast when the size doubled"
    );
}

fn xpath(src: &str) -> bool {
    ast::XPath::parse(src, &Namespaces::default(), &VariableNames::default()).is_ok()
}

fn nest(open: &str, inner: &str, close: &str, n: usize) -> String {
    format!("{}{}{}", open.repeat(n), inner, close.repeat(n))
}

#[test]
fn nested_parentheses() {
    assert_linear("nested parentheses", |n| xpath(&nest("(", "1", ")", n)));
}

#[test]
fn nested_function_calls() {
    assert_linear("nested calls", |n| xpath(&nest("fn:abs(", "1", ")", n)));
}

#[test]
fn nested_dynamic_calls() {
    assert_linear("nested dynamic calls", |n| {
        xpath(&nest("(fn:abs#1)(", "1", ")", n))
    });
}

#[test]
fn nested_placeholder_calls() {
    assert_linear("nested placeholder calls", |n| {
        xpath(&nest("(fn:abs#1)(", "1", ")(?)", n))
    });
}

#[test]
fn nested_simple_maps() {
    assert_linear("nested simple maps", |n| xpath(&nest("1 ! (", "1", ")", n)));
}

#[test]
fn nested_arrow_arguments() {
    assert_linear("nested arrow arguments", |n| {
        xpath(&nest("1 => fn:sum(", "1", ")", n))
    });
}

#[test]
fn arrow_chain() {
    assert_linear("arrow chain", |n| {
        xpath(&format!("1{}", " => fn:abs()".repeat(n)))
    });
}

#[test]
fn placeholder_arrow_chain() {
    assert_linear("placeholder arrow chain", |n| {
        xpath(&format!("1{}", " => fn:concat(?, 1)".repeat(n)))
    });
}

#[test]
fn nested_let_bindings() {
    assert_linear("nested let bindings", |n| {
        xpath(&nest("let $x := ", "1", " return 1", n))
    });
}

#[test]
fn nested_for_bindings() {
    assert_linear("nested for bindings", |n| {
        xpath(&nest("for $x in ", "1", " return 1", n))
    });
}

#[test]
fn nested_quantified_bindings() {
    assert_linear("nested quantified bindings", |n| {
        xpath(&nest("some $x in ", "1", " satisfies 1", n))
    });
}

// A parenthesized path that is a whole relative path is unwrapped to the
// path inside. `((a))` unwraps to `a` at every level and copies little:
// here the path inside, `(…)/b`, grows by a step at every level.
#[test]
fn nested_pattern_paths() {
    assert_linear("nested pattern paths", |n| {
        Pattern::<ast::ExprS>::parse(
            &nest("((", "a", ")/b)", n),
            &Namespaces::default(),
            &VariableNames::default(),
        )
        .is_ok()
    });
}
