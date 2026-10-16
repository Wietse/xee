use super::*;
use crate::backtrack::Frame;
use crate::{Matcher, MAX_CALL_BYTES};

/// Enough for every call here but those that test the budget.
const STEPS: u64 = 50_000_000;

fn re(p: &str, f: &str) -> Regex {
    Regex::new(p, f).unwrap()
}

/// The pattern on the backtracker, whatever its program.
fn back(p: &str, f: &str) -> Regex {
    let mut r = re(p, f);
    r.set_matcher(Matcher::Backtrack);
    r
}

/// The answer of both matchers, which must agree.
fn both<T: PartialEq + std::fmt::Debug>(
    p: &str,
    f: &str,
    op: impl Fn(&Regex, &mut Budget) -> T,
) -> T {
    let pike = op(&re(p, f), &mut Budget::new(STEPS));
    let backtracker = op(&back(p, f), &mut Budget::new(STEPS));
    assert_eq!(
        pike, backtracker,
        "{p:?} flags {f:?}: the matchers disagree"
    );
    pike
}

fn matches(hay: &str, p: &str, f: &str) -> bool {
    both(p, f, |r, b| r.is_match(hay, b)).unwrap()
}

fn replace(hay: &str, p: &str, repl: &str, f: &str) -> Result<String, Error> {
    both(p, f, |r, b| r.replace(hay, repl, b))
}

fn tokenize(hay: &str, p: &str, f: &str) -> Result<Vec<String>, Error> {
    both(p, f, |r, b| r.tokenize(hay, b))
}

/// `fn:analyze-string`'s result, a match in brackets and a group in
/// braces after its number.
fn analyze(hay: &str, p: &str, f: &str) -> Result<String, Error> {
    fn content(entries: &[MatchEntry], out: &mut String) {
        for e in entries {
            match e {
                MatchEntry::String(s) => out.push_str(s),
                MatchEntry::Group { nr, value } => {
                    out.push_str(&format!("{{{nr}:"));
                    content(value, out);
                    out.push('}');
                }
            }
        }
    }
    let mut out = String::new();
    for e in both(p, f, |r, b| r.analyze(hay, b))? {
        match e {
            AnalyzeEntry::NonMatch(s) => out.push_str(&s),
            AnalyzeEntry::Match(m) => {
                out.push('[');
                content(&m, &mut out);
                out.push(']');
            }
        }
    }
    Ok(out)
}

/// `fn:analyze-string`'s entries themselves, where `analyze` would hide an
/// empty string.
fn entries(hay: &str, p: &str) -> Vec<AnalyzeEntry> {
    both(p, "", |r, b| r.analyze(hay, b)).unwrap()
}

fn text(s: &str) -> MatchEntry {
    MatchEntry::String(s.to_string())
}

fn group(nr: usize, value: Vec<MatchEntry>) -> MatchEntry {
    MatchEntry::Group { nr, value }
}

/// The steps a call takes on the backtracker.
fn spent(p: &str, f: &str, op: &dyn Fn(&Regex, &mut Budget)) -> u64 {
    let mut r = back(p, f);
    // The matcher's own cost, without the prefilter's scans.
    r.set_prefilter(false);
    let mut budget = Budget::new(STEPS);
    op(&r, &mut budget);
    budget.spent()
}

fn limit<T: std::fmt::Debug>(r: Result<T, Error>, what: &str) {
    match r {
        Err(Error::Limit(m)) => assert!(m.contains(what), "{m}"),
        other => panic!("expected a limit ({what}), got {other:?}"),
    }
}

#[test]
fn the_examples_of_matches() {
    // F&O 3.1 §5.6.3.
    assert!(matches("abracadabra", "bra", ""));
    assert!(matches("abracadabra", "^a.*a$", ""));
    assert!(!matches("abracadabra", "^bra", ""));
    let poem = "\nKaum hat dies der Hahn gesehen,\nFängt er auch schon an zu krähen:\n\
                Kikeriki! Kikikerikih!!\nTak, tak, tak! - da kommen sie.\n";
    assert!(!matches(poem, "Kaum.*krähen", ""));
    assert!(matches(poem, "Kaum.*krähen", "s"));
    assert!(matches(poem, "^Kaum.*gesehen,$", "m"));
    assert!(!matches(poem, "^Kaum.*gesehen,$", ""));
    assert!(matches(poem, "kiki", "i"));
    // §5.6.2's flags.
    assert!(!matches("abcd\ndefg\n", "^$", "m"));
    assert!(!matches("hello world", "helloworld", "x"));
    assert!(matches("helloworld", "hello world", "x"));
    assert!(!matches("helloworld", "hello[ ]world", "x"));
    assert!(matches("hello world", "hello\\ sworld", "x"));
    assert!(!matches("abcd", ".*", "q"));
    assert!(matches("Mr. B. Obama", "B. OBAMA", "iq"));
}

