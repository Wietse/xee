use crate::{Budget, Error, Matcher, Regex};

const STEPS: u64 = 1_000_000;

/// A small deterministic generator (xorshift), so that a failing case
/// can be replayed from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

const QUANTIFIERS: [&str; 18] = [
    "?", "*", "+", "{0,2}", "{1,2}", "{2}", "{1,}", "{0,3}", "{2,3}", "??", "*?", "+?", "{0,2}?",
    "{1,2}?", "{2}?", "{1,}?", "{0,3}?", "{0}",
];

/// A pattern of the alternation, sequence and repetition of `a`, `b`,
/// `.`, classes, anchors and groups, nested to `depth`; with
/// `backrefs`, a back-reference to a group closed before it.
fn pattern(rng: &mut Rng, depth: u32, backrefs: bool, groups: &mut usize) -> String {
    let branches = 1 + rng.below(3);
    let mut out = Vec::new();
    for _ in 0..branches {
        let mut branch = String::new();
        for _ in 0..rng.below(4) {
            let atom = match rng.below(if depth > 0 { 10 } else { 7 }) {
                0 => "a".to_string(),
                1 => "b".to_string(),
                2 => rng.pick(&[".", "[ab]", "[^a]", "\\n"]).to_string(),
                3 => rng.pick(&["^", "$"]).to_string(),
                4 => String::new(),
                5 | 6 if backrefs && *groups > 0 => {
                    format!("\\{}", 1 + rng.below(*groups as u64))
                }
                5 | 6 => "a".to_string(),
                7 => format!("(?:{})", pattern(rng, depth - 1, backrefs, groups)),
                _ => {
                    let inner = pattern(rng, depth - 1, backrefs, groups);
                    *groups += 1;
                    format!("({inner})")
                }
            };
            branch.push_str(&atom);
            if !atom.is_empty() && rng.below(3) == 0 {
                branch.push_str(rng.pick(&QUANTIFIERS));
            }
        }
        out.push(branch);
    }
    out.join("|")
}

/// Every string over `a`, `b` and a newline up to `len` characters.
fn inputs(len: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut last = vec![String::new()];
    for _ in 0..len {
        last = last
            .iter()
            .flat_map(|s| ["a", "b", "\n"].map(|c| format!("{s}{c}")))
            .collect();
        all.extend(last.iter().cloned());
    }
    all
}

/// The slots of every successive match on each matcher, with the
/// prefilter and without it, or `None` where any refused for want of steps
/// or memory.
fn on_both(p: &str, f: &str, hay: &str) -> Option<(Vec<Vec<Option<usize>>>, bool)> {
    let mut r = Regex::new(p, f).ok()?;
    r.set_prefilter(false);
    let bare = r.match_slots(hay, &mut Budget::new(STEPS));
    r.set_prefilter(true);
    let pike = r.match_slots(hay, &mut Budget::new(STEPS));
    if let (Ok(a), Ok(b)) = (&bare, &pike) {
        assert_eq!(
            a, b,
            "{p:?} flags {f:?} on {hay:?}: the prefilter changed an answer"
        );
    }
    let pike_matches = r.is_match(hay, &mut Budget::new(STEPS));
    r.set_matcher(Matcher::Backtrack);
    let back = r.match_slots(hay, &mut Budget::new(STEPS));
    let back_matches = r.is_match(hay, &mut Budget::new(STEPS));
    match (pike, back, pike_matches, back_matches) {
        (Ok(a), Ok(b), Ok(x), Ok(y)) => {
            assert_eq!(a, b, "{p:?} flags {f:?} on {hay:?}: the slots differ");
            assert_eq!(x, y, "{p:?} flags {f:?} on {hay:?}: matches differs");
            Some((a, x))
        }
        (a, b, x, y) => {
            for r in [a.map(|_| ()), b.map(|_| ()), x.map(|_| ()), y.map(|_| ())] {
                assert!(
                    matches!(r, Ok(()) | Err(Error::Limit(_))),
                    "{p:?} on {hay:?}: {r:?}"
                );
            }
            None
        }
    }
}

