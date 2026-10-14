//! QT3's Perl-derived regular-expression cases (`fn-matches-50`, in
//! `vendor/xpath-tests/fn/matches/perl-tests.xml`), as far as compiling
//! a pattern decides them. The suite's composite test expects `fn:matches`
//! to answer a case whose result is `y` or `n`, and to raise an error for
//! any other; `matches` raises no error once its pattern and flags are
//! accepted, so a case of the first kind must compile and one of the
//! second must not.

use xee_regex::Regex;
use xot::Xot;

/// Cases the composite test expects to fail under XSD 1.1, whose grammar
/// changed what they test (the suite's own exceptions).
const XSD_1_0_CASES: [&str; 4] = ["p888", "p889", "p890", "p891"];

#[test]
fn the_perl_cases_compile_as_the_suite_expects() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../vendor/xpath-tests/fn/matches/perl-tests.xml"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut xot = Xot::new();
    let root = xot.parse(&text).unwrap();
    let [test, id, regex, flags, result] =
        ["test", "id", "regex", "flags", "result"].map(|n| xot.add_name(n));
    let mut cases = 0;
    let mut wrong = Vec::new();
    for node in xot.descendants(root) {
        if xot.element(node).is_none_or(|e| e.name() != test) {
            continue;
        }
        cases += 1;
        let attr = |name| xot.get_attribute(node, name).unwrap_or("");
        let answers = matches!(attr(result), "y" | "n");
        let compiled = Regex::new(attr(regex), attr(flags));
        if answers != compiled.is_ok() && !XSD_1_0_CASES.contains(&attr(id)) {
            wrong.push(format!(
                "{}: {:?} flags {:?} expects {:?}, got {:?}",
                attr(id),
                attr(regex),
                attr(flags),
                attr(result),
                compiled.map(|_| ())
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