#[test]
fn the_examples_of_replace() {
    // F&O 3.1 §5.6.4.
    for (hay, p, repl, out) in [
        ("abracadabra", "bra", "*", "a*cada*"),
        ("abracadabra", "a.*a", "*", "*"),
        ("abracadabra", "a.*?a", "*", "*c*bra"),
        ("abracadabra", "a", "", "brcdbr"),
        ("abracadabra", "a(.)", "a$1$1", "abbraccaddabbra"),
        ("AAAA", "A+", "b", "b"),
        ("AAAA", "A+?", "b", "bbbb"),
        ("darted", "^(.*?)d(.*)$", "$1c$2", "carted"),
        ("abcd", "(ab)|(a)", "[1=$1][2=$2]", "[1=ab][2=]cd"),
    ] {
        assert_eq!(replace(hay, p, repl, "").unwrap(), out, "{p}");
    }
    assert_eq!(
        replace("abracadabra", ".*?", "$1", ""),
        Err(Error::MatchesEmptyString)
    );
}

#[test]
fn the_examples_of_tokenize() {
    // F&O 3.1 §5.6.5.
    for (hay, p, f, out) in [
        (
            "The cat sat on the mat",
            "\\s+",
            "",
            &["The", "cat", "sat", "on", "the", "mat"][..],
        ),
        (
            " red green blue ",
            "\\s+",
            "",
            &["", "red", "green", "blue", ""],
        ),
        ("1, 15, 24, 50", ",\\s*", "", &["1", "15", "24", "50"]),
        ("1,15,,24,50,", ",", "", &["1", "15", "", "24", "50", ""]),
        (
            "Some unparsed <br> HTML <BR> text",
            "\\s*<br>\\s*",
            "i",
            &["Some unparsed", "HTML", "text"],
        ),
        ("abracadabra", "(ab)|(a)", "", &["", "r", "c", "d", "r", ""]),
    ] {
        assert_eq!(tokenize(hay, p, f).unwrap(), out, "{p}");
    }
    assert_eq!(tokenize("abba", ".?", ""), Err(Error::MatchesEmptyString));
}

#[test]
fn the_examples_of_analyze_string() {
    // F&O 3.1 §5.6.6.
    assert_eq!(
        analyze("The cat sat on the mat.", "\\w+", "").unwrap(),
        "[The] [cat] [sat] [on] [the] [mat]."
    );
    assert_eq!(
        analyze("2008-12-03", "^(\\d+)\\-(\\d+)\\-(\\d+)$", "").unwrap(),
        "[{1:2008}-{2:12}-{3:03}]"
    );
    assert_eq!(
        analyze("A1,C15,,D24, X50,", "([A-Z])([0-9]+)", "").unwrap(),
        "[{1:A}{2:1}],[{1:C}{2:15}],,[{1:D}{2:24}], [{1:X}{2:50}],"
    );
    // An empty input gives an element with no children.
    assert_eq!(analyze("", "a", "").unwrap(), "");
    assert_eq!(analyze("b", "a*", ""), Err(Error::MatchesEmptyString));
    // Nested groups, a group that took no part, and an empty group.
    assert_eq!(
        analyze("ab", "((a)(x)?)(b)()", "").unwrap(),
        "[{1:{2:a}}{4:b}{5:}]"
    );
    // No empty text before a group or at the end.
    assert_eq!(
        entries("xa", "x(a)()"),
        [AnalyzeEntry::Match(vec![
            text("x"),
            group(1, vec![text("a")]),
            group(2, vec![])
        ])]
    );
    // A group from an earlier iteration than its enclosing group's last
    // one lies outside it, and nests under the next enclosing group that
    // contains it.
    assert_eq!(
        entries("ab", "((a)|b)+"),
        [AnalyzeEntry::Match(vec![
            group(2, vec![text("a")]),
            group(1, vec![text("b")])
        ])]
    );
    // Siblings in the order of their captures, not of their numbers.
    assert_eq!(
        entries("ba", "(?:(a)|(b))+"),
        [AnalyzeEntry::Match(vec![
            group(2, vec![text("b")]),
            group(1, vec![text("a")])
        ])]
    );
    // The next enclosing group that contains it, not the match: group 3,
    // from the first iteration, lies outside group 2's last capture and
    // inside group 1.
    assert_eq!(
        entries("xab", "(x((a)|b)+)"),
        [AnalyzeEntry::Match(vec![group(
            1,
            vec![
                text("x"),
                group(3, vec![text("a")]),
                group(2, vec![text("b")])
            ]
        )])]
    );
    // An empty group from an earlier iteration, at the start of its parent's
    // last capture, comes before a sibling that starts there too.
    assert_eq!(
        entries("ba", "((a)|b())+"),
        [AnalyzeEntry::Match(vec![
            text("b"),
            group(1, vec![group(3, vec![]), group(2, vec![text("a")])])
        ])]
    );
}