/// The differential test the plan keeps permanently: the PikeVM and the
/// backtracker give the same slots for every match of generated patterns
/// without back-references, on every short input, and, debug builds
/// asserting it in the closure, no more iterations begin at a position
/// than repetitions enclose the instruction. The pattern also matches the
/// empty string exactly when its minimum length is zero (FORX0003), and no
/// match is shorter than that length, which a search also reads to give up
/// on a text too short.
#[test]
fn the_matchers_agree_on_every_slot() {
    let hays = inputs(4);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut compared, mut patterns) = (0, 0);
    for _ in 0..1_500 {
        let p = pattern(&mut rng, 3, false, &mut 0);
        let f = rng.pick(&["", "m", "s", "i", "ms"]);
        let Ok(r) = Regex::new(&p, f) else {
            continue;
        };
        patterns += 1;
        let empty = r.is_match("", &mut Budget::new(STEPS));
        if let Ok(empty) = empty {
            assert_eq!(empty, r.min_len() == 0, "{p:?} flags {f:?}");
        }
        for hay in &hays {
            let Some((slots, _)) = on_both(&p, f, hay) else {
                continue;
            };
            compared += 1;
            for m in &slots {
                let len = hay[m[0].unwrap()..m[1].unwrap()].chars().count();
                assert!(len as u64 >= r.min_len(), "{p:?} flags {f:?} on {hay:?}");
            }
        }
    }
    // Every pair compared: none refused on either matcher.
    assert!(patterns > 1_000, "{patterns}");
    assert_eq!(compared, patterns * hays.len());
}

/// Plan D9's relations between the loop forms of one body, written once:
/// `X{0,n}` matches nothing `X*` or `X{0,n+1}` does not, `X{m,n}` nothing
/// `X{m,}` does not, and `X+` what `X{1,}` does; with back-references on
/// the backtracker, without them on both matchers.
#[test]
fn the_loop_forms_agree() {
    let hays = inputs(4);
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut checked = 0;
    for k in 0..240 {
        // Half the bodies hold a back-reference, the others none.
        let backrefs = k % 2 == 0;
        let body = loop {
            let body = pattern(&mut rng, 1, backrefs, &mut 0);
            let backref = body
                .as_bytes()
                .windows(2)
                .any(|w| w[0] == b'\\' && w[1].is_ascii_digit());
            if !backrefs || backref {
                break body;
            }
        };
        let lazy = if rng.below(2) == 0 { "" } else { "?" };
        let answers = |q: &str| -> Option<Vec<bool>> {
            let p = format!("^(?:{body}){q}{lazy}$");
            let r = Regex::new(&p, "").ok()?;
            hays.iter()
                .map(|h| {
                    let x = r.is_match(h, &mut Budget::new(STEPS)).ok();
                    if !backrefs {
                        let mut b = Regex::new(&p, "").ok()?;
                        b.set_matcher(Matcher::Backtrack);
                        let y = b.is_match(h, &mut Budget::new(STEPS)).ok();
                        if let (Some(x), Some(y)) = (x, y) {
                            assert_eq!(x, y, "{p:?} on {h:?}");
                        }
                    }
                    x
                })
                .collect()
        };
        let within = |a: &Option<Vec<bool>>, b: &Option<Vec<bool>>, what: &str| {
            if let (Some(a), Some(b)) = (a, b) {
                for (h, (x, y)) in hays.iter().zip(a.iter().zip(b)) {
                    assert!(!x || *y, "{body:?}{lazy} {what} on {h:?}");
                }
            }
        };
        let star = answers("*");
        let mut prev: Option<Vec<bool>> = None;
        for n in 1..=3 {
            let cur = answers(&format!("{{0,{n}}}"));
            within(&cur, &star, &format!("{{0,{n}}} within *"));
            within(&prev, &cur, &format!("{{0,{}}} within {{0,{n}}}", n - 1));
            prev = cur;
        }
        for m in 1..=2 {
            let open = answers(&format!("{{{m},}}"));
            for n in m..m + 2 {
                within(
                    &answers(&format!("{{{m},{n}}}")),
                    &open,
                    &format!("{{{m},{n}}} within {{{m},}}"),
                );
            }
        }
        let (plus, one) = (answers("+"), answers("{1,}"));
        within(&plus, &one, "+ within {1,}");
        within(&one, &plus, "{1,} within +");
        checked += 1;
    }
    assert_eq!(checked, 240);
}

