// Evaluation cost of a compiled query, at size n, 2n and 4n, measured two
// ways: the most bytes held at once, and the bytes allocated in all, which
// counts what is copied. Holding or copying what was built so far at every
// step makes either grow quadratically; linear work doubles the increase.
// The counts do not depend on machine load or build profile.
//
// The counting allocator (`counting/mod.rs`) is this test binary's global
// allocator, and the count is per thread, so tests running in parallel do
// not disturb it.

mod counting;

use counting::{bytes, Measure};
use xee_xpath::{Documents, Itemable, Queries, Query};

// An expression, the integer it must evaluate to, and the document that is
// its context item, if it reads one.
struct Case {
    src: String,
    expected: usize,
    document: Option<String>,
}

// Compiles `make(size)`, which returns an expression and the integer it must
// evaluate to, then evaluates it at sizes n, 2n and 4n on a thread with a
// large stack, after one evaluation that pays for whatever is initialized on
// first use. Fails on a wrong result, or unless both measures grow at most
// linearly. Only the evaluation is measured.
fn assert_linear(what: &'static str, n: usize, make: fn(usize) -> (String, usize)) {
    assert_linear_in(&[Measure::Peak, Measure::Allocated], what, n, make)
}

fn assert_linear_in(
    measures: &'static [Measure],
    what: &'static str,
    n: usize,
    make: fn(usize) -> (String, usize),
) {
    assert_linear_cases(measures, what, n, move |size| {
        let (src, expected) = make(size);
        Case {
            src,
            expected,
            document: None,
        }
    })
}

// As `assert_linear_in`, for a `Case`. Its document is parsed, and the
// query run once on it, before the evaluation is measured, so that neither
// parsing the document nor annotating its document order, which the first
// sort does, hides the evaluation's own growth.
fn assert_linear_cases(
    measures: &'static [Measure],
    what: &'static str,
    n: usize,
    make: impl Fn(usize) -> Case + Send + 'static,
) {
    let counts = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let evaluate = |size: usize, measure: Option<Measure>| {
                let Case {
                    src,
                    expected,
                    document,
                } = make(size);
                let queries = Queries::default();
                let src = format!("string({src})");
                let query = queries.one(&src, |_, item| Ok(item.clone())).unwrap();
                let mut documents = Documents::new();
                let document = document.map(|xml| documents.add_string_without_uri(&xml).unwrap());
                let has_document = document.is_some();
                let mut builder = query.dynamic_context_builder(&documents);
                if let Some(document) = document {
                    builder.context_item(document.to_item(&documents).unwrap());
                }
                let context = builder.build();
                let mut run = || {
                    let item = query.execute_with_context(&mut documents, &context);
                    let item = item.unwrap_or_else(|e| panic!("{what} at size {size}: {e:?}"));
                    let value = item.to_atomic().unwrap().to_string().unwrap();
                    assert_eq!(value, expected.to_string(), "{what} at size {size}");
                };
                if has_document {
                    run();
                }
                match measure {
                    Some(measure) => bytes(measure, run),
                    None => {
                        run();
                        0
                    }
                }
            };
            evaluate(1, None);
            measures
                .iter()
                .map(|&measure| [n, 2 * n, 4 * n].map(|size| evaluate(size, Some(measure))))
                .collect::<Vec<_>>()
        })
        .unwrap()
        .join()
        .unwrap();
    for (measure, counts) in measures.iter().zip(counts) {
        assert!(
            counts[0] < counts[1] && counts[1] < counts[2],
            "{what}: {measure:?} bytes do not grow with the size: {counts:?}"
        );
        let ratio = (counts[2] - counts[1]) as f64 / (counts[1] - counts[0]) as f64;
        eprintln!("{what}: {measure:?} ratio {ratio:.2}");
        assert!(
            ratio < 3.0,
            "{what}: {measure:?} bytes grew {ratio:.2} times as fast when the size doubled"
        );
    }
}

fn comma_separated(n: usize, item: impl Fn(usize) -> String) -> String {
    (0..n).map(item).collect::<Vec<_>>().join(", ")
}

// A comma-separated sequence was evaluated as a chain of two-item
// concatenations, each held in a local until the end of the expression:
// memory quadratic in its length (20,000 items: 7.9 GB). Each step also
// copied everything before it: time quadratic.
#[test]
fn a_sequence_of_literals() {
    assert_linear("a sequence of literals", 500, |n| {
        (
            format!("count(({}))", comma_separated(n, |i| i.to_string())),
            n,
        )
    });
}

