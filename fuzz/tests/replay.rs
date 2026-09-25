//! Replays the fuzz checks on stable: every JSONTestSuite parsing file, and
//! hand-written seeds for each listed divergence. Also pins the serde_json
//! behaviour the divergence list relies on, and shows that an entry refuses
//! a divergence whose cause it did not measure.

use std::fs;
use std::path::PathBuf;

use serde::de::IgnoredAny;
use serde_json::Value;
use xee_json::{BomPolicy, Options, DEFAULT_MAX_DEPTH};
use xee_json_fuzz::{
    check_number, check_options, differential, differential_str, flatten_value, judge_accept,
    judge_rejection, judge_value, options_from_selector, parse, robustness, serde_number_text,
    try_differential, try_lower_depth, Report, DEPTH_LIMIT, DIVERGENCES, DUPLICATE_KEYS,
    LEADING_BOM, NUMBER_TEXT, NUMBER_TOKEN_KEY, RAISED_MAX_DEPTH, SERDE_VALUE_MAX_DEPTH,
    VALUE_LONE_SURROGATE, VALUE_RECURSION_LIMIT,
};

/// JSONTestSuite as vendored with the QT3 tests: (file name, bytes), sorted.
fn json_test_suite() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vendor/xpath-tests/misc/JSONTestSuite/test_parsing");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| {
            let path = entry.expect("directory entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("file name")
                .to_string();
            (name, fs::read(&path).expect("read"))
        })
        .collect();
    files.sort();
    assert_eq!(files.len(), 318, "JSONTestSuite test_parsing file count");
    files
}

fn nested(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

/// The fuzz target's own entry point over every file.
#[test]
fn json_test_suite_robustness() {
    for (_, data) in json_test_suite() {
        robustness(&data);
    }
}

/// Every file under every option combination the selector byte can pick
/// (both BOM policies, depths 0 to 8, the default and the raised depth),
/// whole.
#[test]
fn json_test_suite_robustness_every_option() {
    let files = json_test_suite();
    for selector in 0..22u8 {
        let options = options_from_selector(selector);
        for (_, data) in &files {
            if let Ok(text) = std::str::from_utf8(data) {
                check_options(text, options);
            }
        }
    }
}

/// The selector covers both policies and every depth.
#[test]
fn selector_covers_the_options() {
    let picked: Vec<_> = (0..22u8).map(options_from_selector).collect();
    for policy in [BomPolicy::Reject, BomPolicy::Ignore] {
        for depth in (0..=8).chain([DEFAULT_MAX_DEPTH, RAISED_MAX_DEPTH]) {
            let wanted = Options::new().with_bom_policy(policy).with_max_depth(depth);
            assert!(picked.contains(&wanted), "{wanted:?} never picked");
        }
    }
}

/// The fuzz target's own entry point over every file, and which entries
/// the suite exercises: its `i_` files reach the Value recursion limit,
/// lone surrogates and the leading BOM, its `y_` numbers reach the number
/// normalisation and its `y_` objects the duplicate merge; every `y_` file
/// with a container runs the lower-depth leg.
#[test]
fn json_test_suite_differential() {
    let mut fired = std::collections::BTreeSet::new();
    let mut compared = 0;
    let mut lowered = 0;
    for (name, data) in json_test_suite() {
        differential(&data);
        if let Ok(text) = std::str::from_utf8(&data) {
            let report = differential_str(text);
            if name.starts_with("y_") {
                assert!(
                    report.structure_compared > 0
                        || report.fired.contains(VALUE_RECURSION_LIMIT.name),
                    "{name}: no Value leg ran: {report:?}"
                );
                let container = text.contains(['[', '{']);
                let leg = try_lower_depth(text).expect(&name);
                assert_eq!(leg.is_some(), container, "{name}: lower-depth leg {leg:?}");
                lowered += usize::from(leg.is_some());
            }
            compared += report.structure_compared;
            fired.extend(report.fired);
        }
    }
    for entry in [
        &VALUE_RECURSION_LIMIT,
        &VALUE_LONE_SURROGATE,
        &DUPLICATE_KEYS,
        &NUMBER_TEXT,
        &LEADING_BOM,
    ] {
        assert!(fired.contains(entry.name), "{} never fired", entry.name);
    }
    assert!(compared >= 80, "only {compared} structures compared");
    assert!(lowered >= 80, "only {lowered} lower-depth legs");
}

/// The entries a run applied, in name order.
fn fired(report: &Report) -> Vec<&'static str> {
    report.fired.iter().copied().collect()
}

