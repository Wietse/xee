//! QT3's regex syntax tests (`vendor/xpath-tests/fn/matches.re.xml`),
//! which come from the XML Schema test suite's regular expressions and
//! test them wrapped in XPath, run unwrapped through the XSD dialect,
//! `Regex::xsd`.
//!
//! A test is in the wrapped form when its `test` expression, read as
//! XPath tokens (string literals, names, `$s`, parentheses and commas,
//! whitespace between them ignored), is exactly
//!
//! ```text
//! (every $s in tokenize(A, S) satisfies matches($s, '^(?:P)$'))
//!   and (every $s in tokenize(R, S) satisfies not(matches($s, '^(?:P)$')))
//! ```
//!
//! with A, R, S and the two patterns string literals, S the same one
//! character in both halves and not a metacharacter, and the two patterns
//! the same, `^(?:` then P then `)$`. `tokenize(A, S)` is the empty
//! sequence when A is the zero-length string, and otherwise every piece
//! between the occurrences of S, empty ones included. Such a test expects
//! every string of A to be in the language of P and none of R: through
//! `Regex::xsd(P)`, which matches a value whole, each of A matches and
//! none of R; and where it expects an error, `Regex::xsd(P)` refuses P.
//!
//! Left out: the tests with an `xsd-version 1.0` dependency, which xee's
//! runner skips (the dialect is XSD 1.1's); and those whose P the two
//! dialects read differently, since P uses syntax F&O 3.1 §5.6.1 adds to
//! XSD's: `XPATH_ONLY` lists each with how the XSD dialect reads it, and
//! the test checks that list against the tests whose P compiles to
//! another program, or is refused, in one dialect and not the other.

use xee_regex::{Budget, Error, Regex};
use xot::Xot;

/// How the XSD dialect reads a pattern that uses syntax of the XPath
/// dialect.
enum Reading {
    /// A syntax error.
    Refused(&'static str),
    /// `^` and `$` are characters: the pattern compiles to the program of
    /// this one, which has them in classes, where both dialects read them
    /// alike.
    Literal(&'static str, &'static str),
}

use Reading::{Literal, Refused};

const RELUCTANT: &str = "a reluctant quantifier (F&O 3.1 §5.6.1.2)";
const BACKREF: &str = "a back-reference (F&O 3.1 §5.6.1.4)";
const BOTH: &str = "a reluctant quantifier and a back-reference (F&O 3.1 §5.6.1.2, §5.6.1.4)";
const ANCHORS: &str = "`^` and `$`, anchors in XPath (F&O 3.1 §5.6.1.1)";

/// The wrapped tests whose P the dialects read differently.
const XPATH_ONLY: &[(&str, Reading)] = &[
    ("re00617", Refused(BACKREF)),
    ("re00672", Refused(RELUCTANT)),
    ("re00673", Refused(RELUCTANT)),
    ("re00689", Refused(RELUCTANT)),
    ("re00690", Refused(RELUCTANT)),
    ("re00691", Refused(RELUCTANT)),
    ("re00770", Literal(ANCHORS, "[\\^]aa[$]")),
    ("re00771", Literal(ANCHORS, "[\\^]Aa[$]")),
    (
        "re00779",
        Literal(ANCHORS, "[\\^]([a-z0-9]+)@([a-z]+)\\.([a-z]+)[$]"),
    ),
    (
        "re00780",
        Literal(ANCHORS, "[\\^]http://www.([a-zA-Z0-9]+)\\.([a-z]+)[$]"),
    ),
    ("re00789", Refused(RELUCTANT)),
    ("re00795", Literal(ANCHORS, "[\\^](cat|chat)")),
    ("re00818", Refused(RELUCTANT)),
    ("re00825", Refused(BACKREF)),
    ("re00831", Refused(RELUCTANT)),
    ("re00832", Refused(RELUCTANT)),
    ("re00833", Refused(RELUCTANT)),
    ("re00834", Refused(RELUCTANT)),
    ("re00835", Refused(RELUCTANT)),
    ("re00836", Refused(RELUCTANT)),
    ("re00837", Refused(RELUCTANT)),
    ("re00838", Refused(RELUCTANT)),
    ("re00839", Refused(RELUCTANT)),
    ("re00840", Refused(RELUCTANT)),
    ("re00841", Refused(RELUCTANT)),
    ("re00842", Refused(RELUCTANT)),
    ("re00843", Refused(RELUCTANT)),
    ("re00844", Refused(RELUCTANT)),
    ("re00932", Refused(BACKREF)),
    ("re00933", Refused(BACKREF)),
    ("re00934", Refused(BACKREF)),
    ("re00935", Refused(BOTH)),
    ("re00936", Refused(BOTH)),
    ("re00957", Refused(BACKREF)),
    (
        "re00982",
        Literal(
            ANCHORS,
            "[\\^]([0-9a-zA-Z]([-.\\w]*[0-9a-zA-Z])*@(([0-9a-zA-Z])+([-\\w]*[0-9a-zA-Z])*\\.)+[a-zA-Z]{2,9})",
        ),
    ),
];

/// The tests xee's runner skips, with an `xsd-version 1.0` dependency.
const XSD_1_0: [&str; 3] = ["re00056", "re00086", "re00102a"];

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Lit(String),
    Word(String),
}

