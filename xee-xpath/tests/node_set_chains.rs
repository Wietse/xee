// A union chain is lowered to one node set of its operands, and an
// intersect and except chain to the intersection of the operands it keeps,
// except the union of those it removes, not to a fold of two-operand steps.
// These tests check what such chains evaluate to against sets computed
// here.

use std::collections::BTreeSet;

use xee_xpath::{error, Documents, Queries, Query};

// Five `a` elements numbered 1 to 5, as `$c`, and the operand subsets,
// each with the numbers of its nodes.
const NODES: &str = "let $c := parse-xml('<r><a n=\"1\"/><a n=\"2\"/><a n=\"3\"/><a n=\"4\"/><a n=\"5\"/></r>')/r/a";
const OPERANDS: [(&str, &[u32]); 5] = [
    ("$c", &[1, 2, 3, 4, 5]),
    ("$c[position() le 3]", &[1, 2, 3]),
    ("$c[position() ge 3]", &[3, 4, 5]),
    ("($c[4], $c[2], $c[4])", &[2, 4]),
    ("()", &[]),
];

fn evaluate(src: &str) -> error::Result<String> {
    let queries = Queries::default();
    let src = format!("{NODES} return {src}");
    let query = queries.one(&src, |_, item| Ok(item.clone()))?;
    let mut documents = Documents::new();
    let builder = query.dynamic_context_builder(&documents);
    let context = builder.build();
    let item = query.execute_with_context(&mut documents, &context)?;
    Ok(item.to_atomic().unwrap().to_string().unwrap())
}

// The `n` attributes of `src`'s value, space separated.
fn numbers(src: &str) -> error::Result<String> {
    evaluate(&format!("string-join(({src}) ! string(@n), ' ')"))
}

// The error `src` raises; `count` takes any items, so the error is the
// chain's own.
fn error_code(src: &str) -> error::ErrorValue {
    evaluate(&format!("string(count({src}))"))
        .expect_err(src)
        .error
}

fn joined(numbers: &BTreeSet<u32>) -> String {
    numbers
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

// Every chain of four intersect and except operators over operands drawn
// from `OPERANDS`, against the same steps taken from the left on sets.
#[test]
fn intersect_except_chains_group_from_the_left() {
    for pattern in 0..16usize {
        for start in 0..OPERANDS.len() {
            let operand = |i: usize| OPERANDS[(start + 2 * i) % OPERANDS.len()];
            let set = |i: usize| operand(i).1.iter().copied().collect::<BTreeSet<_>>();
            let mut src = operand(0).0.to_string();
            let mut expected = set(0);
            for i in 0..4 {
                let (operator, right) = if (pattern >> i) & 1 == 0 {
                    (
                        "intersect",
                        expected.intersection(&set(i + 1)).copied().collect(),
                    )
                } else {
                    (
                        "except",
                        expected.difference(&set(i + 1)).copied().collect(),
                    )
                };
                src = format!("{src} {operator} {}", operand(i + 1).0);
                expected = right;
            }
            assert_eq!(numbers(&src).unwrap(), joined(&expected), "{src}");
        }
    }
}

#[test]
fn a_union_chain_is_in_document_order_without_duplicates() {
    let src = OPERANDS
        .iter()
        .rev()
        .map(|(operand, _)| *operand)
        .collect::<Vec<_>>()
        .join(" | ");
    assert_eq!(numbers(&src).unwrap(), "1 2 3 4 5");
    // each operand but the repeated one adds a node of its own
    assert_eq!(
        numbers("$c[5] | $c[2] union $c[4] | $c[2] | $c[1]").unwrap(),
        "1 2 4 5"
    );
}

// Union binds more loosely than intersect and except, so an intersect and
// except chain is one operand of a union, first, last or between.
#[test]
fn a_union_of_intersect_except_chains() {
    assert_eq!(
        numbers("$c[1] | $c except $c[1] except $c[2] | $c[2] intersect $c[3]").unwrap(),
        "1 3 4 5"
    );
    assert_eq!(numbers("$c except $c[1] | $c[2]").unwrap(), "2 3 4 5");
    assert_eq!(numbers("$c[1] intersect $c[2] | $c[3]").unwrap(), "3");
}

// A chain parenthesized on the right is an operand of its own:
// `a except (b except c)` is not `a except b except c`.
#[test]
fn a_parenthesized_except_chain_is_an_operand() {
    assert_eq!(
        numbers("$c except ($c[1] except $c[1])").unwrap(),
        "1 2 3 4 5"
    );
    assert_eq!(numbers("$c except ($c except $c[1])").unwrap(), "1");
    assert_eq!(
        numbers("$c intersect ($c except $c[1])").unwrap(),
        "2 3 4 5"
    );
}

// An item that is not a node is a type error in any operand of any of the
// three operators (XPath 3.1 3.4.2). With 17 operands, the one that is not
// a node joins the others behind every number of waiting runs.
#[test]
fn an_operand_that_is_not_a_node_is_a_type_error() {
    for operator in ["|", "intersect", "except"] {
        for bad in 0..17 {
            let src = (0..17)
                .map(|i| if i == bad { "1" } else { "$c" })
                .collect::<Vec<_>>()
                .join(&format!(" {operator} "));
            assert_eq!(error_code(&src), error::ErrorValue::XPTY0004, "{src}");
        }
    }
    assert_eq!(
        error_code("$c intersect $c except $c intersect (1, $c)"),
        error::ErrorValue::XPTY0004
    );
}

// The operands are evaluated one by one while the runs before them wait on
// the stack, where locals live too. Every operand here has locals of its
// own and reads one from outside the chain, and with 17 operands one waits
// behind each number of runs from none to four.
#[test]
fn operands_with_their_own_locals() {
    // `$c` is 18 nodes; operand i holds node i + 1 and the last.
    let operands = (0..17)
        .map(|i| format!("(let $x{i} := {i} + 1 return ($c[$x{i}], $last))"))
        .collect::<Vec<_>>();
    let all = (1..=18).map(|i| i.to_string()).collect::<Vec<_>>();
    let nodes = (1..=18)
        .map(|i| format!("<a n=\"{i}\"/>"))
        .collect::<String>();
    let evaluate = |chain: String| {
        let src = format!(
            "let $c := parse-xml('<r>{nodes}</r>')/r/a, $last := $c[18] \
             return string-join(({chain}) ! string(@n), ' ')"
        );
        let queries = Queries::default();
        let query = queries.one(&src, |_, item| Ok(item.clone())).unwrap();
        let mut documents = Documents::new();
        let builder = query.dynamic_context_builder(&documents);
        let context = builder.build();
        let item = query
            .execute_with_context(&mut documents, &context)
            .unwrap();
        item.to_atomic().unwrap().to_string().unwrap()
    };
    assert_eq!(evaluate(operands.join(" | ")), all.join(" "));
    assert_eq!(evaluate(operands.join(" intersect ")), "18");
    let except = format!("$c except {}", operands.join(" except "));
    assert_eq!(evaluate(except), "");
    let except = format!("$c except {}", operands[..16].join(" except "));
    assert_eq!(evaluate(except), "17");
}
