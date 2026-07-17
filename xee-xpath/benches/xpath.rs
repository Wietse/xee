use divan::{black_box, Bencher};

use xee_xpath::{Documents, Queries, Query};

fn main() {
    divan::main();
}

// ============================================================================
// Original benches — preserved as-is so historical comparisons stay valid.
// ============================================================================

#[divan::bench]
fn range(bencher: Bencher) {
    let queries = Queries::default();
    let mut q = queries.sequence("1 to 1000").unwrap();

    let mut documents = Documents::new();

    bencher.bench_local(move || {
        black_box(&mut q)
            .execute_build_context(&mut documents, |_build| ())
            .unwrap();
    });
}

#[divan::bench]
fn string_concat(bencher: Bencher) {
    let queries = Queries::default();
    let mut q = queries.sequence("concat('foo', 'bar')").unwrap();

    let mut documents = Documents::new();

    bencher.bench_local(move || {
        black_box(&mut q)
            .execute_build_context(&mut documents, |_build| ())
            .unwrap();
    });
}

#[divan::bench]
fn string_value(bencher: Bencher) {
    let mut documents = Documents::new();
    let handle = documents
        .add_string_without_uri("<doc>Hello world</doc>")
        .unwrap();

    let queries = Queries::default();
    let mut q = queries.sequence("/doc/string()").unwrap();

    bencher.bench_local(move || {
        black_box(&mut q).execute(&mut documents, handle).unwrap();
    });
}

#[divan::bench]
fn large_map(bencher: Bencher) {
    let mut documents = Documents::new();

    let queries = Queries::default();
    let mut q = queries
        .sequence("map:keys(map:merge(for $n in 1 to 5000 return map:entry($n, $n+1)))")
        .unwrap();

    bencher.bench_local(move || {
        black_box(&mut q)
            .execute_build_context(&mut documents, |_build| ())
            .unwrap()
    });
}

#[divan::bench]
fn element_with_attribute(bencher: Bencher) {
    let mut documents = Documents::new();
    // large document doc with 1000 p elements where even elements have the
    // attribute state='even' and odd elements have the attribute state='odd'
    let mut doc = String::from("<doc>");
    for i in 0..1000 {
        if i % 2 == 0 {
            doc.push_str(&format!("<p state='even'>{}</p>", i));
        } else {
            doc.push_str(&format!("<p state='odd'>{}</p>", i));
        }
    }
    doc.push_str("</doc>");
    let handle = documents.add_string_without_uri(&doc).unwrap();
    let queries = Queries::default();
    let mut q = queries.sequence("/doc/p[@state = 'even']").unwrap();
    bencher.bench_local(move || {
        black_box(&mut q).execute(&mut documents, handle).unwrap();
    });
}

// ============================================================================
// Helpers shared across the new bench modules.
// ============================================================================

/// `<root><p>0</p>…<p>N-1</p></root>` — a wide document of N flat children.
fn flat_doc(n: usize) -> String {
    let mut s = String::from("<root>");
    for i in 0..n {
        s.push_str(&format!("<p>{}</p>", i));
    }
    s.push_str("</root>");
    s
}

/// `<n0><n1>…<leaf>x</leaf>…</n1></n0>` — a single deep chain `depth` levels deep.
fn deep_doc(depth: usize) -> String {
    let mut open = String::new();
    let mut close = String::new();
    for i in 0..depth {
        open.push_str(&format!("<n{}>", i));
        close.insert_str(0, &format!("</n{}>", i));
    }
    format!("{}<leaf>x</leaf>{}", open, close)
}

// ============================================================================
// Compilation cost — Queries::sequence(...) only, no execution.
// ============================================================================

mod compile {
    use super::*;

    #[divan::bench]
    fn simple_path(bencher: Bencher) {
        let queries = Queries::default();
        bencher.bench_local(|| {
            black_box(queries.sequence("/a/b/c/d").unwrap());
        });
    }

    #[divan::bench]
    fn predicate(bencher: Bencher) {
        let queries = Queries::default();
        bencher.bench_local(|| {
            black_box(
                queries
                    .sequence("/root/p[@state = 'even' and position() > 10]")
                    .unwrap(),
            );
        });
    }

    #[divan::bench]
    fn flwor(bencher: Bencher) {
        let queries = Queries::default();
        bencher.bench_local(|| {
            black_box(
                queries
                    .sequence(
                        "for $i in 1 to 10, $j in 1 to 10 \
                         return (let $k := $i * $j \
                                 return $k[. mod 2 = 0])",
                    )
                    .unwrap(),
            );
        });
    }
}

// ============================================================================
// Path navigation — axes and step chains.
// ============================================================================

