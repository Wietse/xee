// Compile cost measured as the number of bytes allocated on the compiling
// thread, at size n, 2n and 4n. Linear work doubles the increase; copying
// what was built so far at every step quadruples it, and copying it twice
// grows it exponentially. The count does not depend on machine load or
// build profile. Bytes, not allocations: a copied `Vec` is one allocation
// whose size grows.
//
// The counting allocator (`counting/mod.rs`) is this test binary's global
// allocator, and the count is per thread, so tests running in parallel do
// not disturb it.

mod counting;

use counting::{bytes, Measure};
use xee_xpath::Queries;

// Compiles `make(n)`, `make(2n)` and `make(4n)` on a thread with a large
// stack, after one compile that pays for whatever is initialized on first
// use, and fails unless the bytes allocated grow at most linearly.
fn assert_linear(what: &'static str, n: usize, make: fn(usize) -> String) {
    assert_linear_in(Measure::Allocated, what, n, make)
}

fn assert_linear_in(measure: Measure, what: &'static str, n: usize, make: fn(usize) -> String) {
    let counts = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            assert!(Queries::default().sequence(&make(1)).is_ok());
            [n, 2 * n, 4 * n].map(|size| {
                bytes(measure, || {
                    let compiled = Queries::default().sequence(&make(size));
                    assert!(compiled.is_ok(), "{what} at size {size} failed to compile");
                })
            })
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        counts[0] < counts[1] && counts[1] < counts[2],
        "{what}: {measure:?} bytes do not grow with the size: {counts:?}"
    );
    let ratio = (counts[2] - counts[1]) as f64 / (counts[1] - counts[0]) as f64;
    eprintln!("{what}: ratio {ratio:.2}");
    assert!(
        ratio < 3.0,
        "{what}: {measure:?} bytes grew {ratio:.2} times as fast when the size doubled"
    );
}

fn items(n: usize) -> String {
    (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
}

#[test]
fn chained_lookups() {
    assert_linear("chained lookups", 8, |n| {
        format!("let $m := map {{}} return $m{}", "?a".repeat(n))
    });
}

#[test]
fn square_array_members() {
    assert_linear("square array members", 500, |n| format!("[{}]", items(n)));
}

#[test]
fn map_entries() {
    assert_linear("map entries", 500, |n| {
        let entries = (0..n)
            .map(|i| format!("{i}: {i}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("map {{ {entries} }}")
    });
}

#[test]
fn dynamic_call_arguments() {
    assert_linear("dynamic call arguments", 50, |n| {
        let params = (0..n)
            .map(|i| format!("$a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("(function({params}) {{ 1 }})({})", items(n))
    });
}

#[test]
fn sequence_items() {
    assert_linear("sequence items", 500, |n| format!("({})", items(n)));
}

#[test]
fn nested_lets() {
    assert_linear("nested lets", 50, |n| {
        let lets = (0..n)
            .map(|i| format!("let $v{i} := {i} return "))
            .collect::<String>();
        format!("{lets}$v0")
    });
}

#[test]
fn nested_ifs() {
    assert_linear("nested ifs", 50, |n| {
        format!("{}1{}", "if (true()) then ".repeat(n), " else 2".repeat(n))
    });
}

// Lowering a map constructor copied every entry's key and value first, and
// held each enclosing map's copy while its entries were lowered: memory
// quadratic in the nesting, 25 GB for a 64 KB expression. The bytes
// allocated in all stay quadratic here, as `Bindings::concat` copies the
// inner bindings at every level, but they are freed as it goes.
#[test]
fn nested_map_constructors() {
    assert_linear_in(Measure::Peak, "nested map constructors", 50, |n| {
        format!("{}1{}", "map { 1: ".repeat(n), " }".repeat(n))
    });
}

// The same for maps nested in the keys: each key was copied too.
#[test]
fn nested_map_constructor_keys() {
    assert_linear_in(Measure::Peak, "nested map constructor keys", 50, |n| {
        format!("{}1{}", "map { ".repeat(n), ": 1 }".repeat(n))
    });
}
