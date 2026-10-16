//! QT3's Perl-derived regular-expression cases through XPath's
//! `fn:matches`, as the composite test `fn-matches-50` runs them: for each
//! `test` element of `vendor/xpath-tests/fn/matches/perl-tests.xml`,
//! `matches(@input, @regex, string(@flags))` with the element as the
//! context item. A case whose result is `y` expects `true`, one whose
//! result is `n` expects `false`, and every other result value (`c`, `p`,
//! `sc`, `Sc`, `Sn`, `Sy`, `yB`, `yM`) expects an error.
//!
//! `fn-matches-50` depends on XSD 1.0, which xee's runner does not declare
//! (it declares XSD 1.1), so the runner skips it; this test runs it under
//! XSD 1.1 instead. The four cases the suite marks for that dependency
//! (W3C bug 30029), p888 to p891, assume XSD 1.0's hyphen rules (XSD 1.0
//! Part 2 Appendix F): under XSD 1.1 §G.4.1 a hyphen after a character
//! class escape is a character of its own, so each pattern is valid and
//! answers where the suite expects an error. They are asserted to be the
//! only cases that disagree with the suite.

use xee_xpath::error::ErrorValue;
use xee_xpath::{Documents, Item, Queries, Query};

/// The cases that assume XSD 1.0's hyphen rules (W3C bug 30029).
const XSD_1_0_CASES: [&str; 4] = ["p888", "p889", "p890", "p891"];

fn perl_tests() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../vendor/xpath-tests/fn/matches/perl-tests.xml"
    );
    std::fs::read_to_string(path).unwrap()
}

/// What a case expects: an answer, or an error.
#[derive(Debug, PartialEq)]
enum Expected {
    Answer(bool),
    Error,
}

/// F&O 3.1 §5.6.2: flags are drawn from `s`, `m`, `i`, `x` and `q`; any
/// other character makes them invalid, FORX0001.
fn flags_are_valid(flags: &str) -> bool {
    flags.chars().all(|c| "smixq".contains(c))
}