/// The cases that shaped the closure: an empty iteration entered after
/// another thread reached the same instruction at the same position
/// keeps its captures, as on the backtracker; nested loops up to five
/// iterations begun at one position.
#[test]
fn the_closure_tells_threads_apart_by_the_iterations_begun() {
    let cases: &[(&str, &str, &[&str])] = &[
        ("(\\s*|)*a.{0,2}", "", &["B aaaa"]),
        ("((a?)*)*", "", &[]),
        ("(?:(?:|a)*b?)*", "", &[]),
        ("((a|)*b?)*", "", &[]),
        ("(a?)*?b", "", &[]),
        ("((a)|b)*?a", "", &[]),
        ("(?:(a)|b)+?$", "", &[]),
        ("(^|a)*b", "m", &["b\nab"]),
        ("(a|$)*", "m", &["a\na"]),
        ("((?:a|)*)+", "", &[]),
        ("((a)|(b)|)*", "", &[]),
        ("(?:(a?)(b?))*", "", &[]),
        ("((|a)(|b))*a", "", &[]),
        ("(a{0,2}(b|)){1,}", "", &[]),
        ("((a?){2,3})*b?", "", &[]),
        ("(?:x(a)?)+", "", &["xax", "xxa"]),
        ("(((a?)*b?)*)*", "", &[]),
        ("(?:((a)?|b)*?)+?", "", &[]),
        ("(?:(?:(?:(a?)*)*)*)*", "", &[]),
        ("(?:(?:(?:(?:(a?)*)*)*)*)*", "", &[]),
        ("((((a?)*)*b?)*)*", "", &[]),
        ("(?:(a?){0,3}b?){0,3}", "", &[]),
        // A clearing exit clears its own groups only.
        ("(?:(b)?b|(a))*", "", &[]),
    ];
    let hays = inputs(4);
    for (p, f, extra) in cases {
        for hay in hays.iter().map(String::as_str).chain(extra.iter().copied()) {
            assert!(on_both(p, f, hay).is_some(), "{p:?} on {hay:?} refused");
        }
    }
    // The motivating case, by its answer: the match takes the space, and
    // group 1 is the empty string at 2, from the iteration after it.
    let r = Regex::new("(\\s*|)*a.{0,2}", "").unwrap();
    let slots = r.match_slots("B aaaa", &mut Budget::new(STEPS)).unwrap();
    assert_eq!(slots[0][..4], [Some(1), Some(5), Some(2), Some(2)]);
}

/// The PikeVM is linear in the input where the backtracker is not.
#[test]
fn the_pikevm_answers_what_the_backtracker_cannot() {
    let mut hay = "ab".repeat(50_000);
    hay.push_str("xc");
    let r = Regex::new("(a|b)*c", "").unwrap();
    let mut budget = Budget::new(50_000_000);
    assert!(r.is_match(&hay, &mut budget).unwrap());
    let mut budget = Budget::new(50_000_000);
    assert_eq!(r.replace(&hay, "", &mut budget).unwrap().len(), 100_001);
    let mut back = Regex::new("(a|b)*c", "").unwrap();
    back.set_matcher(Matcher::Backtrack);
    assert!(matches!(
        back.is_match(&hay, &mut Budget::new(50_000_000)),
        Err(Error::Limit(_))
    ));
}

/// The thread lists are held against the call's memory: a slot row for
/// each class and `Match`, as wide as the slots the call needs, so a call
/// that needs every group's slots holds far more than one that needs
/// none.
#[test]
fn the_thread_lists_are_held() {
    let r = Regex::new(&"(a)".repeat(200), "").unwrap();
    let hay = "a".repeat(200);
    let memory = || Budget::with_memory(STEPS, 1 << 18);
    assert!(r.is_match(&hay, &mut memory()).unwrap());
    assert!(matches!(
        r.analyze(&hay, &mut memory()),
        Err(Error::Limit(m)) if m.contains("memory")
    ));
}

