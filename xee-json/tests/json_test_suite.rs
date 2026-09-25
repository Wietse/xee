//! JSONTestSuite (`test_parsing`, as vendored with the QT3 tests): every
//! `y_` file accepted, every `n_` file rejected, and every `i_`
//! (implementation-defined) file's outcome recorded as a decision below.
//!
//! The tokenizer takes `&str`, so a file that is not UTF-8 never reaches it:
//! `str::from_utf8` rejects it first. Those files are named here and counted
//! separately.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use common::{check_balanced, check_segments, lone_surrogates, parse_with, strings};
use xee_json::{BomPolicy, ErrorKind, Event, Options, Str};

const REJECT_BOM: Options = Options::new().with_bom_policy(BomPolicy::Reject);
const IGNORE_BOM: Options = Options::new().with_bom_policy(BomPolicy::Ignore);

/// The `n_` files that are not UTF-8, rejected by `str::from_utf8`.
const N_NOT_UTF8: [&str; 12] = [
    "n_array_a_invalid_utf8.json",
    "n_array_invalid_utf8.json",
    "n_number_invalid-utf-8-in-bigger-int.json",
    "n_number_invalid-utf-8-in-exponent.json",
    "n_number_invalid-utf-8-in-int.json",
    "n_number_real_with_invalid_utf8_after_e.json",
    "n_object_lone_continuation_byte_in_key_and_trailing_comma.json",
    "n_string_invalid-utf-8-in-escape.json",
    "n_string_invalid_utf8_after_escape.json",
    "n_structure_incomplete_UTF8_BOM.json",
    "n_structure_lone-invalid-utf-8.json",
    "n_structure_single_eacute.json",
];

/// The decision for an `i_` file.
#[derive(Debug, Clone, Copy)]
enum Decision {
    /// Accepted under both byte order mark policies; the strings deliver
    /// exactly these lone surrogates, in order.
    Accept(&'static [u16]),
    /// A leading byte order mark: accepted under [`BomPolicy::Ignore`],
    /// rejected under [`BomPolicy::Reject`].
    DependsOnBomPolicy,
    /// Not UTF-8, so outside the `&str` input contract: rejected by
    /// `str::from_utf8` before tokenizing.
    NotUtf8,
}

use Decision::*;

/// Every `i_` file and its decision.
///
/// - Numbers are delivered as lexical text, so range and precision are the
///   consumer's concern (RFC 8259 §9): the ten `i_number_*` files are
///   accepted.
/// - An escaped surrogate outside a pair is delivered as
///   `Decoded::LoneSurrogate` for the consumer's policy (RFC 8259 §8.2;
///   F&O 3.1 §17.5.1 passes it to `fallback`): the ten surrogate files are
///   accepted, with the lone units listed.
/// - 500 nested arrays are within the default depth limit of 512.
/// - The byte order mark follows the configured policy (RFC 8259 §8.1).
/// - Thirteen files are not UTF-8 (Latin-1, UTF-16, overlong or truncated
///   sequences, an encoded surrogate): the tokenizer takes `&str`, and
///   decoding bytes is the caller's job.
const I_DECISIONS: [(&str, Decision); 35] = [
    ("i_number_double_huge_neg_exp.json", Accept(&[])),
    ("i_number_huge_exp.json", Accept(&[])),
    ("i_number_neg_int_huge_exp.json", Accept(&[])),
    ("i_number_pos_double_huge_exp.json", Accept(&[])),
    ("i_number_real_neg_overflow.json", Accept(&[])),
    ("i_number_real_pos_overflow.json", Accept(&[])),
    ("i_number_real_underflow.json", Accept(&[])),
    ("i_number_too_big_neg_int.json", Accept(&[])),
    ("i_number_too_big_pos_int.json", Accept(&[])),
    ("i_number_very_big_negative_int.json", Accept(&[])),
    ("i_object_key_lone_2nd_surrogate.json", Accept(&[0xDFAA])),
    (
        "i_string_1st_surrogate_but_2nd_missing.json",
        Accept(&[0xDADA]),
    ),
    (
        "i_string_1st_valid_surrogate_2nd_invalid.json",
        Accept(&[0xD888]),
    ),
    (
        "i_string_incomplete_surrogate_and_escape_valid.json",
        Accept(&[0xD800]),
    ),
    ("i_string_incomplete_surrogate_pair.json", Accept(&[0xDD1E])),
    (
        "i_string_incomplete_surrogates_escape_valid.json",
        Accept(&[0xD800, 0xD800]),
    ),
    ("i_string_invalid_lonely_surrogate.json", Accept(&[0xD800])),
    ("i_string_invalid_surrogate.json", Accept(&[0xD800])),
    (
        "i_string_inverted_surrogates_U+1D11E.json",
        Accept(&[0xDD1E, 0xD834]),
    ),
    ("i_string_lone_second_surrogate.json", Accept(&[0xDFAA])),
    ("i_structure_500_nested_arrays.json", Accept(&[])),
    (
        "i_structure_UTF-8_BOM_empty_object.json",
        DependsOnBomPolicy,
    ),
    ("i_string_invalid_utf-8.json", NotUtf8),
    ("i_string_iso_latin_1.json", NotUtf8),
    ("i_string_lone_utf8_continuation_byte.json", NotUtf8),
    ("i_string_not_in_unicode_range.json", NotUtf8),
    ("i_string_overlong_sequence_2_bytes.json", NotUtf8),
    ("i_string_overlong_sequence_6_bytes.json", NotUtf8),
    ("i_string_overlong_sequence_6_bytes_null.json", NotUtf8),
    ("i_string_truncated-utf-8.json", NotUtf8),
    ("i_string_utf16BE_no_BOM.json", NotUtf8),
    ("i_string_utf16LE_no_BOM.json", NotUtf8),
    ("i_string_UTF-16LE_with_BOM.json", NotUtf8),
    ("i_string_UTF-8_invalid_sequence.json", NotUtf8),
    ("i_string_UTF8_surrogate_U+D800.json", NotUtf8),
];

struct Suite {
    files: Vec<(String, Vec<u8>)>,
}

impl Suite {
    fn load() -> Suite {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/xpath-tests/misc/JSONTestSuite/test_parsing");
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let entry = entry.expect("directory entry");
            let name = entry.file_name().into_string().expect("UTF-8 file name");
            if name.ends_with(".json") {
                let bytes = std::fs::read(entry.path()).expect("readable file");
                files.push((name, bytes));
            }
        }
        files.sort();
        let count = |prefix: &str| files.iter().filter(|(n, _)| n.starts_with(prefix)).count();
        // A missing or partial directory must not pass vacuously.
        assert_eq!(
            (count("y_"), count("n_"), count("i_"), files.len()),
            (95, 188, 35, 318),
            "JSONTestSuite file counts in {}",
            dir.display()
        );
        Suite { files }
    }