#[test]
fn the_error_conditions() {
    // FORX0003 whatever the input, but `fn:tokenize` of an empty input is
    // the empty sequence before the pattern is looked at.
    assert!(tokenize("", ".?", "").unwrap().is_empty());
    assert_eq!(replace("", "a*", "x", ""), Err(Error::MatchesEmptyString));
    // F&O orders no two errors; FORX0003 is found first here.
    assert_eq!(replace("x", "a*", "$", ""), Err(Error::MatchesEmptyString));
    // FORX0004, whether or not anything matches; not with `q`.
    for repl in ["$", "a$b", "\\", "\\a", "x\\"] {
        assert!(
            matches!(
                replace("b", "a", repl, ""),
                Err(Error::InvalidReplacement(_))
            ),
            "{repl}"
        );
    }
    assert_eq!(replace("a", "a", "\\$\\\\", "").unwrap(), "$\\");
    // The error says what is wrong, and where.
    for (repl, why) in [
        ("a$b", "a $ at byte 1 is not followed by a digit"),
        ("ab\\c", "a \\ at byte 2 is not followed by \\ or $"),
    ] {
        let e = replace("b", "a", repl, "").unwrap_err();
        assert_eq!(e, Error::InvalidReplacement(why.to_string()));
        assert_eq!(e.to_string(), format!("invalid replacement string: {why}"));
    }
    assert_eq!(replace("a", "a", "$\\", "q").unwrap(), "$\\");
}

#[test]
fn the_digits_after_a_dollar() {
    // F&O 3.1 §5.6.4: N is all the digits; while N > S and N > 9, its last
    // digit is a literal; S < N <= 9 is the empty string.
    for (repl, out) in [
        ("[$01]", "[a]"),
        ("[$05]", "[]"),
        ("[$0]", "[a]"),
        ("[$1]", "[a]"),
        ("[$12]", "[a2]"),
        ("[$001]", "[a]"),
        ("[$00]", "[a]"),
        ("[$000]", "[a]"),
        ("[$09]", "[]"),
        ("[$10]", "[a0]"),
        ("[$100]", "[a00]"),
    ] {
        assert_eq!(replace("a", "(a)", repl, "").unwrap(), out, "{repl}");
    }
    assert_eq!(
        replace("abcde", "(a)(b)(c)(d)(e)", "$23", "").unwrap(),
        "b3"
    );
    let ten = "(a)".repeat(10);
    assert_eq!(
        replace(&"a".repeat(10), &ten, "$10|$11", "").unwrap(),
        "a|a1"
    );
}

/// The corners a differential against the engine xee used before found,
/// where F&O decides otherwise than that engine answers.
#[test]
fn the_corners() {
    // §5.6.1.4: a group that matched nothing matches the empty string.
    assert!(matches("x", "(a){0}\\1x", ""));
    assert!(matches("", "k(a)|\\1", ""));
    // A repetition of zero iterations, before `^` or over an empty body.
    assert!(matches("b", "b*^", ""));
    assert!(matches("- x", "(?:.*){1,}-", ""));
    // §5.6.4's leftmost match: `yb` is in the language.
    assert!(matches("yb", "(?:.b?)*?b", ""));
    // §5.6.2: case variants are those of `fn:lower-case` and
    // `fn:upper-case`, and a negative group excludes them.
    assert!(matches("ς", "σ", "i"));
    assert!(matches("\u{212A}", "[A-Z]", "i"));
    assert!(!matches("ı", "[^a-z]", "i"));
    assert!(!matches("İ", "i", "i"));
    assert!(!matches("q", "[^Q]", "i"));
    assert!(!matches("a", "\\p{Lu}", "i"));
    // A back-reference compared case-blind.
    for word in ["Mum", "mom", "Dad", "DUD"] {
        assert!(matches(word, "([md])[aeiou]\\1", "i"), "{word}");
    }
    assert!(!matches("Mud", "([md])[aeiou]\\1", "i"));
    // Without `i`, the same character only.
    assert!(!matches("aA", "^(a)\\1$", ""));
    assert!(matches("aa", "^(a)\\1$", ""));
    // Case variants of other widths: the Kelvin sign is three bytes.
    assert!(matches("k\u{212A}", "^(k)\\1$", "i"));
    assert!(matches("\u{212A}k", "^(.)\\1$", "i"));
    assert!(matches("\u{212A}kx", "^(.)\\1x$", "i"));
    // Variants whose lower cases differ: `ſ` and `s` share an upper case,
    // as `ς` and `σ` do.
    assert!(matches("sſ", "^(s)\\1$", "i"));
    assert!(matches("ςσ", "^(.)\\1$", "i"));
    // A huge count: false on an input too short for it.
    assert!(!matches("aaa", "a{2147483647}", ""));
}

/// Lazy repetitions whose iterations can match the empty string, which a
/// matcher that repeats an empty iteration loops on forever, answer at
/// once on both matchers; and patterns that match the empty string only
/// through `^` or a back-reference to a group that matched nothing are
/// refused by `fn:tokenize` (FORX0003), not searched.
#[test]
fn lazy_empty_iterations_end() {
    for (hay, p) in [
        ("ab", "()*?b"),
        ("yb", "(?:.b?)+?b"),
        ("", "(?:a(b*))*\\1+?"),
    ] {
        for r in [re(p, ""), back(p, "")] {
            let mut budget = Budget::new(STEPS);
            assert!(r.is_match(hay, &mut budget).unwrap(), "{p}");
            assert!(budget.spent() < 200, "{p}: {}", budget.spent());
        }
    }
    for (hay, p) in [("ab", "(?:^|x){2}"), ("bk", "k(a)|\\1")] {
        assert_eq!(tokenize(hay, p, ""), Err(Error::MatchesEmptyString), "{p}");
    }
}