/// Pins the PikeVM's cost model: a step per position reached, per thread
/// at each position and per instruction the closure visits, and one more
/// per slot set, per eight slots of a row copied, per word of the thread
/// lists, and per doubling of a class's ranges. A change to any charge
/// moves these counts and must be explained.
#[test]
fn the_steps_of_small_calls_on_the_pikevm() {
    let steps = |p: &str, op: &dyn Fn(&Regex, &mut Budget)| {
        let mut budget = Budget::new(STEPS);
        let mut r = Regex::new(p, "").unwrap();
        // The PikeVM's own cost, without the prefilter's scans.
        r.set_prefilter(false);
        op(&r, &mut budget);
        budget.spent()
    };
    let got = [
        steps("a", &|r, b| assert!(r.is_match("ba", b).unwrap())),
        steps(",", &|r, b| {
            assert_eq!(r.tokenize("a,b", b).unwrap().len(), 2)
        }),
        steps("(a)|b", &|r, b| {
            assert_eq!(r.analyze("ab", b).unwrap().len(), 2)
        }),
        // A class of many ranges.
        steps("\\w", &|r, b| assert!(r.is_match("a", b).unwrap())),
    ];
    // The first, by hand. The thread lists hold the closure's set, two
    // words per instruction and loop depth, for two lists: 16. Each
    // position reached is a step: 0, 1 and the end, 3. At each position a
    // new thread's closure visits `Save` and the class, 2, and each listed
    // instruction costs 1: at 0, 2 + 2; at 1, 2 + 2 and the closure after
    // the `a`, 2; at the end, 2 for the new thread, and the listed `Save`
    // and `Match`, 2. Copying the match's two slots out, 2. 16 + 3 + 4 + 6
    // + 4 + 2 = 35.
    // The last: 16 for the lists; two positions, 2; at 0 the new thread's
    // closure 2, the two listed instructions 2 and the class's 9 more for
    // `\w`'s ranges, and the closure after it 2; at the end the new
    // thread's 2 and the listed `Save` and `Match` 2; the slots copied out
    // 2. 39.
    assert_eq!(got, [35, 81, 159, 39]);
}

/// Pins what calls that reach each charge spend and hold: a call that needs
/// the bounds only of a pattern with groups, a replacement of `$0`, a
/// match that ends the search early, a clearing exit, a hundred groups.
/// A change to a charge or a held amount moves these and must be
/// explained.
#[test]
fn the_steps_and_memory_of_calls_that_reach_each_charge() {
    let cost = |p: &str, op: &dyn Fn(&Regex, &mut Budget)| {
        let mut budget = Budget::new(STEPS);
        let mut r = Regex::new(p, "").unwrap();
        // The PikeVM's own cost, without the prefilter's scans.
        r.set_prefilter(false);
        op(&r, &mut budget);
        (budget.spent(), budget.held)
    };
    let got = [
        cost("(,)", &|r, b| {
            assert_eq!(r.tokenize("a,b", b).unwrap().len(), 2)
        }),
        cost("(,)", &|r, b| {
            assert_eq!(r.replace("a,b", "$0", b).unwrap(), "a,b")
        }),
        cost("a+", &|r, b| assert!(r.is_match("aaaa", b).unwrap())),
        cost("(a)?b", &|r, b| {
            assert_eq!(r.analyze("b", b).unwrap().len(), 1)
        }),
        cost(&format!("{}a", "()".repeat(100)), &|r, b| {
            assert_eq!(r.analyze("a", b).unwrap().len(), 1)
        }),
        // A class followed by a class, with slots: the fast path past the
        // closure, its visit and its row copy.
        cost("ab", &|r, b| {
            assert_eq!(r.analyze("ab", b).unwrap().len(), 1)
        }),
        cost("a{3}", &|r, b| {
            assert_eq!(r.analyze("aaaa", b).unwrap().len(), 2)
        }),
        // A match before a long tail: the search stops once no thread is
        // left, and the next one runs the tail.
        cost(",", &|r, b| {
            assert_eq!(r.replace(",bcdefgh", "x", b).unwrap(), "xbcdefgh")
        }),
    ];
    // The first's memory, by hand: the PikeVM's lists, for six
    // instructions, two rows and two slots, a set of six words twice and a
    // table of four per list, 64 + 64 + 48, twice, 352; the current and the
    // match's slots, 32 each; the closure's two frames, 64; the two
    // tokens' slots, 64, and the tokens, 32 each. 608.
    assert_eq!(
        got,
        [
            (102, 608),
            (104, 544),
            (51, 320),
            (121, 944),
            (4229, 26704),
            (74, 576),
            (104, 736),
            (127, 480)
        ]
    );
}

