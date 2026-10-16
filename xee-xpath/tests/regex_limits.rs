// The regular-expression functions (F&O 3.1 §5.6) under the engine's
// limits, through the XPath API with the production limits: a pattern over
// one of the parse ceilings, a call that needs the program of a pattern
// over one of the compile ceilings, and a call that would take more than
// its budget of steps or hold more memory than its ceiling, are each
// refused with XPDY0130, an implementation limit. Each refusal has a near miss that
// answers, so that the test pins the cause it names.

use xee_xpath::context::Variables;
use xee_xpath::error::ErrorValue;
use xee_xpath::{Item, Sequence};

mod common;

use common::run_with_variables;

fn variable(name: &str, value: &str) -> (xot::xmlname::OwnedName, Sequence) {
    let item: Item = value.into();
    (
        xot::xmlname::OwnedName::new(name.to_string(), "".to_string(), "".to_string()),
        item.into(),
    )
}

/// `expr` with `$s` and `$p` bound to `s` and `p`.
fn eval(expr: &str, s: &str, p: &str) -> Result<Sequence, ErrorValue> {
    let variables = Variables::from([variable("s", s), variable("p", p)]);
    run_with_variables(expr, variables).map_err(|e| e.error)
}

fn answer(expr: &str, s: &str, p: &str) -> bool {
    match eval(expr, s, p) {
        Ok(result) => result.effective_boolean_value().unwrap(),
        Err(e) => panic!("{expr}: {e:?}"),
    }
}

fn refused(expr: &str, s: &str, p: &str) {
    match eval(expr, s, p) {
        Err(e) => assert_eq!(e, ErrorValue::XPDY0130, "{expr}"),
        Ok(result) => panic!("{expr}: answered {result:?}"),
    }
}

const MATCHES: &str = "matches($s, $p)";

#[test]
fn a_pattern_over_a_compile_ceiling_is_an_implementation_limit() {
    // Its length, 1 MiB: under flag `x` the whitespace is dropped.
    let spaces = |n: usize| format!("a{}", " ".repeat(n - 1));
    refused("matches($s, $p, 'x')", "a", &spaces((1 << 20) + 1));
    assert!(answer("matches($s, $p, 'x')", "a", &spaces(1 << 20)));
    // Its nesting, 100 groups.
    let nested = |n: usize| format!("{}a{}", "(".repeat(n), ")".repeat(n));
    refused(MATCHES, "a", &nested(101));
    assert!(answer(MATCHES, "a", &nested(100)));
    // Its parsed nodes, 2^18: a branch and its atoms.
    refused(MATCHES, "a", &"a".repeat(1 << 18));
    assert!(!answer(MATCHES, "a", &"a".repeat((1 << 18) - 1)));
    // The ranges of its character sets, 2^22: `\w` is about 830.
    let words = "a".repeat(1_200);
    refused(MATCHES, &words, &"\\w".repeat(6_000));
    assert!(answer(MATCHES, &words, &"\\w".repeat(1_200)));
}

#[test]
fn a_program_over_the_instruction_ceiling_is_refused_on_a_long_enough_input() {
    // `a{2000000}` is over 2^20 instructions: an input shorter than its
    // minimum length does not match, a longer one is refused.
    refused(MATCHES, &"a".repeat(2_000_000), "a{2000000}");
    assert!(!answer(MATCHES, &"a".repeat(1_999_999), "a{2000000}"));
    // QT3 cbcl-matches-038.
    assert!(!answer(MATCHES, "aaa", "a{2147483647}"));
}

// Each function's call answers at the cost of its budget, 50,000,000
// steps, and is refused a step or two past it. The costs are `xee-regex`'s
// charges (its `Budget`), on shapes whose cost grows by one or two steps
// with the input: a change of those charges moves these inputs.

#[test]
fn fn_matches_has_a_budget_of_50_000_000_steps() {
    // The prefilter scans the input for the `b` every match holds, a step
    // a character: 50,000,000 `a`s cost the whole budget.
    let s = "a".repeat(50_000_000);
    assert!(!answer(MATCHES, &s, "b"));
    refused(MATCHES, &format!("{s}a"), "b");
}