mod path {
    use super::*;

    #[divan::bench]
    fn deep_descendant(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&deep_doc(50)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("//leaf").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }

    #[divan::bench]
    fn ancestor_from_leaf(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&deep_doc(50)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("//leaf/ancestor::*").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }

    #[divan::bench]
    fn following_sibling(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(1000)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("/root/p[1]/following-sibling::p").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }
}

// ============================================================================
// Predicates — positional, numeric, nested.
// ============================================================================

mod predicate {
    use super::*;

    #[divan::bench]
    fn positional(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(1000)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("/root/p[500]").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }

    #[divan::bench]
    fn numeric_filter(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(1000)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("/root/p[xs:integer(.) > 500]").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }

    #[divan::bench]
    fn nested(bencher: Bencher) {
        let mut documents = Documents::new();
        let mut doc = String::from("<root>");
        for i in 0..200 {
            doc.push_str(&format!("<p><q>{}</q></p>", i));
        }
        doc.push_str("</root>");
        let handle = documents.add_string_without_uri(&doc).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("/root/p[q = '100']").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }
}

// ============================================================================
// Function-call overhead — arithmetic, string, aggregation.
// ============================================================================

mod function {
    use super::*;

    #[divan::bench]
    fn arithmetic_chain(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries
            .sequence("sum(for $i in 1 to 1000 return $i * $i + $i - 1)")
            .unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn string_ops(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries
            .sequence(
                "string-length(string-join(\
                    for $i in 1 to 500 return concat('x', xs:string($i)), '-'))",
            )
            .unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn aggregation(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries.sequence("count(1 to 10000)").unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }
}

// ============================================================================
// Sequence ops — distinct, reverse, union.
// ============================================================================

mod sequence {
    use super::*;

    #[divan::bench]
    fn distinct_values(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries
            .sequence("distinct-values(for $i in 1 to 1000 return $i mod 100)")
            .unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn reverse_large(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries.sequence("reverse(1 to 5000)").unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn node_union(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(500)).unwrap();
        let queries = Queries::default();
        let mut q = queries
            .sequence("/root/p[xs:integer(.) < 250] | /root/p[xs:integer(.) >= 250]")
            .unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }
}

// ============================================================================
// FLWOR — for/let with body cost.
// ============================================================================

mod flwor {
    use super::*;

    #[divan::bench]
    fn simple_for(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries.sequence("for $i in 1 to 1000 return $i + 1").unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn nested_for(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries
            .sequence("for $i in 1 to 50, $j in 1 to 50 return $i * $j")
            .unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }
}

// ============================================================================
// Comparisons — value vs general, with and without coercion.
// ============================================================================

mod comparison {
    use super::*;

    #[divan::bench]
    fn value_int(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries
            .sequence("(for $i in 1 to 1000 return ($i eq 500))[. = true()]")
            .unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn general_sequences(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries.sequence("(1 to 1000) = (995 to 1100)").unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }

    #[divan::bench]
    fn string_eq(bencher: Bencher) {
        let queries = Queries::default();
        let mut q = queries
            .sequence(
                "(for $i in 1 to 1000 return concat('abc', xs:string($i)))[. = 'abc500']",
            )
            .unwrap();
        let mut documents = Documents::new();
        bencher.bench_local(move || {
            black_box(&mut q)
                .execute_build_context(&mut documents, |_| ())
                .unwrap();
        });
    }
}

// ============================================================================
// Atomization — node→atomic conversion paths. Exercises the loop that turns
// nodes into their typed values, the hot path behind sum/compare over nodes.
// ============================================================================

mod atomization {
    use super::*;

    #[divan::bench]
    fn sum_text_values(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(1000)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("sum(/root/p)").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }

    #[divan::bench]
    fn compare_nodes(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(1000)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("/root/p[. = '500']").unwrap();
        bencher.bench_local(move || {
            black_box(&mut q).execute(&mut documents, handle).unwrap();
        });
    }
}

// ============================================================================
// Reuse — compile once, execute many times against a single document. The
// common pattern when one query is run repeatedly over the same instance.
// ============================================================================

mod reuse {
    use super::*;

    #[divan::bench]
    fn hundred_executions(bencher: Bencher) {
        let mut documents = Documents::new();
        let handle = documents.add_string_without_uri(&flat_doc(100)).unwrap();
        let queries = Queries::default();
        let mut q = queries.sequence("/root/p[xs:integer(.) > 50]").unwrap();
        bencher.bench_local(move || {
            for _ in 0..100 {
                black_box(&mut q).execute(&mut documents, handle).unwrap();
            }
        });
    }
}