/// The prefilter answers what the matchers would search for: a text
/// without a character every match holds, or without the literal every
/// match begins with, costs a scan, a step a byte, on either matcher.
#[test]
fn the_prefilter_rules_out_a_search() {
    let ab: String = "ab".repeat(50_000);
    for matcher in [Matcher::Pike, Matcher::Backtrack] {
        let steps = |p: &str, hay: &str| {
            let mut r = Regex::new(p, "").unwrap();
            r.set_matcher(matcher);
            let mut budget = Budget::new(STEPS);
            assert!(!r.is_match(hay, &mut budget).unwrap(), "{p}");
            budget.spent()
        };
        // No `c`: the input scanned once.
        assert_eq!(steps("(a|b)*c", &ab), 100_000);
        // No `x` for `\w{1,255}x`.
        assert_eq!(steps("\\w{1,255}x", &"a".repeat(10_000)), 10_000);
        // A literal that never occurs, in a text holding each of its
        // characters: the first `n` found in a window of 16, the matcher's
        // lists or slots set up, under 100, then the literal's windows
        // scanned, each with its length less one past it, about 40. A
        // search of every position would take five steps or more each.
        let parts = "nedle".repeat(24_000);
        assert!(!parts.contains("needle"));
        let s = steps("needle(s)?", &parts);
        assert!((120_000..=120_000 + 16 + 100 + 5 * 50).contains(&s), "{s}");
        // A literal at the end of a long text: each scan pays for the bytes
        // before it once, at most twice over.
        let n = 10_000;
        let r = Regex::new("needle(s)?", "").unwrap();
        assert!(r
            .is_match(&format!("{}needle", "x".repeat(n)), &mut Budget::new(STEPS))
            .unwrap());
        let s = {
            let mut r = r;
            r.set_matcher(matcher);
            let mut budget = Budget::new(STEPS);
            assert!(r
                .is_match(&format!("{}needle", "x".repeat(n)), &mut budget)
                .unwrap());
            budget.spent()
        };
        assert!(s <= 4 * n as u64 + 300, "{s}");
        // A candidate where the match then fails: the search goes on where
        // the literal next occurs, not at every position after it.
        let s = steps("needle[!?]", &format!("needle.{}", "x".repeat(n)));
        assert!(s <= 2 * n as u64 + 300, "{s}");
    }
    // Searches that start ever later scan the input once, not once each:
    // the cost of tokenizing doubles with the input. Here the required
    // characters come after a long run without one, which a scan from the
    // start of the text, at each search, would read again.
    let cost = |n: usize| {
        let r = Regex::new("x", "").unwrap();
        let hay = format!("{}{}", "a".repeat(n), "x".repeat(n));
        let mut budget = Budget::new(STEPS);
        assert_eq!(r.tokenize(&hay, &mut budget).unwrap().len(), n + 1);
        budget.spent()
    };
    let (one, two) = (cost(2_000), cost(4_000));
    assert!(two < 2 * one + 1_000, "{one} {two}");
}

