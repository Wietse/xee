// Cost of an XSLT transformation whose template holds sequence constructors
// of many items, at size n, 2n and 4n: the whole of `evaluate`, which parses
// and compiles the stylesheet before it runs it. Measured as the most bytes
// held at once and as the bytes allocated in all (see
// `xee-xpath/tests/counting/mod.rs`, whose counting allocator this test
// binary shares).

#[path = "../../xee-xpath/tests/counting/mod.rs"]
mod counting;

use counting::{bytes, Measure};
use xot::Xot;

// Transforms `<doc/>` with `stylesheet(size)` at sizes n, 2n and 4n on a
// thread with a large stack, after one run that pays for whatever is
// initialized on first use, checks that the output element has `size + 1`
// children, and fails unless each of `measures` grows at most linearly.
fn assert_linear(
    measures: &'static [Measure],
    what: &'static str,
    n: usize,
    stylesheet: fn(usize) -> String,
) {
    let counts = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let run = |size: usize| {
                let mut xot = Xot::new();
                let output = xee_xslt_compiler::evaluate(&mut xot, "<doc/>", &stylesheet(size));
                let output = output.unwrap();
                let o = output.iter().next().unwrap().to_node().unwrap();
                assert_eq!(xot.children(o).count(), size + 1, "{what} at size {size}");
            };
            run(1);
            measures
                .iter()
                .map(|&measure| [n, 2 * n, 4 * n].map(|size| bytes(measure, || run(size))))
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

fn template(body: &str) -> String {
    format!(
        r#"<xsl:transform xmlns:xsl="http://www.w3.org/1999/XSL/Transform" version="3">
  <xsl:template match="/"><o>{body}</o></xsl:template>
</xsl:transform>"#
    )
}

// A sequence constructor of several items was evaluated as a chain of
// two-item concatenations, each held in a local until the end of the
// template, and each copying what came before: memory and time quadratic in
// the number of items.
#[test]
fn a_sequence_constructor_of_many_items() {
    assert_linear(
        &[Measure::Peak, Measure::Allocated],
        "a sequence constructor of many items",
        500,
        |n| template(&"<a/>".repeat(n + 1)),
    );
}

// Each `xsl:sequence` here holds an element and the next `xsl:sequence`:
// sequence constructors nested n deep, each an item of the one around it,
// evaluated as an expression of its own. Memory is linear in the nesting.
// (Bound to a local instead, each level's inner sequence was held until the
// end: quadratic.) Each level still copies its inner sequence, so only the
// most held at once is measured.
#[test]
fn nested_sequence_constructors() {
    assert_linear(&[Measure::Peak], "nested sequence constructors", 250, |n| {
        template(&format!(
            "{}<a/>{}",
            "<a/><xsl:sequence>".repeat(n),
            "</xsl:sequence>".repeat(n)
        ))
    });
}