/// F&O 3.1 §5.6.2: with `m`, `^` does not match after a newline that ends
/// the input, and `$` matches at the end only if no newline ends it.
#[test]
fn the_anchors_in_multi_line_mode() {
    assert!(matches("a\nb", "a$", "m"));
    assert!(matches("a\nb", "^b", "m"));
    assert!(!matches("a\n", "\n^", "m"));
    assert!(!matches("a\n", "\n$", "m"));
    assert!(matches("a\n", "a$", "m"));
    assert!(matches("", "^$", "m"));
    // Without `m`, only the ends of the input.
    assert!(matches("a\n", "\n$", ""));
    assert!(!matches("a\nb", "a$", ""));
    assert!(!matches("a\nb", "^b", ""));
}

fn xsd(p: &str) -> Regex {
    Regex::xsd(p).unwrap()
}

/// F&O's four operations on a pattern of the XSD dialect, which matches a
/// whole input or nothing, each under its caller's budget.
#[test]
fn the_operations_on_the_xsd_dialect() {
    let mut budget = Budget::new(STEPS);
    let b = &mut budget;
    let r = xsd("(a)(b)?|c");
    assert!(r.is_match("ab", b).unwrap() && r.is_match("a", b).unwrap());
    assert!(!r.is_match("xab", b).unwrap() && !r.is_match("ac", b).unwrap());
    // `$` in a replacement string is F&O 3.1 §5.6.4's, whatever the
    // dialect.
    assert_eq!(r.replace("ab", "[$2$1]", b).unwrap(), "[ba]");
    assert_eq!(r.replace("a", "[$2$1\\$]", b).unwrap(), "[a$]");
    assert_eq!(r.replace("abab", "[$1]", b).unwrap(), "abab");
    assert!(matches!(
        r.replace("ab", "$", b),
        Err(Error::InvalidReplacement(_))
    ));
    assert_eq!(r.tokenize("c", b).unwrap(), ["", ""]);
    assert_eq!(r.tokenize("cc", b).unwrap(), ["cc"]);
    assert_eq!(
        r.analyze("ab", b).unwrap(),
        [AnalyzeEntry::Match(vec![
            group(1, vec![text("a")]),
            group(2, vec![text("b")])
        ])]
    );
    assert_eq!(
        r.analyze("abc", b).unwrap(),
        [AnalyzeEntry::NonMatch("abc".into())]
    );
    // FORX0003, from the pattern's minimum length.
    for op in [
        |r: &Regex, b: &mut Budget| r.tokenize("a", b).map(|_| ()),
        |r: &Regex, b: &mut Budget| r.replace("a", "x", b).map(|_| ()),
        |r: &Regex, b: &mut Budget| r.analyze("a", b).map(|_| ()),
    ] {
        assert_eq!(op(&xsd("a*"), b), Err(Error::MatchesEmptyString));
    }
}

/// The XSD dialect has no back-references, so its patterns run on the
/// PikeVM: `(a+)+` on 30 `a`s and a `b` answers in a few hundred steps,
/// where the backtracker spends any budget a test could give.
#[test]
fn the_xsd_dialect_runs_on_the_pikevm() {
    let hay = format!("{}b", "a".repeat(30));
    let r = xsd("(a+)+");
    let mut budget = Budget::new(10_000);
    assert!(!r.is_match(&hay, &mut budget).unwrap());
    assert!(budget.spent() < 40 * 31, "{}", budget.spent());
    let mut r = xsd("(a+)+");
    r.set_matcher(Matcher::Backtrack);
    limit(r.is_match(&hay, &mut Budget::new(STEPS)), "budget");
}

/// An operation on one input under a budget.
type Op<'a> = dyn Fn(&mut Budget) -> Result<(), Error> + 'a;

/// A pattern of the XSD dialect is refused at its caller's budget and
/// memory ceiling, in each of the four operations, as one of the XPath
/// dialect is.
#[test]
fn the_xsd_dialect_is_refused_at_its_budget() {
    let r = xsd("([a-z]+)");
    let hay = "a".repeat(1000);
    let ops: [&Op; 4] = [
        &|b| r.is_match(&hay, b).map(|_| ()),
        &|b| r.replace(&hay, "[$1]", b).map(|_| ()),
        &|b| r.tokenize(&hay, b).map(|_| ()),
        &|b| r.analyze(&hay, b).map(|_| ()),
    ];
    for op in ops {
        let mut budget = Budget::new(STEPS);
        op(&mut budget).unwrap();
        let cost = budget.spent();
        assert!(cost > 1000);
        assert!(op(&mut Budget::new(cost)).is_ok());
        limit(op(&mut Budget::new(cost - 1)), "budget");
    }
    // The memory a call holds: a replacement or an analysis of a
    // thousand-character match, and the one token of an input that does
    // not match, each hold more than 1,000 bytes.
    let unmatched = format!("{hay}!");
    let held: [&Op; 3] = [ops[1], &|b| r.tokenize(&unmatched, b).map(|_| ()), ops[3]];
    for op in held {
        limit(op(&mut Budget::with_memory(STEPS, 1000)), "memory");
        assert!(op(&mut Budget::with_memory(STEPS, 1 << 20)).is_ok());
    }
}