/// The XPath tokens of the expressions this file's tests are written in,
/// or `None` for anything else.
fn tokens(text: &str) -> Option<Vec<Token>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            chars.next();
        } else if c == '\'' {
            chars.next();
            let mut lit = String::new();
            loop {
                match chars.next()? {
                    '\'' if chars.peek() == Some(&'\'') => {
                        chars.next();
                        lit.push('\'');
                    }
                    '\'' => break,
                    ch => lit.push(ch),
                }
            }
            out.push(Token::Lit(lit));
        } else if matches!(c, '(' | ')' | ',') {
            chars.next();
            out.push(Token::Word(c.to_string()));
        } else if c == '$' || c.is_ascii_alphabetic() {
            let mut word = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_alphanumeric() || ch == '$' || ch == '-' {
                    word.push(ch);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Token::Word(word));
        } else {
            return None;
        }
    }
    Some(out)
}

/// A, R, S and P of a test in the wrapped form.
struct Wrapped {
    accept: String,
    reject: String,
    sep: char,
    pattern: String,
}

fn wrapped(text: &str) -> Option<Wrapped> {
    // `_` is a string literal.
    let template = "( every $s in tokenize ( _ , _ ) satisfies matches ( $s , _ ) ) and \
                    ( every $s in tokenize ( _ , _ ) satisfies not ( matches ( $s , _ ) ) )";
    let toks = tokens(text)?;
    let shape: Vec<&str> = template.split(' ').collect();
    if toks.len() != shape.len() {
        return None;
    }
    let mut lits = Vec::new();
    for (tok, want) in toks.iter().zip(&shape) {
        match (tok, *want) {
            (Token::Lit(s), "_") => lits.push(s.clone()),
            (Token::Word(w), want) if w == want => {}
            _ => return None,
        }
    }
    let [accept, sep, p1, reject, sep2, p2] = <[String; 6]>::try_from(lits).ok()?;
    let mut sep_chars = sep.chars();
    let sep_char = sep_chars.next()?;
    if sep_chars.next().is_some() || sep != sep2 || "\\.?*+{}()|[]^$".contains(sep_char) {
        return None;
    }
    let pattern = p1.strip_prefix("^(?:")?.strip_suffix(")$")?.to_string();
    (p1 == p2).then_some(Wrapped {
        accept,
        reject,
        sep: sep_char,
        pattern,
    })
}

/// `fn:tokenize($input, S)` for a one-character S that is no
/// metacharacter.
fn split(input: &str, sep: char) -> Vec<&str> {
    if input.is_empty() {
        Vec::new()
    } else {
        input.split(sep).collect()
    }
}

fn listing(r: &Result<Regex, Error>) -> Result<Option<String>, ()> {
    r.as_ref().map(Regex::program_listing).map_err(|_| ())
}

/// Whether the two dialects read `pattern` alike: the XSD dialect's
/// program, or its refusal, is that of `^(?:pattern)$` in XPath.
fn read_alike(pattern: &str) -> bool {
    listing(&Regex::xsd(pattern)) == listing(&Regex::new(&format!("^(?:{pattern})$"), ""))
}