#[test]
fn every_entry_has_a_reason() {
    let mut names: Vec<_> = DIVERGENCES.iter().map(|d| d.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), DIVERGENCES.len());
    for entry in DIVERGENCES {
        assert!(!entry.reason.is_empty() && !entry.explained_by.is_empty());
    }
}

#[test]
fn seed_agreement_fires_nothing() {
    let report = differential_str(r#"{"a": [1, "x\u00e9", true, null], "b": {}}"#);
    assert_eq!(fired(&report), Vec::<&str>::new());
    assert_eq!(report.structure_compared, 1);
    for text in [
        "",
        " ",
        "[1,]",
        "{\"a\" 1}",
        "01",
        "\"\t\"",
        "[1] x",
        "\"\\x\"",
        // Closing brackets that do not match, and the last control
        // character.
        "[}",
        "[1}",
        "{]",
        "{\"a\":1]",
        "\"\u{1f}\"",
    ] {
        assert_eq!(differential_str(text), Report::default(), "{text:?}");
    }
}

/// The lower-depth leg: one level below the text's own depth, xee-json
/// fails with DepthExceeded and DEPTH_LIMIT explains it.
#[test]
fn seed_lower_depth() {
    assert_eq!(try_lower_depth("[[1], {\"a\": 2}]"), Ok(Some(1)));
    assert_eq!(try_lower_depth("[[1], {\"a\": []}]"), Ok(Some(2)));
    assert_eq!(try_lower_depth("{}"), Ok(Some(0)));
    // No container, or rejected: the leg does not apply.
    for text in ["1", "\"x\"", "[1,]", "", "\u{FEFF}[1]"] {
        assert_eq!(try_lower_depth(text), Ok(None), "{text:?}");
    }
    assert_eq!(
        try_lower_depth(&nested(SERDE_VALUE_MAX_DEPTH + 1)),
        Ok(Some(SERDE_VALUE_MAX_DEPTH))
    );
}

#[test]
fn seed_depth_limit() {
    let text = nested(DEFAULT_MAX_DEPTH + 1);
    assert!(serde_json::from_str::<IgnoredAny>(&text).is_ok());
    let report = differential_str(&text);
    assert_eq!(
        fired(&report),
        [VALUE_RECURSION_LIMIT.name, DEPTH_LIMIT.name]
    );
    // At the limit, only the Value leg differs.
    let report = differential_str(&nested(DEFAULT_MAX_DEPTH));
    assert_eq!(fired(&report), [VALUE_RECURSION_LIMIT.name]);
}

/// serde_json's IgnoredAny path has no depth limit: a deep valid text is
/// accepted.
#[test]
fn serde_ignored_any_is_unbounded() {
    assert!(serde_json::from_str::<IgnoredAny>(&nested(100_000)).is_ok());
}

/// The boundary SERDE_VALUE_MAX_DEPTH names: 127 open containers accepted
/// by Value, 128 rejected with the message the harness matches.
#[test]
fn serde_value_recursion_limit_is_pinned() {
    assert!(serde_json::from_str::<Value>(&nested(SERDE_VALUE_MAX_DEPTH)).is_ok());
    let error = serde_json::from_str::<Value>(&nested(SERDE_VALUE_MAX_DEPTH + 1))
        .expect_err("one past the limit");
    assert!(
        error.to_string().starts_with("recursion limit exceeded"),
        "{error}"
    );
    let report = differential_str(&nested(SERDE_VALUE_MAX_DEPTH));
    assert_eq!(fired(&report), Vec::<&str>::new());
    assert_eq!(report.structure_compared, 1);
    let report = differential_str(&nested(SERDE_VALUE_MAX_DEPTH + 1));
    assert_eq!(fired(&report), [VALUE_RECURSION_LIMIT.name]);
}

#[test]
fn seed_lone_surrogates() {
    for text in [
        r#"["\uD800"]"#,
        r#"["\uDC00"]"#,
        r#"["\uD800x"]"#,
        r#"["\uD800\n"]"#,
        r#"["\uD800\u0041"]"#,
        r#"["\uDC00\uD800"]"#,
        r#"{"\uD800": 1}"#,
    ] {
        assert!(serde_json::from_str::<IgnoredAny>(text).is_ok(), "{text}");
        assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
        assert_eq!(
            fired(&differential_str(text)),
            [VALUE_LONE_SURROGATE.name],
            "{text}"
        );
    }
    // A pair is not lone: both sides decode it.
    let report = differential_str(r#"["\uD834\uDD1E"]"#);
    assert_eq!(fired(&report), Vec::<&str>::new());
    assert_eq!(report.structure_compared, 1);
}

#[test]
fn seed_leading_bom() {
    // serde_json rejects a leading U+FEFF, as xee-json's default does.
    assert!(serde_json::from_str::<IgnoredAny>("\u{FEFF}[1]").is_err());
    let report = differential_str("\u{FEFF}[1]");
    assert_eq!(fired(&report), [LEADING_BOM.name]);
    assert_eq!(report.structure_compared, 1);
    // Under Ignore, a BOM with nothing after it is an error, and so is the
    // empty remainder for serde_json.
    for text in ["\u{FEFF}", "\u{FEFF} ", "\u{FEFF}\u{FEFF}1", "\u{FEFF}[1,]"] {
        let report = differential_str(text);
        assert_eq!(fired(&report), [LEADING_BOM.name], "{text:?}");
        assert_eq!(report.structure_compared, 0, "{text:?}");
    }
}

/// What arbitrary_precision does to number text, pinned against serde_json
/// itself and against serde_number_text.
#[test]
fn serde_number_text_is_pinned() {
    for (source, stored) in [
        ("0", "0"),
        ("-0", "0"),
        ("-0.0", "-0.0"),
        ("1E5", "1e+5"),
        ("1e5", "1e+5"),
        ("1e+5", "1e+5"),
        ("1.5E-3", "1.5e-3"),
        ("-1.0e0", "-1.0e+0"),
        ("1e400", "1e+400"),
        ("18446744073709551615", "18446744073709551615"),
        ("18446744073709551616", "18446744073709551616"),
        ("-9223372036854775808", "-9223372036854775808"),
        ("-9223372036854775809", "-9223372036854775809"),
        (
            "123456789012345678901234567890",
            "123456789012345678901234567890",
        ),
    ] {
        let value: Value = serde_json::from_str(source).expect(source);
        assert_eq!(value.to_string(), stored, "serde_json on {source}");
        assert_eq!(
            serde_number_text(source),
            stored,
            "serde_number_text({source})"
        );
    }
    assert_eq!(fired(&differential_str("[1E5, -0]")), [NUMBER_TEXT.name]);
    assert_eq!(
        fired(&differential_str("[1e+5, 0, -1.5]")),
        Vec::<&str>::new()
    );
}

#[test]
fn seed_duplicate_keys_are_merged() {
    for text in [
        r#"{"a": 1, "a": 2}"#,
        r#"{"a": 1, "\u0061": 2}"#,
        r#"[{"x": {"a": 1, "b": 2, "a": 3}}, "y", 1E5]"#,
        r#"{"a": [1, {"b": 2}], "c": 3, "a": {"d": [4]}, "a": null}"#,
        r#"{"a": {"a": 1, "a": 2}, "b": 0, "a": [{"c": 1, "c": {"d": 2}}]}"#,
    ] {
        let report = differential_str(text);
        assert!(report.fired.contains(DUPLICATE_KEYS.name), "{text}");
        assert_eq!(report.structure_compared, 1, "{text}");
    }
    // The same key in different objects is not a duplicate.
    let report = differential_str(r#"[{"a": 1}, {"a": 2, "b": {"a": 3}}]"#);
    assert_eq!(fired(&report), Vec::<&str>::new());
    assert_eq!(report.structure_compared, 1);
}

/// The merge is serde_json's: the first member's position, the last
/// member's value. Keeping the first value, or moving the key to its last
/// position, is a divergence.
#[test]
fn duplicate_merge_is_first_position_last_value() {
    let events = parse(r#"{"a": 1, "b": 2, "a": 3}"#, Options::default()).expect("valid");
    let value = |text| serde_json::from_str::<Value>(text).expect(text);
    let mut report = Report::default();
    assert!(judge_value(&events, Ok(value(r#"{"a": 3, "b": 2}"#)), &mut report).is_ok());
    for wrong in [r#"{"a": 1, "b": 2}"#, r#"{"b": 2, "a": 3}"#] {
        let verdict = judge_value(&events, Ok(value(wrong)), &mut report);
        assert!(verdict.is_err(), "{wrong}");
    }
}

/// serde_json's `Value`, with arbitrary_precision, reads an object whose
/// first key is its private number token as a number; RFC 8259 §4 makes
/// it an ordinary object. Pinned against serde_json, then the harness.
#[test]
fn seed_number_token_key() {
    let token = "$serde_json::private::Number";
    let as_number = serde_json::from_str::<Value>(&format!(r#"{{"{token}": "1"}}"#));
    assert!(
        matches!(as_number, Ok(Value::Number(_))),
        "serde_json no longer reads the token key as a number: {as_number:?}"
    );
    for text in [
        format!(r#"{{"{token}": "1"}}"#),
        format!(r#"{{"{token}": "abc"}}"#),
        format!(r#"{{"{token}": 5}}"#),
        format!(r#"[{{"{token}": "1", "b": 2}}]"#),
        format!(r#"[{{"{token}": "1.5"}}]"#),
        // The same key written with an escape decodes to the token.
        r#"{"$serde_json::private::Number": "1"}"#.to_string(),
    ] {
        assert!(serde_json::from_str::<IgnoredAny>(&text).is_ok(), "{text}");
        let report = differential_str(&text);
        assert_eq!(fired(&report), [NUMBER_TOKEN_KEY.name], "{text}");
        assert_eq!(report.structure_compared, 0, "{text}");
    }
    // Only the first key is classified: later, the token is a plain key.
    let report = differential_str(&format!(r#"{{"a": 1, "{token}": "1"}}"#));
    assert_eq!(fired(&report), Vec::<&str>::new());
    assert_eq!(report.structure_compared, 1);
}

// -- The entries refuse divergences whose cause they did not measure. -----

#[test]
fn depth_entry_refuses_a_text_that_is_invalid_anyway() {
    // Too deep and unclosed: raising the limit still rejects it.
    let text = format!("{}{}", "[".repeat(600), "]".repeat(599));
    let xee = parse(&text, Options::default());
    let mut report = Report::default();
    let verdict = judge_accept(&text, Options::default(), xee, Ok(()), &mut report);
    assert!(verdict.is_err(), "{verdict:?}");
    assert!(report.fired.is_empty());
}

#[test]
fn accept_leg_refuses_other_disagreements() {
    let mut report = Report::default();
    // xee-json rejects, serde_json (supposedly) accepts.
    let xee = parse("[01]", Options::default());
    assert!(judge_accept("[01]", Options::default(), xee, Ok(()), &mut report).is_err());
    // xee-json accepts, serde_json (supposedly) rejects.
    let xee = parse("[1]", Options::default());
    let serde = Err("invented".to_string());
    assert!(judge_accept("[1]", Options::default(), xee, serde, &mut report).is_err());
    assert!(report.fired.is_empty());
}

/// serde_json witnesses where xee-json rejects: every real rejection
/// passes, including a bad hex digit inside serde_json's four-byte window,
/// and an error placed one character early does not.
#[test]
fn rejection_witness_checks_the_offset() {
    for text in [
        "\"\\x\"",
        "\"\\u1G\"",
        "\"\\u123G\"",
        "\"\\\\\\u1G\"",
        "[}",
        "[1,]",
        "{\"a\" 1}",
        "trux",
        "01",
        "-a",
        "1 x",
        "\"\u{1}\"",
        "\u{FEFF}1",
        "[\u{A0}]",
    ] {
        let error = parse(text, Options::default()).expect_err(text);
        assert_eq!(judge_rejection(text, error), Ok(()), "{text:?}");
    }
    // An error at byte 1 (from "[}") claims that "\"\\" cannot be completed;
    // serde_json reads it as unfinished.
    let early = parse("[}", Options::default()).expect_err("invalid");
    assert!(judge_rejection("\"\\x\"", early).is_err());
    // An escaped backslash does not start a hex escape, so the text below
    // is a valid string; an InvalidEscape at its `G` (byte 5, taken from a
    // real hex escape) gets no exemption.
    let bad_hex = parse("\"\\u12G", Options::default()).expect_err("invalid");
    assert_eq!(bad_hex.offset(), 5);
    assert!(judge_rejection("\"\\\\u1G\"", bad_hex).is_err());
}

/// The number check sees a sign or digit dropped from either end.
#[test]
fn number_check_needs_the_whole_number() {
    let text = "[-0, 12]";
    check_number(text, &text[1..3]);
    check_number(text, &text[5..7]);
    for (start, end) in [(2, 3), (5, 6), (6, 7)] {
        let slice = &text[start..end];
        let caught = std::panic::catch_unwind(|| check_number(text, slice));
        assert!(caught.is_err(), "{slice:?} at {start} passed");
    }
    let elsewhere = String::from("-0");
    assert!(std::panic::catch_unwind(|| check_number(text, &elsewhere)).is_err());
}

#[test]
fn value_entries_need_their_precondition() {
    let events = |text| parse(text, Options::default()).expect(text);
    let mut report = Report::default();
    // A recursion-limit error against shallow events.
    let deep_error = serde_json::from_str::<Value>(&nested(SERDE_VALUE_MAX_DEPTH + 1));
    assert!(judge_value(&events("[[1]]"), deep_error, &mut report).is_err());
    // A lone-surrogate error against events without one.
    let lone_error = serde_json::from_str::<Value>(r#"["\uD800"]"#);
    assert!(judge_value(&events(r#"["\u0041"]"#), lone_error, &mut report).is_err());
    assert!(report.fired.is_empty());
}

/// If serde_json's Value ever accepts a text deeper than its pinned limit,
/// or one with a lone surrogate, the harness reports the stale pin rather
/// than comparing.
#[test]
fn value_leg_refuses_a_stale_pin() {
    let deep = nested(200);
    let events = parse(&deep, Options::default()).expect("valid");
    let mut value = Value::Array(Vec::new());
    for _ in 1..200 {
        value = Value::Array(vec![value]);
    }
    assert_eq!(flatten_value(&value).len(), 400);
    let mut report = Report::default();
    let verdict = judge_value(&events, Ok(value), &mut report);
    assert!(verdict.is_err_and(|e| e.contains("stale")), "deep");
    let events = parse(r#"["\uD800"]"#, Options::default()).expect("valid");
    let verdict = judge_value(&events, Ok(Value::Array(vec![Value::Null])), &mut report);
    assert!(
        verdict.is_err_and(|e| e.contains("stale")),
        "lone surrogate"
    );
    assert!(report.fired.is_empty());
}

#[test]
fn value_leg_compares_order_strings_and_numbers() {
    let events = |text| parse(text, Options::default()).expect(text);
    let value = |text| serde_json::from_str::<Value>(text).expect(text);
    let mut report = Report::default();
    // Value's own == ignores member order; the flat comparison does not.
    assert_eq!(value(r#"{"a":1,"b":2}"#), value(r#"{"b":2,"a":1}"#));
    for (ours, theirs) in [
        (r#"{"a":1,"b":2}"#, r#"{"b":2,"a":1}"#),
        (r#"["x"]"#, r#"["y"]"#),
        (r#"["\u00e9"]"#, r#"["e"]"#),
        ("[1.0]", "[1]"),
        ("[[]]", "[{}]"),
        ("[1,2]", "[1]"),
    ] {
        let verdict = judge_value(&events(ours), Ok(value(theirs)), &mut report);
        assert!(verdict.is_err(), "{ours} against {theirs}");
    }
    assert!(judge_value(&events(r#"["\u00e9"]"#), Ok(value("[\"é\"]")), &mut report).is_ok());
}

#[test]
fn flatten_value_walks_in_order() {
    let value: Value = serde_json::from_str(r#"{"b": [1, {"c": null}], "a": true}"#).unwrap();
    assert_eq!(
        format!("{:?}", flatten_value(&value)),
        r#"[StartObject, Key("b"), StartArray, Number("1"), StartObject, Key("c"), Null, EndObject, EndArray, Key("a"), Bool(true), EndObject]"#
    );
}

#[test]
fn unexplained_divergence_reports_both_sides() {
    // No real input diverges without an entry, so this goes through the
    // accept leg directly; try_differential itself agrees here.
    assert!(try_differential("[1]").is_ok());
    let xee = parse("[1]", Options::default());
    let serde = Err("expected value".to_string());
    let problem = judge_accept(
        "[1]",
        Options::default(),
        xee,
        serde,
        &mut Report::default(),
    )
    .expect_err("a divergence");
    assert!(problem.contains("xee-json: accepted") && problem.contains("expected value"));
}

// -- Robustness seeds. -----------------------------------------------------

#[test]
fn robustness_seeds() {
    let seeds: &[&[u8]] = &[
        b"",
        b"\x00",
        b"\x01[[[1]]]",
        b"\x11[[[[[[[[[[1]]]]]]]]]]",
        "\u{1}\u{FEFF}{\"a\":1}".as_bytes(),
        "\u{0}\u{FEFF}{\"a\":1}".as_bytes(),
        "\u{FEFF}".as_bytes(),
        b"\x03[\"\\uD800\\uDC00\\uDC00\"]",
        b"\x05{\"a\\u00e9\":[\"\\ud834\\udd1e\"],\"b\":-0.5e+7}",
        b"\xff[1]",
        b"\x02\xff",
        "\u{2}[\"é\u{10000}\"]".as_bytes(),
        // Selector 19: Ignore, the default depth; errors after a skipped BOM.
        "\u{13}\u{FEFF}[1,]".as_bytes(),
        "\u{13}\u{FEFF}".as_bytes(),
        "\u{13}\u{FEFF}\u{FEFF}1".as_bytes(),
        "\u{13}\u{FEFF}\"\\q\"".as_bytes(),
        b"\x00-0",
        b"\x00[-0, -0.0, 0, 1e5]",
        b"\x00{\"a\":-0}",
    ];
    for seed in seeds {
        robustness(seed);
        differential(seed);
    }
}

/// Past the default depth: the raised limit reads what the default rejects,
/// and stops at its own limit.
#[test]
fn robustness_past_the_default_depth() {
    for depth in [
        DEFAULT_MAX_DEPTH + 1,
        RAISED_MAX_DEPTH,
        RAISED_MAX_DEPTH + 1,
    ] {
        for policy in [BomPolicy::Reject, BomPolicy::Ignore] {
            let options = Options::new()
                .with_bom_policy(policy)
                .with_max_depth(RAISED_MAX_DEPTH);
            check_options(&nested(depth), options);
            check_options(&format!("\u{FEFF}{}", nested(depth)), options);
        }
    }
}

/// Every depth limit 0 to 8 on nested arrays and objects of depth 0 to 9.
#[test]
fn robustness_depths() {
    for depth in 0..10 {
        let arrays = nested(depth);
        let objects = format!("{}1{}", "{\"a\":".repeat(depth), "}".repeat(depth));
        for text in [&arrays, &objects] {
            for limit in (0..=8).chain([DEFAULT_MAX_DEPTH, RAISED_MAX_DEPTH]) {
                for policy in [BomPolicy::Reject, BomPolicy::Ignore] {
                    let options = Options::new().with_bom_policy(policy).with_max_depth(limit);
                    check_options(text, options);
                }
            }
        }
    }
}