#[test]
fn a_sequence_of_references() {
    assert_linear("a sequence of references", 500, |n| {
        let items = comma_separated(n, |_| "$a".to_string());
        (format!("let $a := 1 return count(({items}))"), n)
    });
}

#[test]
fn a_sequence_of_computed_items() {
    assert_linear("a sequence of computed items", 500, |n| {
        let items = comma_separated(n, |i| format!("{i} + 1"));
        (format!("count(({items}))"), n)
    });
}

#[test]
fn a_sequence_of_sequences() {
    assert_linear("a sequence of sequences", 250, |n| {
        let items = comma_separated(n, |i| format!("({i}, {i})"));
        (format!("count(({items}))"), 2 * n)
    });
}

// `count(body)`, which must be `expected`, with `n` distinct element nodes
// bound to `$c`.
fn count_over_nodes(n: usize, body: String, expected: usize) -> Case {
    Case {
        src: format!("let $c := /r/a return count({body})"),
        expected,
        document: Some(format!("<r>{}</r>", "<a/>".repeat(n))),
    }
}

fn chain(n: usize, operator: &str, operand: impl Fn(usize) -> String) -> String {
    (1..=n).map(operand).collect::<Vec<_>>().join(operator)
}

// The i-th node of `$c`. A filter, `$c[i]`, allocates for every node of
// `$c` it visits, which would grow quadratically on its own.
fn nth(i: usize) -> String {
    format!("subsequence($c, {i}, 1)")
}

const BOTH: [Measure; 2] = [Measure::Peak, Measure::Allocated];

// A union chain was evaluated as a left fold of two-operand unions, each
// held in a local until the end of the expression, and each hashing and
// sorting every node before it: memory and time quadratic in its length
// (10,000 distinct nodes: 10.6 s and 1.58 GB in release).
#[test]
fn a_union_of_distinct_nodes() {
    assert_linear_cases(&BOTH, "a union of distinct nodes", 500, |n| {
        count_over_nodes(n, chain(n, " | ", nth), n)
    });
}

// Every operand here is the same n nodes, so each union's result is as long
// as the operands, and the bytes allocated grow quadratically however the
// unions are grouped. The most held at once, about log2(n) results of n
// nodes waiting to be joined, grows as n log n.
#[test]
fn a_union_of_references() {
    assert_linear_cases(&[Measure::Peak], "a union of references", 250, |n| {
        count_over_nodes(n, chain(n, " | ", |_| "$c".to_string()), n)
    });
}

// An except chain was a left fold too: each step held its result until the
// end of the expression and hashed and sorted what was left of the first
// operand, so removing nodes one by one took memory and time quadratic in
// the number removed.
#[test]
fn an_except_chain() {
    assert_linear_cases(&BOTH, "an except chain", 500, |n| {
        let removed = chain(n, " except ", |i| nth(2 * i));
        count_over_nodes(2 * n, format!("$c except {removed}"), n)
    });
}

// Each intersection here is as long as its operands, as with the union of
// references above.
#[test]
fn an_intersect_chain() {
    assert_linear_cases(&[Measure::Peak], "an intersect chain", 250, |n| {
        count_over_nodes(n, chain(n, " intersect ", |_| "$c".to_string()), n)
    });
}

// Intersections with all of `$c` and removals of one node, alternating:
// folded from the left, every step's result, nearly all of `$c`, was held
// until the end of the expression.
#[test]
fn an_intersect_except_chain() {
    assert_linear_cases(&[Measure::Peak], "an intersect except chain", 250, |n| {
        let steps = chain(n, " ", |i| format!("intersect $c except {}", nth(i)));
        count_over_nodes(2 * n, format!("$c {steps}"), n)
    });
}

// A sequence nested in a sequence is an item of it, evaluated as an
// expression of its own: memory linear in the nesting. (Bound to a local
// instead, each level's inner sequence was held until the end: quadratic.)
// Each level still copies its inner sequence, so the bytes allocated grow
// quadratically; only the most held at once is measured.
#[test]
fn nested_sequences() {
    assert_linear_in(&[Measure::Peak], "nested sequences", 250, |n| {
        let mut src = String::from("0");
        for i in 1..n {
            src = format!("({i}, {src})");
        }
        (format!("count({src})"), n)
    });
}