#[test]
fn the_wrapped_xsd_tests_answer_through_the_xsd_dialect() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../vendor/xpath-tests/fn/matches.re.xml"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut xot = Xot::new();
    let root = xot.parse(&text).unwrap();
    let ns = "http://www.w3.org/2010/09/qt-fots-catalog";
    let ns_id = xot.add_namespace(ns);
    let [test_case, test, result, dependency, error, assert_true, any_of] = [
        "test-case",
        "test",
        "result",
        "dependency",
        "error",
        "assert-true",
        "any-of",
    ]
    .map(|n| xot.add_name_ns(n, ns_id));
    let [name, type_, value] = ["name", "type", "value"].map(|n| xot.add_name(n));
    let child = |node, n| {
        xot.children(node)
            .find(|&c| xot.element(c).is_some_and(|e| e.name() == n))
    };

    let (mut cases, mut both_empty) = (0, 0);
    let (mut skipped, mut xpath_only, mut wrong) = (Vec::new(), Vec::new(), Vec::new());
    for node in xot.descendants(root) {
        if xot.element(node).is_none_or(|e| e.name() != test_case) {
            continue;
        }
        let id = xot.get_attribute(node, name).unwrap().to_string();
        let expr = xot
            .text_content_str(child(node, test).unwrap())
            .unwrap_or("");
        let Some(w) = wrapped(expr) else {
            continue;
        };
        cases += 1;
        if w.accept.is_empty() && w.reject.is_empty() {
            both_empty += 1;
        }
        let xsd_1_0 = xot.children(node).any(|c| {
            xot.element(c).is_some_and(|e| e.name() == dependency)
                && xot.get_attribute(c, type_) == Some("xsd-version")
                && xot.get_attribute(c, value) == Some("1.0")
        });
        if xsd_1_0 {
            skipped.push(id);
            continue;
        }
        if !read_alike(&w.pattern) {
            xpath_only.push((id, w));
            continue;
        }
        // What the test accepts: an answer, an error, or either.
        let res = child(node, result).unwrap();
        let outcomes: Vec<_> = xot
            .children(res)
            .filter_map(|c| xot.element(c).map(|e| (c, e.name())))
            .flat_map(|(c, n)| {
                if n == any_of {
                    xot.children(c)
                        .filter_map(|g| xot.element(g).map(|e| e.name()))
                        .collect()
                } else {
                    vec![n]
                }
            })
            .collect();
        let got = Regex::xsd(&w.pattern).map(|r| {
            let answers = |list: &str| -> Vec<(String, bool)> {
                split(list, w.sep)
                    .into_iter()
                    .map(|s| {
                        let m = r.is_match(s, &mut Budget::new(50_000_000)).unwrap();
                        (s.to_string(), m)
                    })
                    .collect()
            };
            let mut bad: Vec<String> = answers(&w.accept)
                .into_iter()
                .filter(|(_, m)| !m)
                .map(|(s, _)| format!("{s:?} does not match"))
                .collect();
            bad.extend(
                answers(&w.reject)
                    .into_iter()
                    .filter(|(_, m)| *m)
                    .map(|(s, _)| format!("{s:?} matches")),
            );
            bad
        });
        let fine = match &got {
            Ok(bad) => bad.is_empty() && outcomes.contains(&assert_true),
            Err(Error::Syntax(_)) => outcomes.contains(&error),
            Err(_) => false,
        };
        if !fine {
            wrong.push(format!("{id}: {:?}: {got:?}", w.pattern));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} tests:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    assert_eq!(cases, 682, "the tests in the wrapped form");
    assert_eq!(both_empty, 38, "those with both lists empty");
    assert_eq!(skipped, XSD_1_0);
    let ids: Vec<&str> = xpath_only.iter().map(|(id, _)| id.as_str()).collect();
    let listed: Vec<&str> = XPATH_ONLY.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids, listed,
        "the tests whose pattern the dialects read differently"
    );
    for ((id, w), (_, reading)) in xpath_only.iter().zip(XPATH_ONLY) {
        let pattern = &w.pattern;
        match reading {
            Refused(why) => {
                // XPath compiles it; the XSD dialect has no such syntax.
                assert!(Regex::new(pattern, "").is_ok(), "{id}");
                assert!(
                    matches!(Regex::xsd(pattern), Err(Error::Syntax(_))),
                    "{id}: {pattern:?} is {why}, a syntax error in XSD"
                );
            }
            Literal(why, alike) => {
                assert!(read_alike(alike), "{id}: {alike:?}");
                assert_eq!(
                    listing(&Regex::xsd(pattern)),
                    listing(&Regex::xsd(alike)),
                    "{id}: {pattern:?} has {why}, characters in XSD"
                );
                // The test's own strings, read with the anchors as
                // characters: XPath's `^(?:^Q$)$` is Q whole, so with a `^`
                // before and a `$` after, where P has them, each of A
                // matches and none of R; and since every string P matches
                // has them, none of A or R as written.
                let r = Regex::xsd(pattern).unwrap();
                let is = |s: &str| r.is_match(s, &mut Budget::new(50_000_000)).unwrap();
                let pre = if alike.starts_with("[\\^]") { "^" } else { "" };
                let post = if alike.ends_with("[$]") { "$" } else { "" };
                assert!(!pre.is_empty() || !post.is_empty(), "{id}");
                assert!(!w.accept.is_empty() || !w.reject.is_empty(), "{id}");
                for s in split(&w.accept, w.sep) {
                    assert!(is(&format!("{pre}{s}{post}")), "{id}: {s:?}");
                    assert!(!is(s), "{id}: {s:?}");
                }
                for s in split(&w.reject, w.sep) {
                    assert!(!is(&format!("{pre}{s}{post}")) && !is(s), "{id}: {s:?}");
                }
            }
        }
    }
}