/// Plan D11's capture model: a quantifier that takes no iteration unsets
/// the groups inside it; otherwise a group keeps its last value.
#[test]
fn the_capture_model() {
    assert_eq!(replace("xax", "^(?:x(a)?)+$", "[$1]", "").unwrap(), "[]");
    assert_eq!(replace("xax", "^(?:x(a)*)+$", "[$1]", "").unwrap(), "[]");
    assert_eq!(replace("ab", "^(?:(a)|b)+$", "[$1]", "").unwrap(), "[a]");
    // A match starts with every group unset, whatever the last one set.
    assert_eq!(replace("ab", "(a)|(b)", "[$1|$2]", "").unwrap(), "[a|][|b]");
    // A clearing exit taken and then backtracked over restores the group.
    assert_eq!(replace("acb", "(?:(a)?c)+", "[$1]", "").unwrap(), "[a]b");
    // QT3 p303 (W3C bug 29253) and §5.6.1.3's plain repeat.
    assert!(matches("babadad", "^((.)?a\\2)+$", ""));
    assert!(matches("aba", "^(?:(a)|b)+\\1$", ""));
    assert!(matches("acba", "^(?:(a)c|b)+\\1$", ""));
}

/// Plan D9: an optional iteration that matches the empty string ends the
/// repetition, keeping its captures; a required one runs regardless.
#[test]
fn an_empty_iteration_ends_the_repetition() {
    assert_eq!(replace("xa", "x(?:|a)*", "!", "").unwrap(), "!a");
    assert_eq!(replace("aab", "(a|)*b", "[$1]", "").unwrap(), "[]");
    for (q, answer) in [
        ("*", false),
        ("{0,4}", false),
        ("+", false),
        ("{1,4}", false),
        ("{0,4}?", false),
        ("{2}", false),
        ("{2,4}", true),
        ("{3}", true),
        ("{2,}", true),
    ] {
        let p = format!("^(?:(a|)|\\1b){q}$");
        assert_eq!(matches("ab", &p, ""), answer, "{p}");
    }
}

#[test]
fn class_subtraction() {
    // XSD 1.1 §G.4.1: a subtraction removes the subtracted class from the
    // class before it, negated first if it is a negative group.
    assert!(matches("b", "^[a-z-[aeiou]]$", ""));
    assert!(!matches("a", "^[a-z-[aeiou]]$", ""));
    assert!(matches("e", "^[a-z-[aeiou-[e]]]$", ""));
    assert!(!matches("o", "^[a-z-[aeiou-[e]]]$", ""));
    assert!(matches("B", "^[^a-z-[AEIOU]]$", ""));
    assert!(!matches("E", "^[^a-z-[AEIOU]]$", ""));
    assert!(!matches("b", "^[^a-z-[AEIOU]]$", ""));
}

#[test]
fn the_budget_bounds_a_call_not_a_search() {
    // Each search reads to the end to rule out `.*b`: quadratic in the
    // input over the call, on either matcher, while no one search comes
    // near the budget.
    for r in [re(".*b|a", ""), back(".*b|a", "")] {
        let hay = "a".repeat(2_000);
        limit(r.replace(&hay, "x", &mut Budget::new(1_000_000)), "budget");
        limit(r.tokenize(&hay, &mut Budget::new(1_000_000)), "budget");
        let mut budget = Budget::new(1_000_000);
        assert_eq!(
            r.replace(&hay[..200], "x", &mut budget).unwrap(),
            "x".repeat(200)
        );
        assert!(budget.spent() > 20_000, "{}", budget.spent());
    }
    // Exponential on the backtracker, refused; linear on the PikeVM.
    let hay = format!("{}b", "a".repeat(30));
    let mut budget = Budget::new(1_000_000);
    limit(back("^(a+)+$", "").is_match(&hay, &mut budget), "budget");
    assert_eq!(budget.spent(), 1_000_000);
    let mut budget = Budget::new(1_000_000);
    assert!(!re("^(a+)+$", "").is_match(&hay, &mut budget).unwrap());
    assert!(budget.spent() < 2_000, "{}", budget.spent());
    // On a long input too, about 33 steps a character.
    let n = 100_000;
    let hay = format!("{}b", "a".repeat(n));
    let mut budget = Budget::new(STEPS);
    assert!(!re("^(a+)+$", "").is_match(&hay, &mut budget).unwrap());
    assert!(budget.spent() < 40 * n as u64, "{}", budget.spent());
    // A back-reference puts it on the backtracker: refused.
    let hay = format!("{}b", "a".repeat(30));
    let mut budget = Budget::new(1_000_000);
    limit(re("^(a+)+\\1$", "").is_match(&hay, &mut budget), "budget");
    assert_eq!(budget.spent(), 1_000_000);
}

