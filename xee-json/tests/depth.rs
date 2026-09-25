//! The nesting limit: at the limit, one past it, for arrays, objects and
//! mixed nesting; and nesting far deeper than a recursive parser survives.

mod common;

use std::path::PathBuf;

use common::{check_balanced, items_with, parse_with};
use xee_json::{ErrorKind, Event, Options, Tokenizer, DEFAULT_MAX_DEPTH};

/// `depth` nested arrays: `[[…[]…]]`.
fn arrays(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

/// `depth` nested objects: `{"a":{"a":…{}…}}`. The `n`th `{` (from 1) is at
/// byte `5 * (n - 1)`.
fn objects(depth: usize) -> String {
    format!(
        "{}{{}}{}",
        "{\"a\":".repeat(depth - 1),
        "}".repeat(depth - 1)
    )
}

/// `depth` containers alternating array and object, with a value after
/// each nested one; also the byte offset of every opening bracket.
fn mixed(depth: usize) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut openers = Vec::new();
    let mut closers = Vec::new();
    for level in 0..depth {
        openers.push(text.len());
        if level % 2 == 0 {
            text.push_str("[1,");
            closers.push("]");
        } else {
            text.push_str("{\"k\":");
            closers.push(",\"z\":null}");
        }
    }
    text.push('0');
    // Close the innermost first; an array's trailing element is the `0` or
    // the nested container.
    for closer in closers.iter().rev() {
        text.push_str(closer);
    }
    (text, openers)
}

fn depth_error(input: &str, options: Options) -> (ErrorKind, usize) {
    let error = parse_with(input, options).expect_err("rejected");
    (error.kind(), error.offset())
}

fn max_nesting(events: &[Event<'_>]) -> usize {
    let mut depth = 0usize;
    let mut max = 0;
    for event in events {
        match event {
            Event::StartArray | Event::StartObject => {
                depth += 1;
                max = max.max(depth);
            }
            Event::EndArray | Event::EndObject => depth -= 1,
            _ => {}
        }
    }
    max
}

#[test]
fn default_max_depth_is_512() {
    assert_eq!(DEFAULT_MAX_DEPTH, 512);
    assert_eq!(Options::default().max_depth(), 512);
}

#[test]
fn arrays_at_the_default_limit_are_accepted() {
    let text = arrays(512);
    let events = parse_with(&text, Options::default()).expect("512 levels");
    assert_eq!(events.len(), 1024);
    assert_eq!(max_nesting(&events), 512);
    check_balanced(&events).unwrap();
}

#[test]
fn arrays_one_past_the_default_limit_are_rejected() {
    // Also the innermost-empty case, which the `json` crate accepted.
    let text = arrays(513);
    assert_eq!(
        depth_error(&text, Options::default()),
        (ErrorKind::DepthExceeded, 512)
    );
    let items = items_with(&text, Options::default());
    assert_eq!(items.len(), 513, "512 starts, then the error");
    assert!(items[..512].iter().all(|i| *i == Ok(Event::StartArray)));
}

#[test]
fn objects_at_and_past_the_default_limit() {
    let text = objects(512);
    let events = parse_with(&text, Options::default()).expect("512 levels");
    assert_eq!(max_nesting(&events), 512);
    check_balanced(&events).unwrap();
    assert_eq!(
        depth_error(&objects(513), Options::default()),
        (ErrorKind::DepthExceeded, 5 * 512)
    );
}

#[test]
fn mixed_nesting_at_and_past_the_default_limit() {
    let (text, _) = mixed(512);
    let events = parse_with(&text, Options::default()).expect("512 levels");
    assert_eq!(max_nesting(&events), 512);
    check_balanced(&events).unwrap();
    let (text, openers) = mixed(513);
    assert_eq!(
        depth_error(&text, Options::default()),
        (ErrorKind::DepthExceeded, openers[512])
    );
}

#[test]
fn depth_is_released_when_containers_close() {
    // Two siblings, each 511 deep inside one outer array: 512 at most.
    let text = format!("[{},{},{{}}]", arrays(511), arrays(511));
    let events = parse_with(&text, Options::default()).expect("siblings");
    assert_eq!(max_nesting(&events), 512);
}

#[test]
fn custom_limits() {
    let three = Options::default().with_max_depth(3);
    assert!(parse_with("[{\"a\":[]}]", three).is_ok());
    assert_eq!(
        depth_error("[{\"a\":[[]]}]", three),
        (ErrorKind::DepthExceeded, 7)
    );
    let zero = Options::default().with_max_depth(0);
    assert_eq!(parse_with("1", zero), Ok(vec![Event::Number("1")]));
    assert_eq!(depth_error("[]", zero), (ErrorKind::DepthExceeded, 0));
    assert_eq!(depth_error("{}", zero), (ErrorKind::DepthExceeded, 0));
}

#[test]
fn json_test_suite_100000_opening_arrays_hits_the_depth_limit() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vendor/xpath-tests/misc/JSONTestSuite/test_parsing")
        .join("n_structure_100000_opening_arrays.json");
    let bytes = std::fs::read(&path).expect("JSONTestSuite file");
    let text = std::str::from_utf8(&bytes).expect("UTF-8");
    assert!(text.len() >= 100_000);
    assert_eq!(
        depth_error(text, Options::default()),
        (ErrorKind::DepthExceeded, 512)
    );
}

#[test]
fn a_million_levels_parse_without_recursion() {
    // A recursive descent parser overflows the test thread's stack long
    // before this depth; the explicit stack only needs a byte per level.
    const DEPTH: usize = 1_000_000;
    let text = arrays(DEPTH);
    let options = Options::default().with_max_depth(DEPTH);
    let mut starts = 0usize;
    let mut ends = 0usize;
    let mut ended = false;
    for item in Tokenizer::with_options(&text, options).take(text.len() + 2) {
        match item {
            Ok(Event::StartArray) => starts += 1,
            Ok(Event::EndArray) => ends += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    let mut tokenizer = Tokenizer::with_options(&text, options);
    for _ in 0..text.len() + 2 {
        if tokenizer.next().is_none() {
            ended = true;
            break;
        }
    }
    assert!(ended);
    assert_eq!((starts, ends), (DEPTH, DEPTH));
    // One more level than the raised limit is still rejected.
    let deeper = arrays(DEPTH + 1);
    let error = Tokenizer::with_options(&deeper, options)
        .take(deeper.len() + 2)
        .find_map(Result::err)
        .expect("rejected");
    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::DepthExceeded, DEPTH)
    );
}
