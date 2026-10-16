//! The regular-expression functions (F&O 3.1 §5.6) through the XPath API,
//! on synthetic inputs.
//!
//! Run with `cargo bench -p xee-xpath --bench regex`, or
//! `cargo test -p xee-xpath --bench regex` to run each bench once. Each
//! bench evaluates one compiled XPath expression with `$s` bound to its
//! input, so that its time includes building the dynamic context, compiling
//! the pattern (the cache of compiled patterns lasts one evaluation) and
//! the call itself; the expressions and inputs are defined in
//! [`regex_inputs`]:
//!
//! - `short`: one `fn:matches` on a short value, where compiling the
//!   pattern weighs as much as matching it; `repeated`: the same check
//!   1,000 times in one evaluation, the pattern compiled once;
//! - `text`: a text block of about 1 KB to about 1 MB through
//!   `fn:tokenize`, `fn:replace`, `fn:analyze-string` and `fn:matches`;
//!   the counter is the block's bytes;
//! - `shape`: patterns whose cost grows faster than the input on some
//!   matchers, at sizes a backtracking matcher answers.
//!
//! Each workload gives its answer once before it is timed
//! ([`regex_inputs::Workload::check`]), so a broken expression or a refused
//! call fails the run rather than being timed. Each timed evaluation runs
//! in an empty arena of documents of its own, made before the sample and
//! dropped after it with the result, so neither is timed and no evaluation
//! is timed against the trees earlier ones built ([`time`]). Every bench stops after 20
//! samples or 2 seconds, whichever comes first, and each input is sized so
//! that one evaluation takes well under a second.

mod regex_inputs;

use divan::counter::BytesCount;
use divan::Bencher;
use regex_inputs::*;
use xee_xpath::Documents;

fn main() {
    divan::main();
}

/// Times `prepared`, each evaluation in an arena of its own.
fn time(bencher: Bencher, prepared: &Prepared) {
    bencher
        .with_inputs(Documents::new)
        .bench_local_refs(|documents| prepared.run(documents).unwrap());
}

const SHORT: [&str; 5] = ["date", "code_list", "identifier", "prefix", "url"];

#[divan::bench(args = SHORT, max_time = 2, sample_count = 20)]
fn short(bencher: Bencher, name: &str) {
    time(bencher, &short_check(name).check());
}

#[divan::bench(args = SHORT, max_time = 2, sample_count = 20)]
fn repeated_1000(bencher: Bencher, name: &str) {
    time(bencher, &repeated(name).check());
}

/// Registers one bench over the text sizes for the text workload `name`.
macro_rules! text_bench {
    ($name:ident) => {
        #[divan::bench(args = TEXT_SIZES, max_time = 2, sample_count = 20)]
        fn $name(bencher: Bencher, size: usize) {
            let workload = text_workload(stringify!($name), size);
            let prepared = workload.check();
            time(
                bencher.counter(BytesCount::of_str(&workload.input)),
                &prepared,
            );
        }
    };
}

mod text {
    use super::*;

    text_bench!(tokenize_spaces);
    text_bench!(replace_spaces);
    text_bench!(replace_tags);
    text_bench!(replace_ing_i);
    text_bench!(replace_groups);
    text_bench!(analyze_pairs);
    text_bench!(matches_absent);
    text_bench!(matches_blank);
}

const SHAPE_NAMES: [&str; 4] = [
    "alternation_star",
    "wide_count",
    "counted_class",
    "nested_plus",
];

#[divan::bench(args = SHAPE_NAMES, max_time = 2, sample_count = 20)]
fn shape(bencher: Bencher, name: &str) {
    time(bencher, &regex_inputs::shape(name).check());
}