#[test]
fn fn_replace_has_a_budget_of_50_000_000_steps() {
    // A step for each byte of the replacement string, read once, and for
    // each byte of the result, and 55 for the rest of the call: a
    // replacement of 24,999,972 bytes costs 49,999,999 steps, one more
    // byte 50,000,001.
    let r = "x".repeat(24_999_972);
    assert!(answer(
        "string-length(replace('a', 'a', $p)) = 24999972",
        "",
        &r
    ));
    refused("replace('a', 'a', $p)", "", &format!("{r}x"));
}

#[test]
fn fn_tokenize_has_a_budget_of_50_000_000_steps() {
    // A step for each character the prefilter scans for the `b`, and one
    // for each byte of the one token, and one more: 24,999,999 `a`s cost
    // 49,999,999 steps, one more `a` 50,000,001.
    let s = "a".repeat(24_999_999);
    assert!(answer("count(tokenize($s, $p)) = 1", &s, "b"));
    refused("tokenize($s, $p)", &format!("{s}a"), "b");
}

#[test]
fn fn_analyze_string_has_a_budget_of_50_000_000_steps() {
    // As for `fn:tokenize`, the one non-match in place of the one token.
    let s = "a".repeat(24_999_999);
    assert!(answer(
        "count(analyze-string($s, $p)/fn:non-match) = 1",
        &s,
        "b"
    ));
    refused("analyze-string($s, $p)", &format!("{s}a"), "b");
}

#[test]
fn each_call_has_a_budget_of_its_own() {
    // Each call costs 20,000,000 to 40,000,055 steps, under its budget; the
    // calls of each function together cost more than one budget.
    let s = "a".repeat(20_000_000);
    let expr = "not(matches($s, $p)) and not(matches($s, $p)) and not(matches($s, $p)) \
        and replace('a', 'a', $s) = replace('a', 'a', $s) \
        and count(tokenize($s, $p)) + count(tokenize($s, $p)) = 2 \
        and count(analyze-string($s, $p)/fn:non-match) \
          + count(analyze-string($s, $p)/fn:non-match) = 2";
    assert!(answer(expr, &s, "b"));
}

#[test]
fn a_call_over_its_budget_is_an_implementation_limit() {
    // With a back-reference the pattern runs on the backtracker, which
    // takes time exponential in the `a`s here: 22 `a`s and a `b` cost
    // 53,827,029 steps, 21 cost 26,913,572.
    let p = "^(a+)+\\1$";
    refused(MATCHES, &format!("{}b", "a".repeat(22)), p);
    assert!(!answer(MATCHES, &format!("{}b", "a".repeat(21)), p));
    // One budget for the whole call, not one per search: each search for
    // `.*b` reads to the end of the input before `a` matches.
    let replace = "replace($s, $p, 'x') = $s";
    refused(replace, &"a".repeat(3_000), ".*b|a");
    assert!(!answer(replace, &"a".repeat(1_000), ".*b|a"));
}

#[test]
fn a_call_over_its_memory_ceiling_is_an_implementation_limit() {
    // `fn:analyze-string` tracks every group of each thread the PikeVM
    // keeps, so with 100 groups each unit of the count `n` in `.{0,n}` adds
    // about 3.3 KB to what the call holds (2^27 bytes over some 40,600
    // units). A count that answers beside the next one, refused, pins the
    // ceiling of 2^27 bytes at the call site to within about 3.3 KB; the
    // steps, some 17 M, are far from the budget. `fn:matches` tracks no
    // group.
    let groups = |n: usize| format!("{}.{{0,{n}}}", "(a)".repeat(100));
    let s = "a".repeat(1_000);
    let analyze = "exists(analyze-string($s, $p)/fn:match)";
    assert!(answer(analyze, &s, &groups(40_614)));
    refused(analyze, &s, &groups(40_615));
    assert!(answer(MATCHES, &s, &groups(100_000)));
    // `fn:replace` tracks every group when its replacement names one, and
    // none when it does not.
    let replace = "replace($s, $p, '$1') = 'a'";
    assert!(answer(replace, &s, &groups(40_618)));
    refused(replace, &s, &groups(40_619));
    assert!(answer("replace($s, $p, 'x') = 'x'", &s, &groups(100_000)));
}