/// Pins the cost model: a step per instruction visit, plus one per slot or
/// register set up, saved or cleared, per byte a back-reference compares,
/// and per byte and part of each result. A change to any charge moves
/// these counts and must be explained.
#[test]
fn the_steps_of_small_calls() {
    // On the backtracker.
    let steps = |p: &str, op: &dyn Fn(&Regex, &mut Budget)| spent(p, "", op);
    let got = [
        steps(",", &|r, b| {
            assert_eq!(r.tokenize("a,b,c", b).unwrap().len(), 3);
        }),
        steps("^(a{10})\\1$", &|r, b| {
            assert!(r.is_match(&"a".repeat(20), b).unwrap());
        }),
        steps("b", &|r, b| {
            assert_eq!(r.replace("abc", "[$0]", b).unwrap(), "a[b]c");
        }),
        steps("(?:a?)*b", &|r, b| {
            assert!(r.is_match("aab", b).unwrap());
        }),
        steps("(?:(a)(b))*c", &|r, b| {
            assert_eq!(r.analyze("ababc", b).unwrap().len(), 1);
        }),
        // The clearing exit, taken.
        steps("(a)?b", &|r, b| {
            assert!(r.is_match("b", b).unwrap());
        }),
        // A back-reference that fails is charged its whole capture too.
        steps("^(a)\\1\\1", &|r, b| {
            assert!(!r.is_match("aab", b).unwrap());
        }),
        // A class of many ranges.
        steps("\\w", &|r, b| {
            assert!(r.is_match("a", b).unwrap());
        }),
    ];
    // By hand. The first: the cache's two slots, 2. A start position sets
    // up the two slots, 2; `Save` costs 2, any other visit 1, so a failing
    // start costs 2 + 3 and a match 2 + 6, and copying its slots out 2.
    // The searches from 0 and 2 cost 5 + 10 each, the one from 4 5 + 5 (the
    // end of the input is a start position too); each token costs a step
    // and its byte, 2 three times. 2 + 15 + 15 + 10 + 6 = 48.
    // The second: four slots, 4 for the cache, 4 to set up and 4 to copy
    // out; four `Save`s at 2; fourteen other visits (`^`, ten classes, the
    // back-reference, `$` and `Match`); the back-reference's ten bytes.
    // 12 + 8 + 14 + 10 = 44.
    // The third: the replacement's 4 bytes; the cache 2; a failing start
    // 5 and a match 8 + 2; the match's three parts and four bytes, 7; the
    // search from 2, 5 + 5; the text after the match, 1. 4 + 17 + 7 + 10
    // + 1 = 39.
    // The last: as a one-character class, 12, and 9 more for `\w`'s
    // ranges, between 2^9 and 2^10.
    assert_eq!(got, [48, 44, 39, 34, 64, 25, 41, 21]);
}

#[test]
fn a_call_that_finds_no_room_spends_nothing() {
    // An input shorter in bytes than the pattern's minimum length is
    // answered without a search.
    let mut budget = Budget::new(STEPS);
    assert!(!re("abc", "").is_match("ab", &mut budget).unwrap());
    assert_eq!(budget.spent(), 0);
    let mut budget = Budget::new(STEPS);
    assert!(re("a", "").tokenize("", &mut budget).unwrap().is_empty());
    assert_eq!(budget.spent(), 0);
}

#[test]
fn whether_a_pattern_matches_the_empty_string_costs_nothing() {
    // The pattern matches the empty string exactly when its minimum length
    // is zero: no search, whatever it would cost (2^30 paths here).
    let r = re("(?:(?:|){30}a|)", "");
    let mut budget = Budget::new(1);
    assert_eq!(r.tokenize("x", &mut budget), Err(Error::MatchesEmptyString));
    assert_eq!(budget.spent(), 0);
    for (p, empty) in [
        ("a?", true),
        ("^$", true),
        ("$^", true),
        ("(a)?\\1", true),
        ("(a){0}\\1", true),
        ("(?:a|$)", true),
        ("a", false),
        ("(a)\\1", false),
        ("a{0}b", false),
    ] {
        assert_eq!(re(p, "m").matches_empty(), empty, "{p}");
        assert_eq!(matches("", p, "m"), empty, "{p}");
    }
    // A pattern over the program ceiling is refused for FORX0003 too.
    assert_eq!(
        replace("x", "(?:a{2000000})?", "y", ""),
        Err(Error::MatchesEmptyString)
    );
}

#[test]
fn a_result_is_charged_by_its_size() {
    // 10,000 matches each replaced by 1,000 characters: 10 MB of result
    // from a 10 KB input, refused by the budget.
    let r = re("a", "");
    let hay = "a".repeat(10_000);
    limit(
        r.replace(&hay, &"x".repeat(1000), &mut Budget::new(5_000_000)),
        "budget",
    );
    let mut budget = Budget::new(5_000_000);
    assert_eq!(
        r.replace(&hay[..1000], "xy", &mut budget).unwrap().len(),
        2000
    );
    // One match whose result would be 10 MB is refused.
    let r = re(".+", "");
    limit(
        r.replace(
            &hay[..1000],
            &"$0".repeat(10_000),
            &mut Budget::new(5_000_000),
        ),
        "budget",
    );
    // Group references and literals, sized before they are built.
    assert_eq!(
        replace("xaay", "(a+)(b)?", "[$1|$2|$0]", "").unwrap(),
        "x[aa||aa]y"
    );
}