    fn with_prefix<'s>(
        &'s self,
        prefix: &'s str,
    ) -> impl Iterator<Item = (&'s str, &'s [u8])> + 's {
        self.files
            .iter()
            .filter(move |(n, _)| n.starts_with(prefix))
            .map(|(n, b)| (n.as_str(), b.as_slice()))
    }
}

/// Parses a file that must be accepted and checks its events and strings.
fn accept_file<'t>(name: &str, text: &'t str, options: Options) -> Vec<Event<'t>> {
    let events = parse_with(text, options).unwrap_or_else(|e| panic!("{name} rejected: {e}"));
    check_balanced(&events).unwrap_or_else(|e| panic!("{name}: {e}"));
    for s in strings(&events) {
        check_segments(&s);
    }
    events
}

#[test]
fn json_test_suite_y_files_are_accepted() {
    let suite = Suite::load();
    let mut accepted = 0;
    let mut escaped_strings = 0;
    for (name, bytes) in suite.with_prefix("y_") {
        let text =
            std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{name} is not UTF-8: {e}"));
        let events = accept_file(name, text, REJECT_BOM);
        assert_eq!(accept_file(name, text, IGNORE_BOM), events, "{name}");
        escaped_strings += strings(&events)
            .filter(|s| matches!(s, Str::Escaped(_)))
            .count();
        accepted += 1;
    }
    assert_eq!(accepted, 95);
    // The segment invariants were checked on real escapes.
    assert!(escaped_strings > 20, "{escaped_strings} escaped strings");
}

#[test]
fn json_test_suite_n_files_are_rejected() {
    let suite = Suite::load();
    let mut by_tokenizer = 0;
    let mut not_utf8 = BTreeSet::new();
    for (name, bytes) in suite.with_prefix("n_") {
        let Ok(text) = std::str::from_utf8(bytes) else {
            not_utf8.insert(name);
            continue;
        };
        for options in [REJECT_BOM, IGNORE_BOM] {
            if let Ok(events) = parse_with(text, options) {
                panic!("{name} accepted under {options:?}: {events:?}");
            }
        }
        by_tokenizer += 1;
    }
    assert_eq!(not_utf8, BTreeSet::from(N_NOT_UTF8));
    assert_eq!((by_tokenizer, not_utf8.len()), (176, 12));
}

#[test]
fn json_test_suite_i_files_follow_the_recorded_decisions() {
    let suite = Suite::load();
    let files: BTreeSet<&str> = suite.with_prefix("i_").map(|(n, _)| n).collect();
    let table: BTreeSet<&str> = I_DECISIONS.iter().map(|(n, _)| *n).collect();
    assert_eq!(table.len(), I_DECISIONS.len(), "duplicate table entries");
    assert_eq!(
        files.difference(&table).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "i_ files without a decision"
    );
    assert_eq!(
        table.difference(&files).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "decisions without an i_ file"
    );

    let mut counts = [0usize; 3];
    for (name, bytes) in suite.with_prefix("i_") {
        let decision = I_DECISIONS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| *d)
            .expect("decision");
        let utf8 = std::str::from_utf8(bytes);
        match decision {
            Accept(expected_lone) => {
                counts[0] += 1;
                let text = utf8.unwrap_or_else(|e| panic!("{name}: {e}"));
                let events = accept_file(name, text, REJECT_BOM);
                assert_eq!(accept_file(name, text, IGNORE_BOM), events, "{name}");
                let lone: Vec<u16> = strings(&events).flat_map(|s| lone_surrogates(&s)).collect();
                assert_eq!(lone, expected_lone, "{name}");
                if name.starts_with("i_number_") {
                    // The number's text is delivered exactly as written.
                    let number = text.trim().trim_start_matches('[').trim_end_matches(']');
                    assert_eq!(
                        events,
                        [Event::StartArray, Event::Number(number), Event::EndArray],
                        "{name}"
                    );
                }
            }
            DependsOnBomPolicy => {
                counts[1] += 1;
                let text = utf8.unwrap_or_else(|e| panic!("{name}: {e}"));
                accept_file(name, text, IGNORE_BOM);
                let error = parse_with(text, REJECT_BOM).expect_err(name);
                assert_eq!(
                    (error.kind(), error.offset()),
                    (ErrorKind::BomNotAllowed, 0)
                );
            }
            NotUtf8 => {
                counts[2] += 1;
                assert!(utf8.is_err(), "{name} is UTF-8");
            }
        }
    }
    assert_eq!(counts, [21, 1, 13]);
}