#[test]
fn a_nested_repetition_answers_in_linear_time() {
    // Exponential on a backtracker; on the PikeVM, linear and under the
    // budget.
    let s = format!("{}b", "a".repeat(100_000));
    assert!(!answer(MATCHES, &s, "^(a+)+$"));
    let result = common::run("matches(string-join((1 to 100000)!'a') || 'b', '^(a+)+$')");
    assert!(!result.unwrap().effective_boolean_value().unwrap());
}

#[test]
fn the_regular_expression_errors_are_raised() {
    for (expr, code) in [
        ("matches('a', 'a', 'z')", ErrorValue::FORX0001),
        ("matches('a', '(')", ErrorValue::FORX0002),
        ("replace('a', 'a*', 'b')", ErrorValue::FORX0003),
        ("tokenize('a', 'a?')", ErrorValue::FORX0003),
        ("analyze-string('a', '')", ErrorValue::FORX0003),
        ("replace('a', 'a', '$')", ErrorValue::FORX0004),
    ] {
        assert_eq!(common::run(expr).map_err(|e| e.error), Err(code), "{expr}");
    }
}

// The casts and `fn:serialize`'s normalization-form test a value against a
// fixed pattern, anchored and linear, under a budget that grows with the
// value, at more than twice what the test can cost (`xee-interpreter`'s
// `fixed_pattern_steps`): each answers whatever the value's length, as a
// function that reads a string does. The values here are longer than a
// call's budget of 50,000,000 steps could test, each of the dearest shape
// a byte: 13 steps a character for the name types, and 16.25 a byte for
// `xs:language`'s subtags of seven characters.

#[test]
fn a_cast_to_a_name_type_answers_whatever_the_value_s_length() {
    // 4,000,000 characters cost about 52,000,000 steps, and the invalid
    // value is invalid at its last character, so that its test reads it
    // all.
    let s = "a".repeat(4_000_000);
    let invalid = format!("{s},");
    assert!(answer(
        "string-length($s cast as xs:NMTOKEN) = 4000000",
        &s,
        ""
    ));
    assert!(!answer("$s castable as xs:NMTOKEN", &invalid, ""));
    assert_eq!(
        eval("$s cast as xs:NMTOKEN", ",", "").err(),
        Some(ErrorValue::FORG0001)
    );
    // `castable as` tests the value as the cast does.
    for target in ["xs:Name", "xs:NCName", "xs:ID"] {
        assert!(
            answer(&format!("$s castable as {target}"), &s, ""),
            "{target}"
        );
    }
}

#[test]
fn a_cast_to_xs_language_answers_whatever_the_value_s_length() {
    // 500,000 seven-letter subtags, 4,000,001 bytes, cost about
    // 65,000,000 steps.
    let s = format!("a{}", "-aaaaaaa".repeat(500_000));
    let invalid = format!("{s}-");
    assert!(answer(
        "string-length($s cast as xs:language) = 4000001",
        &s,
        ""
    ));
    assert!(!answer("$s castable as xs:language", &invalid, ""));
}

#[test]
fn the_normalization_form_test_answers_whatever_the_value_s_length() {
    // The value is tested against xs:NMTOKEN's pattern as it stands. A
    // valid value that names no normalization form the JSON method
    // supports is SESU0011 (Serialization 3.1 §9.1.9), after the test; an
    // invalid one is SEPM0016.
    let serialize = "serialize((), map { 'method': 'json', 'normalization-form': $s })";
    let s = "a".repeat(4_000_000);
    assert_eq!(eval(serialize, &s, "").err(), Some(ErrorValue::SESU0011));
    let invalid = format!("{s} ");
    assert_eq!(
        eval(serialize, &invalid, "").err(),
        Some(ErrorValue::SEPM0016)
    );
}