/// A pattern anchored at the start, outside multi-line mode, is tried
/// there alone, on either matcher: a text that fails at its first
/// character is not searched further, nor one that does not begin with
/// the pattern's literal, and the searches the operations make past a
/// match, from a later position, end at once. The scan for a character
/// every match holds still reads a text without one, a step a byte.
#[test]
fn an_anchored_search_tries_the_start_alone() {
    let n = 100_000;
    // At most the call's set-up and a few positions' steps.
    let few = 200;
    let xs = "x".repeat(n);
    let spaces = format!("{}x", " ".repeat(n));
    let steps = |r: &Regex, hay: &str| {
        let mut budget = Budget::new(50 * STEPS);
        assert!(!r.is_match(hay, &mut budget).unwrap());
        budget.spent()
    };
    // Per matcher: `^\s*$`'s steps on the `x`s and a space's on the run,
    // and what `fn:replace` and `fn:tokenize` cost past the copy.
    for (matcher, at_x, a_space, replace, tokenize) in [
        (Matcher::Pike, 45, 10, 56, 58),
        (Matcher::Backtrack, 11, 5, 15, 17),
    ] {
        let with = |p: &str, f: &str| {
            let mut r = Regex::new(p, f).unwrap();
            r.set_matcher(matcher);
            r
        };
        let blank = with("^\\s*$", "");
        // The thread from the start dies at the first `x`: the call's
        // set-up and that position. A thread from the next position would
        // add a few steps, from every later one a step or more each.
        assert_eq!(steps(&blank, &xs), at_x, "{matcher:?}");
        // On a run of spaces the thread from the start crosses the run, and
        // no thread from a later position follows it.
        let s = steps(&blank, &spaces);
        assert_eq!(s, a_space * n as u64 + at_x, "{matcher:?}");
        // A back-reference runs on the backtracker either way.
        assert!(steps(&with("^(x)\\1y", ""), &xs) < few, "{matcher:?}");
        // A text that does not begin with the literal is refused by one
        // comparison, a step a byte of the literal, before a matcher runs:
        // here the backtracker would try 2^24 ways through the empty
        // groups, and the PikeVM visit each, at the start. The texts differ
        // from the literal at its first character and at its last.
        for text in ["zabc", "abzabc"] {
            let p = format!("^{}(abc)\\1", "(?:|)".repeat(24));
            assert_eq!(steps(&with(&p, ""), text), 3, "{matcher:?} {text}");
            let p = format!("^{}abc", "(?:|)".repeat(1_000));
            assert_eq!(steps(&with(&p, ""), text), 3, "{matcher:?} {text}");
        }
        // A step a byte, not a character: `ééx` is five bytes.
        assert_eq!(steps(&with("^ééx", ""), "ééy"), 5, "{matcher:?}");
        // With flag `m`, `^` also matches after a newline, and every
        // position is tried.
        assert!(steps(&with("^\\s*$", "m"), &xs) > n as u64, "{matcher:?}");
        // After the match at the start, the next search begins past it and
        // ends at once, before the literal's comparison and the required
        // character's scan read the text: the operation pays for what it
        // copies, a step a byte, and the call's set-up.
        let r = with("^a", "");
        let hay = format!("a{xs}");
        let mut budget = Budget::new(STEPS);
        assert_eq!(r.replace(&hay, "", &mut budget).unwrap(), xs);
        assert_eq!(budget.spent(), n as u64 + replace, "{matcher:?}");
        let mut budget = Budget::new(STEPS);
        assert_eq!(r.tokenize(&hay, &mut budget).unwrap(), ["", &xs]);
        assert_eq!(budget.spent(), n as u64 + tokenize, "{matcher:?}");
    }
    // Every pattern of the XSD dialect is anchored: a value that fails at
    // its first character, and holds a character the pattern requires, is
    // not searched further, and a literal facet's value is compared with
    // the literal once.
    let digits = format!("a{}", "1".repeat(n));
    assert!(steps(&Regex::xsd("[0-9]+").unwrap(), &digits) < few);
    let tail = format!("b{}", "a".repeat(n));
    for p in ["a".repeat(16), "abc".into()] {
        assert!(steps(&Regex::xsd(&p).unwrap(), &tail) < few, "{p}");
    }
    // A value without a digit is read to its end by the scan for one.
    let letters = format!("a{}", "b".repeat(n));
    assert_eq!(
        steps(&Regex::xsd("[0-9]+").unwrap(), &letters),
        n as u64 + 1
    );
}

/// The prefilter changes no answer on texts long enough to cross its
/// scans' windows, of characters one to four bytes long: literal-heavy
/// patterns, some anchored at the start, under three flag sets, on both
/// matchers.
#[test]
fn the_prefilter_changes_no_answer_on_long_texts() {
    const PIECES: [&str; 12] = [
        "a",
        "b",
        "é",
        "😀",
        "x",
        "ab",
        "é😀",
        "a?",
        "(b|é)",
        "[x😀]",
        "^",
        "(^a|^😀)",
    ];
    const LETTERS: [&str; 6] = ["a", "b", "é", "😀", "\n", "x"];
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut compared = 0;
    for _ in 0..600 {
        let mut p = String::new();
        for _ in 0..1 + rng.below(5) {
            p.push_str(rng.pick(&PIECES));
        }
        if rng.below(4) == 0 {
            p = format!("{p}|{}", rng.pick(&PIECES));
        }
        let f = rng.pick(&["", "m", "i"]);
        let Ok(mut r) = Regex::new(&p, f) else {
            continue;
        };
        for _ in 0..6 {
            let len = rng.below(300) as usize;
            let hay: String = (0..len).map(|_| rng.pick(&LETTERS)).collect();
            let mut answers = Vec::new();
            for matcher in [Matcher::Pike, Matcher::Backtrack] {
                for on in [true, false] {
                    r.set_matcher(matcher);
                    r.set_prefilter(on);
                    answers.push(r.match_slots(&hay, &mut Budget::new(STEPS)).unwrap());
                }
            }
            assert!(
                answers.windows(2).all(|w| w[0] == w[1]),
                "{p:?} flags {f:?} on {hay:?}"
            );
            compared += 1;
        }
    }
    assert!(compared > 3_000, "{compared}");
}