#[test]
fn the_perl_cases_answer_through_fn_matches_as_the_suite_expects() {
    let xml = perl_tests();
    let mut documents = Documents::new();
    let handle = documents.add_string_without_uri(&xml).unwrap();
    let queries = Queries::default();
    let tests = queries
        .sequence("/tests/test")
        .unwrap()
        .execute(&mut documents, handle)
        .unwrap();
    let matches = queries
        .sequence("matches(@input, @regex, string(@flags))")
        .unwrap();
    let without_flags = queries.sequence("matches(@input, @regex)").unwrap();
    let attribute = |documents: &mut Documents, node: xot::Node, name: &str| {
        let xot = documents.xot_mut();
        let name = xot.add_name(name);
        xot.get_attribute(node, name).map(str::to_string)
    };

    let mut cases = 0;
    let mut answered = 0;
    let mut invalid_flags = 0;
    let mut invalid_both = 0;
    let mut disagreeing = Vec::new();
    let mut wrong = Vec::new();
    for item in tests.iter() {
        let Item::Node(node) = item else {
            panic!("{item:?} is no element");
        };
        cases += 1;
        let id = attribute(&mut documents, node, "id").unwrap();
        let result = attribute(&mut documents, node, "result").unwrap();
        let flags = attribute(&mut documents, node, "flags").unwrap_or_default();
        let expected = match result.as_str() {
            "y" => Expected::Answer(true),
            "n" => Expected::Answer(false),
            "c" | "p" | "sc" | "Sc" | "Sn" | "Sy" | "yB" | "yM" => Expected::Error,
            other => panic!("{id}: result {other:?}"),
        };
        let got = matches
            .execute(&mut documents, &item)
            .map(|sequence| sequence.effective_boolean_value().unwrap())
            .map_err(|e| e.error);
        // fn-matches-50's rule: a case fails if it answers other than its
        // result, or raises an error where its result is `y` or `n`.
        let agrees = match (&expected, &got) {
            (Expected::Answer(want), Ok(answer)) => want == answer,
            (Expected::Error, Err(_)) => true,
            _ => false,
        };
        if !agrees {
            disagreeing.push(id.clone());
            if !XSD_1_0_CASES.contains(&id.as_str()) {
                wrong.push(format!("{id}: result {result:?}, got {got:?}"));
            }
            continue;
        }
        match got {
            Ok(_) => answered += 1,
            // The suite does not name a code; F&O 3.1 §5.6 does. A pattern
            // that does not compile is FORX0002 (§5.6.1), invalid flags
            // FORX0001 (§5.6.2); no case reaches an implementation limit.
            // Where the flags and the pattern are both invalid, F&O lists
            // both conditions and puts neither first, so either code is
            // right.
            Err(code) => {
                let want: &[ErrorValue] = if flags_are_valid(&flags) {
                    &[ErrorValue::FORX0002]
                } else {
                    invalid_flags += 1;
                    let pattern = without_flags.execute(&mut documents, &item);
                    if pattern.map_err(|e| e.error) == Err(ErrorValue::FORX0002) {
                        invalid_both += 1;
                        &[ErrorValue::FORX0001, ErrorValue::FORX0002]
                    } else {
                        &[ErrorValue::FORX0001]
                    }
                };
                if !want.contains(&code) {
                    wrong.push(format!("{id}: expects {want:?}, got {code:?}"));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} cases:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    assert_eq!(cases, 1641);
    // y 513 and n 165; p867 to p884 are commented out in the file.
    assert_eq!(answered, 678);
    // `/(?m'[ew]+)/` twice and `/(?second'[fF]o+)/` once, each beside a
    // pattern that does not compile either.
    assert_eq!(invalid_flags, 3);
    assert_eq!(invalid_both, 3);
    assert_eq!(disagreeing, XSD_1_0_CASES);
}

/// The four cases outside the suite answer as XSD 1.1 reads them: §G.4.1's
/// hyphen rules apply to a hyphen after a `singleChar`, and in `[\d-z]` and
/// `[\d-\s]` it follows a character class escape, so it is a `singleChar`
/// of its own and the class is the digits, the hyphen, and the `z` or the
/// whitespace.
#[test]
fn the_xsd_1_0_cases_follow_xsd_1_1() {
    for (expr, answer) in [
        (r"matches('a0-za', '([\d-z]+)')", true),
        (r"matches('-', '([\d-z]+)')", true),
        (r"matches('a0- z', '([\d-\s]+)')", true),
        (r"matches('-', '([\d-\s]+)')", true),
        (r"replace('a0-za', '([\d-z]+)', '[$1]')", true),
    ] {
        let queries = Queries::default();
        let mut documents = Documents::new();
        let result = queries
            .sequence(expr)
            .unwrap()
            .execute_build_context(&mut documents, |_| ())
            .unwrap();
        let got = if expr.starts_with("replace") {
            result.string_value(documents.xot()).unwrap() == "a[0-z]a"
        } else {
            result.effective_boolean_value().unwrap()
        };
        assert_eq!(got, answer, "{expr}");
    }
}

/// One evaluation over every case that expects an answer, so that the
/// cases share the cache of compiled patterns, which lasts the evaluation
/// and is keyed by pattern and flags. They hold 343 distinct patterns in
/// 424 distinct pairs of pattern and flags, some patterns under several
/// flags; a cache keyed by the pattern alone would serve a pattern
/// compiled under one case's flags to another's, and some of those would
/// answer wrongly. The counts are the data's; the test does not read the
/// cache.
#[test]
fn the_answering_cases_agree_in_one_evaluation() {
    let xml = perl_tests();
    let mut documents = Documents::new();
    let handle = documents.add_string_without_uri(&xml).unwrap();
    let queries = Queries::default();
    let expr = "let $cases := /tests/test[@result = ('y', 'n')] \
        return (count($cases), \
          count($cases[matches(@input, @regex, string(@flags)) ne (@result = 'y')]), \
          count(distinct-values($cases/@regex)), \
          count(distinct-values($cases ! concat(string-length(@flags), ':', @flags, @regex))))";
    let result = queries
        .sequence(expr)
        .unwrap()
        .execute(&mut documents, handle)
        .unwrap();
    let counts: Vec<String> = result
        .iter()
        .map(|item| item.string_value(documents.xot()).unwrap())
        .collect();
    assert_eq!(counts, ["678", "0", "343", "424"]);
}
