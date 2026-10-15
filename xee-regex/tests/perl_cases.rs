//! QT3's Perl-derived regular-expression cases (`fn-matches-50`, in
//! `vendor/xpath-tests/fn/matches/perl-tests.xml`). The suite's composite
//! test expects `fn:matches` to answer `true` for a case whose result is
//! `y`, `false` for one whose result is `n`, and to raise an error for any
//! other.

use xee_regex::{Budget, Regex};
use xot::Xot;

/// Cases the composite test expects to fail under XSD 1.1, whose grammar
/// changed what they test (the suite's own exceptions).
const XSD_1_0_CASES: [&str; 4] = ["p888", "p889", "p890", "p891"];

#[test]
fn the_perl_cases_answer_as_the_suite_expects() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../vendor/xpath-tests/fn/matches/perl-tests.xml"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut xot = Xot::new();
    let root = xot.parse(&text).unwrap();
    let [test, id, regex, flags, input, result] =
        ["test", "id", "regex", "flags", "input", "result"].map(|n| xot.add_name(n));
    let mut cases = 0;
    let mut wrong = Vec::new();
    for node in xot.descendants(root) {
        if xot.element(node).is_none_or(|e| e.name() != test) {
            continue;
        }
        cases += 1;
        let attr = |name| xot.get_attribute(node, name).unwrap_or("");
        let expected = match attr(result) {
            "y" => Some(true),
            "n" => Some(false),
            _ => None,
        };
        let got = Regex::new(attr(regex), attr(flags))
            .and_then(|r| r.is_match(attr(input), &mut Budget::new(50_000_000)));
        if expected != got.clone().ok() && !XSD_1_0_CASES.contains(&attr(id)) {
            wrong.push(format!(
                "{}: {:?} flags {:?} on {:?} expects {:?}, got {:?}",
                attr(id),
                attr(regex),
                attr(flags),
                attr(input),
                attr(result),
                got
            ));
        }
    }
    assert_eq!(cases, 1641);
    assert!(
        wrong.is_empty(),
        "{} cases:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