#[test]
fn the_text_after_the_last_match_is_charged() {
    // No search runs: the input is shorter than the pattern's minimum
    // length, so the whole input is the text after the last match.
    let r = re("z{500002}", "");
    let hay = "a".repeat(500_001);
    limit(r.replace(&hay, "x", &mut Budget::new(500_000)), "budget");
    limit(r.tokenize(&hay, &mut Budget::new(500_000)), "budget");
    limit(r.analyze(&hay, &mut Budget::new(500_000)), "budget");
    // Its bytes, and an item for `tokenize` and a non-empty `analyze`.
    let r = re("z{5}", "");
    let steps = |op: &dyn Fn(&mut Budget)| {
        let mut budget = Budget::new(STEPS);
        op(&mut budget);
        budget.spent()
    };
    let got = [
        steps(&|b| assert_eq!(r.replace("aaaa", "", b).unwrap(), "aaaa")),
        steps(&|b| assert_eq!(r.tokenize("aaaa", b).unwrap(), ["aaaa"])),
        steps(&|b| assert_eq!(r.analyze("aaaa", b).unwrap().len(), 1)),
    ];
    assert_eq!(got, [4, 5, 5]);
}

#[test]
fn a_replacement_is_charged_before_it_is_parsed() {
    // Over the budget and malformed: refused for its size, not parsed.
    let r = re("a", "");
    let repl = format!("{}\\", "x".repeat(1_000_001));
    let mut budget = Budget::new(1_000_000);
    limit(r.replace("b", &repl, &mut budget), "budget");
    assert_eq!(budget.spent(), 1_000_000);
    assert!(matches!(
        r.replace("b", "x\\", &mut Budget::new(STEPS)),
        Err(Error::InvalidReplacement(_))
    ));
}

#[test]
fn a_replacement_string_is_read_in_linear_time() {
    let repl = format!("${}", "1".repeat(200_000));
    assert_eq!(replace("a", "a", &repl, "").unwrap(), "1".repeat(199_999));
}

#[test]
fn the_group_tree_is_charged_by_its_walk() {
    // The same search as `fn:matches`; then the match's byte and an item,
    // 2; a step for each group looked at; one for each enclosing group a
    // walk up visits, each stopping at its parent here; and sorting the k
    // groups, k (1 + log2 k).
    let extra = |p: &str, hay: &str| {
        let mut r = back(p, "");
        r.set_prefilter(false);
        let mut analyze = Budget::new(STEPS);
        assert_eq!(r.analyze(hay, &mut analyze).unwrap().len(), 1);
        let mut matches = Budget::new(STEPS);
        assert!(r.is_match(hay, &mut matches).unwrap());
        analyze.spent() - matches.spent()
    };
    assert_eq!(extra("(?:(?:(?:a)))", "a"), 2);
    assert_eq!(extra("(((a)))", "a"), 2 + 3 + 2 + 3 * 2);
    // Fifty nested: 50 + 49 + 50 * 6, where a charge for each group's
    // depth would be 1,275.
    let deep = format!("{}a{}", "(".repeat(50), ")".repeat(50));
    assert_eq!(extra(&deep, "a"), 2 + 50 + 49 + 50 * 6);
    // A walk that goes up: group 2's capture, from the first iteration,
    // lies outside group 1's last, so its walk visits group 1 and leaves.
    assert_eq!(extra("((a)|b)+", "ab"), 3 + 2 + 1 + 2 * 2);
    // Walks that go up more than once: group 3 keeps `a` from the first
    // iteration, outside group 2's `c` and group 1's `b`, so its walk
    // visits both; group 2's visits group 1.
    assert_eq!(extra("(?:(b|(c|(a))))+", "acb"), 4 + 3 + (1 + 2) + 3 * 2);
}

#[test]
fn an_uncompiled_pattern_counts_the_input_it_reads() {
    // Over the instruction ceiling, so not compiled; its minimum length
    // fits the input's bytes but not its characters, so every byte is read.
    let r = re("a{2000000}", "");
    assert!(!r.is_compiled());
    let mut budget = Budget::new(STEPS);
    assert!(!r
        .is_match(&"\u{e9}".repeat(1_500_000), &mut budget)
        .unwrap());
    assert_eq!(budget.spent(), 3_000_000);
    // Long enough in characters: refused.
    limit(
        r.is_match(&"a".repeat(2_000_000), &mut Budget::new(STEPS)),
        "over a ceiling",
    );
}

#[test]
fn the_memory_a_call_holds_has_a_ceiling() {
    assert_eq!(Budget::new(1).memory, MAX_CALL_BYTES);
    let hold = |n| Budget::with_memory(1, 100).hold(n);
    assert!(hold(100).is_ok());
    limit(hold(101), "memory");
    // A sum past the largest count is refused, not wrapped round.
    let mut whole = Budget::with_memory(1, usize::MAX);
    assert!(whole.hold(usize::MAX).is_ok());
    limit(whole.hold(1), "memory");
    // What the allocator takes: an eight-byte header, rounded up to
    // sixteen, and no chunk under 32 bytes. A one-byte copy holds 32.
    for (bytes, taken) in [(0, 0), (1, 32), (24, 32), (25, 48), (40, 48), (41, 64)] {
        assert_eq!(crate::chunk(bytes), taken, "{bytes}");
    }
    assert!(crate::copy("a", &mut Budget::with_memory(1, 32)).is_ok());
    limit(crate::copy("a", &mut Budget::with_memory(1, 31)), "memory");
    // Grown a byte at a time to 64, a vector or a string holds the chunk of
    // its last capacity, 80, its earlier ones freed.
    let grown = |bytes| {
        let mut budget = Budget::with_memory(1, bytes);
        let (mut v, mut s) = (Vec::<u8>::new(), String::new());
        for _ in 0..64 {
            crate::reserve(&mut v, 1, &mut budget)?;
            v.push(0);
        }
        let mut other = Budget::with_memory(1, bytes);
        for _ in 0..64 {
            crate::reserve_str(&mut s, 1, &mut other)?;
            s.push('a');
        }
        Ok::<_, Error>(())
    };
    assert!(grown(80).is_ok());
    limit(grown(79), "memory");
    // The backtracker's cache: four slots of two words and one register,
    // 64 and 8 bytes, 80 and 32 in chunks.
    let r = re("(a?)*\\1", "");
    let prog = r.program.as_ref().unwrap();
    assert_eq!((prog.slots, prog.regs), (4, 1));
    let cache = |bytes| {
        crate::backtrack::Cache::new(prog, &mut Budget::with_memory(STEPS, bytes)).map(|_| ())
    };
    assert!(cache(112).is_ok());
    limit(cache(111), "memory");
    // So does a vector's or a string's first byte.
    let grow = |bytes| crate::reserve(&mut Vec::<u8>::new(), 1, &mut Budget::with_memory(1, bytes));
    assert!(grow(32).is_ok());
    limit(grow(31), "memory");
    let grow =
        |bytes| crate::reserve_str(&mut String::new(), 1, &mut Budget::with_memory(1, bytes));
    assert!(grow(32).is_ok());
    limit(grow(31), "memory");
    // The stack, three words a frame, doubles as a vector does: with the
    // cache's four slots of two words, 2^11 frames fit in 2^16 bytes and
    // 2^12 do not. Each `a` pushes four frames, and the end of the input
    // four more.
    assert_eq!(size_of::<Frame>(), 24);
    let r = re("^(?:(a)|b)*\\1$", "");
    let at = |n: usize| r.is_match(&"a".repeat(n), &mut Budget::with_memory(STEPS, 1 << 16));
    assert!(at(511).unwrap());
    limit(at(512), "memory");
    // A result: the slots of tokens, three words each, beside the cache's
    // two slots and two frames, each allocation in its chunk: 8,192 slots
    // take 196,624 bytes, the cache 48, the stack 64; 196,736 in all,
    // 8,192 times 24 and 128.
    let commas = ",".repeat(5000);
    let tokens = |bytes| back(",", "").tokenize(&commas, &mut Budget::with_memory(STEPS, bytes));
    limit(tokens(8192 * 24 + 127), "memory");
    assert_eq!(tokens(8192 * 24 + 128).unwrap().len(), 5001);
    // A group tree's entries, and the parsed replacement's parts.
    let groups = "()".repeat(100);
    limit(
        re(&format!("{groups}a"), "")
            .analyze(&"a".repeat(100), &mut Budget::with_memory(STEPS, 1 << 16)),
        "memory",
    );
    limit(
        re("(a)", "").replace(
            "b",
            &"$1".repeat(10_000),
            &mut Budget::with_memory(STEPS, 1 << 16),
        ),
        "memory",
    );
    // A match's slots, kept for the result, are held as the allocator takes
    // them: one more match of `,` holds 48 bytes more, its two slots 32.
    let held = |n: usize| {
        let mut budget = Budget::new(STEPS);
        let slots = re(",", "").match_slots(&",".repeat(n), &mut budget);
        assert_eq!(slots.unwrap().len(), n);
        budget.held
    };
    assert_eq!(held(6) - held(5), 48);
    // A call starts holding nothing, whatever its budget held before:
    // each call holds four token slots, two cache slots and two frames,
    // 224 bytes with the allocator's headers, so two would not fit in 256.
    let mut budget = Budget::with_memory(STEPS, 256);
    for _ in 0..3 {
        assert_eq!(back(",", "").tokenize(",,,", &mut budget).unwrap().len(), 4);
    }
}

#[test]
fn a_call_may_spend_its_whole_budget() {
    for r in [re("(a)?b", ""), back("(a)?b", "")] {
        let mut whole = Budget::new(STEPS);
        assert!(r.is_match("ab", &mut whole).unwrap());
        let cost = whole.spent();
        assert!(r.is_match("ab", &mut Budget::new(cost)).unwrap());
        limit(r.is_match("ab", &mut Budget::new(cost - 1)), "budget");
    }
}
