//! The front end: the grammar production by production (XSD 1.1 Part 2
//! Appendix G with F&O 3.1 §5.6.1), the flags (§5.6.2), the ceilings, and
//! the programs a few shapes compile to.

use crate::charset::CharSet;
use crate::compile;
use crate::parse::{self, Node};
use crate::{Error, Flags, Regex};

fn ok(p: &str, f: &str) -> Regex {
    Regex::new(p, f).unwrap_or_else(|e| panic!("{p:?} flags {f:?}: {e}"))
}

fn syntax(p: &str) {
    match Regex::new(p, "") {
        Err(Error::Syntax(_)) => {}
        other => panic!("{p:?}: expected a syntax error, got {other:?}"),
    }
}

fn limit(p: &str, f: &str, what: &str) {
    match Regex::new(p, f) {
        Err(Error::Limit(m)) => assert!(m.contains(what), "{m}"),
        other => panic!(
            "{}… flags {f:?}: expected a limit, got {:?}",
            p.chars().take(20).collect::<String>(),
            other.map(|_| ())
        ),
    }
}

/// The set a one-atom pattern parses to.
fn set(p: &str, f: &str) -> CharSet {
    match parse::parse(p, &Flags::parse(f).unwrap()).unwrap().node {
        Node::Set(s) => s,
        other => panic!("{p:?}: {other:?}"),
    }
}

fn has(s: &CharSet, c: char) -> bool {
    s.contains(c as u32)
}

fn listing(p: &str) -> String {
    ok(p, "").program_listing().unwrap()
}

#[test]
fn flags() {
    for f in ["", "s", "m", "i", "x", "q", "smixq", "ii"] {
        ok("a", f);
    }
    for f in ["g", "I", " ", "-i", "sm i"] {
        assert_eq!(
            Regex::new("a", f).unwrap_err(),
            Error::InvalidFlags(f.to_string())
        );
    }
    // §5.6.2: with `q`, `m`, `s` and `x` have no effect.
    assert_eq!(
        Flags::parse("qmsx").unwrap(),
        Flags {
            q: true,
            ..Flags::default()
        }
    );
}

#[test]
fn branches() {
    // regExp ::= branch ( '|' branch )*, and a branch may be empty.
    for p in ["", "a|b", "|", "a||b", "|a", "a|"] {
        ok(p, "");
    }
    syntax(")");
    syntax("a)");
    syntax("(a");
}

#[test]
fn quantifiers() {
    for p in [
        "a?", "a*", "a+", "a{2}", "a{2,}", "a{2,3}", "a{0}", "a{0,0}", "a{3,3}", "a??", "a*?",
        "a+?", "a{2,3}?", "a{2,}?",
    ] {
        ok(p, "");
    }
    for p in [
        "a{3,2}", "a{,3}", "a{", "a{2", "a{2,3", "a{x}", "a{2,x}", "a{-1}", "a**", "a*+",
        "a{2}{3}", "?", "*a", "+", "{1}", "a{1}}", "]", "}",
    ] {
        syntax(p);
    }
    // XSD's n <= m holds on the numbers as written, beyond u32.
    syntax("a{10,9}");
    syntax("a{0010,9}");
    ok("a{99999999999,99999999999}", "");
    syntax("a{99999999999,9999999999}");
    // A count past u32 saturates; QT3 cbcl-matches-004 accepts false or
    // FORX0002 for a 26-digit count.
    assert_eq!(
        ok("a{99999999999999999999999999}", "").min_len(),
        u64::from(u32::MAX)
    );
}

#[test]
fn atoms() {
    for p in [
        "(a)", "(?:a)", "()", "(?:)", "((a)(b))", "^", "$", "a^b$c", ".", "[a]",
    ] {
        ok(p, "");
    }
    // `(?` begins a non-capturing group only with ':'.
    syntax("(?a)");
    syntax("(?=a)");
    syntax("(?i)a");
    assert_eq!(ok("(a)(?:b)((c))", "").group_count(), 3);
    // `.` is every character but #xA and #xD, or every one with `s`.
    let dot = set(".", "");
    assert!(has(&dot, 'a') && has(&dot, '\u{10FFFF}'));
    assert!(!has(&dot, '\n') && !has(&dot, '\r'));
    assert!(has(&set(".", "s"), '\n'));
}

#[test]
fn single_character_escapes() {
    for (p, c) in [
        ("\\n", '\n'),
        ("\\r", '\r'),
        ("\\t", '\t'),
        ("\\\\", '\\'),
        ("\\|", '|'),
        ("\\.", '.'),
        ("\\?", '?'),
        ("\\*", '*'),
        ("\\+", '+'),
        ("\\(", '('),
        ("\\)", ')'),
        ("\\{", '{'),
        ("\\}", '}'),
        ("\\$", '$'),
        ("\\-", '-'),
        ("\\[", '['),
        ("\\]", ']'),
        ("\\^", '^'),
    ] {
        assert_eq!(set(p, ""), CharSet::single(c as u32), "{p}");
    }
    // A hyphen that starts a range must be followed by a singleChar other
    // than a hyphen (XSD 1.1 §G.4.1).
    syntax("[!--]");
    // Not in the grammar.
    for p in [
        "\\b", "\\B", "\\x41", "\\u0041", "\\0", "\\a", "\\e", "\\", "\\Q", "\\k",
    ] {
        syntax(p);
    }
}

#[test]
fn class_escapes() {
    let s = set("\\s", "");
    assert!(has(&s, ' ') && has(&s, '\t') && has(&s, '\n') && has(&s, '\r'));
    assert!(!has(&s, '\u{A0}'));
    assert!(has(&set("\\S", ""), 'a') && !has(&set("\\S", ""), ' '));
    assert!(has(&set("\\i", ""), '_') && !has(&set("\\i", ""), '1'));
    assert!(has(&set("\\c", ""), '1') && !has(&set("\\c", ""), ' '));
    assert!(has(&set("\\d", ""), '\u{0660}') && !has(&set("\\d", ""), 'a'));
    assert!(has(&set("\\D", ""), 'a'));
    // \w: every character but punctuation, separators and others.
    let w = set("\\w", "");
    assert!(has(&w, 'a') && has(&w, '1') && has(&w, '$'));
    assert!(!has(&w, '.') && !has(&w, ' ') && !has(&w, '\u{7}'));
    assert!(has(&set("\\W", ""), '.'));
    // Categories and blocks.
    assert!(has(&set("\\p{Lu}", ""), 'A') && !has(&set("\\p{Lu}", ""), 'a'));
    assert!(has(&set("\\P{Lu}", ""), 'a'));
    assert!(has(&set("\\p{L}", ""), 'a'));
    assert!(has(&set("\\p{IsBasicLatin}", ""), 'a'));
    assert!(!has(&set("\\P{IsBasicLatin}", ""), 'a'));
    // IsBlock ::= 'Is' [a-zA-Z0-9#x2D]+
    assert!(set("\\p{IsLatin-1Supplement}", "").contains(0xE9));
    assert!(has(&set("\\p{IsGreek}", ""), '\u{3B1}'));
    for p in [
        "\\p{Cs}",
        "\\p{LC}",
        "\\p{Letter}",
        "\\p{IsNoSuchBlock}",
        "\\p{Is}",
        "\\p{Is_Basic}",
        "\\p{}",
        "\\p",
        "\\pL",
        "\\p{Lu",
        "\\p{IsBasic Latin}",
    ] {
        syntax(p);
    }
}

#[test]
fn back_references() {
    ok("(a)\\1", "");
    assert!(ok("(a)\\1", "")
        .program_listing()
        .unwrap()
        .contains("back-references"));
    // F&O 3.1 §5.6.1: the group must have closed before the reference.
    syntax("\\1");
    syntax("(a\\1)");
    syntax("(a)\\2");
    // The longest run of digits that names a group: \10 is \1 then '0'
    // with one group, group 10 with ten.
    let parsed = |p: &str| parse::parse(p, &Flags::default()).unwrap().node;
    match parsed("(a)\\10") {
        Node::Concat(v) => assert_eq!(v[1], Node::Backref(1)),
        other => panic!("{other:?}"),
    }
    let ten = format!("{}\\10", "(a)".repeat(10));
    match parsed(&ten) {
        Node::Concat(v) => assert_eq!(v[10], Node::Backref(10)),
        other => panic!("{other:?}"),
    }
    // The digits extend to a group whose opening parenthesis precedes the
    // reference, and that group must have closed: \10 inside group 10 is an
    // error, not \1 then '0'.
    syntax("(a)(((((((((\\10)))))))))");
    // No back-reference in a class.
    syntax("(a)[\\1]");
}

#[test]
fn character_classes() {
    for p in [
        "[a-z]",
        "[^a]",
        "[-a]",
        "[a-]",
        "[a-z-[aeiou]]",
        "[a--[b]]",
        "[\\--a]",
        "[a\\-z]",
        "[\\p{L}\\d]",
        "[^\\s]",
        "[a-z-[aeiou-[e]]]",
    ] {
        ok(p, "");
    }
    for p in [
        "[]",
        "[^]",
        "[",
        "[a",
        "[--z]",
        "[a-\\d]",
        "[z-a]",
        "[[a]]",
        "[a-[b]",
        "[a-z-[b]c]",
        "[-[a]]",
    ] {
        syntax(p);
    }
    // XSD 1.1 §G.4.1: a subtraction removes a class from the group before
    // it, negated first when the group is negative.
    let v = set("[a-z-[aeiou]]", "");
    assert!(has(&v, 'b') && !has(&v, 'a'));
    let v = set("[a-z-[aeiou-[e]]]", "");
    assert!(has(&v, 'e') && !has(&v, 'o'));
    let v = set("[^a-z-[AEIOU]]", "");
    assert!(has(&v, 'B') && !has(&v, 'E') && !has(&v, 'b'));
    // A hyphen before ']' is a character; '--[' keeps the first.
    assert!(has(&set("[a-]", ""), '-'));
    let v = set("[a--[a]]", "");
    assert!(has(&v, '-') && !has(&v, 'a'));
}

#[test]
fn flag_x_removes_whitespace_outside_classes() {
    let same = |a: &str, fa: &str, b: &str| {
        assert_eq!(
            parse::parse(a, &Flags::parse(fa).unwrap()).unwrap().node,
            parse::parse(b, &Flags::default()).unwrap().node,
            "{a:?}"
        )
    };
    same("a b\tc\nd\re", "x", "abcde");
    same("a [b c]", "x", "a[b c]");
    same("\\ s", "x", "\\s");
    same("a { 2 , 3 }", "x", "a{2,3}");
    // Not every space: U+00A0 stays.
    same("a\u{A0}b", "x", "a\u{A0}b");
}

#[test]
fn flag_x_and_escaped_brackets() {
    let same = |a: &str, b: &str| {
        assert_eq!(
            parse::parse(a, &Flags::parse("x").unwrap()).unwrap().node,
            parse::parse(b, &Flags::default()).unwrap().node,
            "{a:?}"
        )
    };
    // An escaped bracket opens or closes no class.
    same("\\[ a\\]", "\\[a\\]");
    // Whitespace in a class is kept, after it removed.
    same("[a] b", "[a]b");
    same("[\\] ] b", "[\\] ]b");
    let class = set("[\\] ]", "x");
    assert_eq!(
        class,
        CharSet::from_ranges(vec![(0x20, 0x20), (0x5D, 0x5D)])
    );
}

#[test]
fn flag_q_reads_every_character_as_itself() {
    let r = ok("a.b[(", "q");
    assert_eq!(r.group_count(), 0);
    assert_eq!(r.min_len(), 5);
    let l = r.program_listing().unwrap();
    for c in ['a', '.', 'b', '[', '('] {
        assert!(l.contains(&format!("[{:X}]", c as u32)), "{l}");
    }
}

#[test]
fn flag_i_adds_case_variants() {
    let k = set("k", "i");
    assert!(has(&k, 'K') && has(&k, '\u{212A}'));
    let r = set("[A-Z]", "i");
    assert!(has(&r, 'a') && has(&r, '\u{17F}'));
    // An escaped character and a class's characters too, but not a class
    // escape.
    assert!(has(&set("\\.", "i"), '.'));
    assert!(has(&set("[k]", "i"), '\u{212A}'));
    // §5.6.2: other constructs are unaffected, inside a class too.
    assert!(!has(&set("\\p{Lu}", "i"), 'a'));
    assert!(!has(&set("[\\p{Lu}]", "i"), 'a'));
    // With `q` as well: "matches('Mr. B. Obama', 'B. OBAMA', 'iq')".
    let kelvin = 0x212A;
    assert!(set("k", "qi").contains(kelvin));
}

#[test]
fn minimum_length() {
    for (p, n) in [
        ("", 0),
        ("a", 1),
        ("abc", 3),
        ("a|bc", 1),
        ("a*", 0),
        ("a+", 1),
        ("a{3,5}", 3),
        ("(ab){2}", 4),
        ("^$", 0),
        ("(a)\\1", 1),
        ("a{2147483647}", 2147483647),
        // Saturating, through a product and through a sum.
        ("(?:(?:a{4294967295}){4294967295}){2}", u64::MAX),
        (
            "(?:a{4294967295}){4294967295}(?:a{4294967295}){4294967295}",
            u64::MAX,
        ),
    ] {
        assert_eq!(ok(p, "").min_len(), n, "{p}");
    }
}

#[test]
fn the_pattern_length_has_a_ceiling() {
    let n = parse::MAX_PATTERN_LEN;
    limit(&" ".repeat(n + 1), "x", "longer than");
    ok(&" ".repeat(n), "x");
    // In bytes, not characters.
    limit(&format!("[{}]", "\u{E9}".repeat(n / 2)), "", "longer than");
}

#[test]
fn the_nodes_have_a_ceiling() {
    let n = parse::MAX_NODES;
    // A branch and n atoms; then groups, each an atom and a branch; then
    // empty branches.
    limit(&"a".repeat(n), "", "parts");
    ok(&"a".repeat(n - 1), "");
    limit(&"()".repeat(n / 2), "", "parts");
    limit(&"|".repeat(n), "", "parts");
    ok(&"|".repeat(n - 1), "");
    // Each quantifier is a node: a branch, then an atom and a quantifier
    // each.
    limit(&"a?".repeat(n / 2), "", "parts");
    ok(&"a?".repeat(n / 2 - 1), "");
    // With `q`, each character.
    limit(&"a".repeat(n + 1), "q", "parts");
    ok(&"a".repeat(n), "q");
}

#[test]
fn the_sets_have_a_ceiling() {
    // Each over the ceiling, and a fifth of it under.
    let shapes = |n: usize| -> [(String, &str); 4] {
        [
            ("\\w".repeat(6000 / n), ""),
            // Its parts are under the ceiling; with each class's merged
            // set they are over it.
            ("[\\w]".repeat(3000 / n), ""),
            (format!("[{}]", "\\p{L}".repeat(7000 / n)), ""),
            // The case variants flag `i` visits count.
            (format!("[{}]", "A-\u{FF5A}".repeat(2000 / n)), "i"),
        ]
    };
    for (p, f) in shapes(1) {
        limit(&p, f, "character sets exceed");
    }
    for (p, f) in shapes(5) {
        ok(&p, f);
    }
    // A set's overhead counts: each `[a]` builds two sets of one range.
    let n = parse::MAX_NODES - 2;
    limit(&"[a]".repeat(n), "", "character sets exceed");
    ok(&"[a]".repeat(n / 3), "");
}

#[test]
fn nesting_has_a_ceiling() {
    let d = parse::MAX_DEPTH;
    ok(&format!("{}a{}", "(".repeat(d), ")".repeat(d)), "");
    limit(
        &format!("{}a{}", "(".repeat(d + 1), ")".repeat(d + 1)),
        "",
        "nests deeper",
    );
    ok(
        &format!("[{}b{}]", "a-[".repeat(d - 1), "]".repeat(d - 1)),
        "",
    );
    limit(
        &format!("[{}b{}]", "a-[".repeat(d), "]".repeat(d)),
        "",
        "nests deeper",
    );
    // Leaving a subtraction leaves its level.
    ok(&"[a-[b]]".repeat(3 * d), "");
    // Refused, not overflowed, however deep.
    limit(
        &format!("{}a{}", "(".repeat(100_000), ")".repeat(100_000)),
        "",
        "nests deeper",
    );
}

/// Parsing, the minimum length, compiling and dropping, each recursive,
/// at the nesting ceiling on a 2 MiB stack, in the build the gate runs
/// (debug).
#[test]
fn the_nesting_ceiling_fits_a_small_stack() {
    let d = parse::MAX_DEPTH;
    let shapes = [
        format!("{}a{}", "(?:".repeat(d), ")".repeat(d)),
        format!("{}a{}", "(b?".repeat(d), ")?".repeat(d)),
        format!("{}a{}", "(?:b?".repeat(d), ")*".repeat(d)),
        format!("[{}b{}]", "a-[".repeat(d - 1), "]".repeat(d - 1)),
    ];
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            for p in &shapes {
                let r = Regex::new(p, "").unwrap();
                assert!(r.is_compiled());
                drop(r);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn a_program_over_its_ceilings_is_kept_uncompiled() {
    // Instructions: two saves and a match around the copies.
    let fits = compile::MAX_INSTS - 3;
    assert!(ok(&format!("a{{{fits}}}"), "").is_compiled());
    let r = ok(&format!("a{{{}}}", fits + 1), "");
    assert!(!r.is_compiled());
    assert_eq!(r.min_len(), fits as u64 + 1);
    // Nodes compiled: 3,000 copies of 6,002 visits are over the ceiling,
    // 1,000 of 2,002 under it.
    assert!(!ok("(?:(?:){3000}){3000}", "").is_compiled());
    assert!(ok("(?:(?:){1000}){1000}", "").is_compiled());
    // Nodes compiled: copies of nothing emit no instruction.
    let r = ok("(?:(?:(?:(?:){1000}){1000}){1000}){1000}x", "");
    assert!(!r.is_compiled());
    assert_eq!(r.min_len(), 1);
}

#[test]
fn a_repetitions_body_is_walked_once() {
    // Not once per copy of the enclosing repetition (10^10 node visits).
    let p = format!("(?:(?:{}){{0}}){{1000000}}x", "a".repeat(10_000));
    let t = std::time::Instant::now();
    assert!(ok(&p, "").is_compiled());
    assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    // Optional copies of a body that emits nothing stop at the first, short
    // of the visits ceiling.
    assert!(ok("(?:){0,2147483647}x", "").is_compiled());
}

#[test]
fn counted_copies_share_their_set() {
    let r = ok("\\p{L}{0,300000}x", "");
    let program = r.program.as_ref().unwrap();
    assert!(program.insts.len() > 600_000);
    assert_eq!(program.sets.len(), 2);
}

#[test]
fn sibling_loops_share_a_register() {
    let header = |p: &str| listing(p).lines().next().unwrap().to_string();
    assert_eq!(header(&"(?:a?)*".repeat(1000)), "slots 2 registers 1");
    assert_eq!(header("(?:(?:(?:a?)*b?)*c?)*"), "slots 2 registers 3");
}

/// The program a pattern compiles to, line by line after its header.
fn lines(p: &str) -> Vec<String> {
    listing(p).lines().map(str::to_string).collect()
}

#[test]
fn programs() {
    let a = ["slots 2 registers 0", "set 0 = [61]"];
    let expect = |p: &str, head: &[&str], body: &[&str]| {
        let want: Vec<String> = head.iter().chain(body).map(|l| l.to_string()).collect();
        assert_eq!(lines(p), want, "{p}");
    };
    expect("a", &a, &["0: save 0", "1: set 0", "2: save 1", "3: match"]);
    expect(
        "a|b",
        &["slots 2 registers 0", "set 0 = [61]", "set 1 = [62]"],
        &[
            "0: save 0",
            "1: split 2 4",
            "2: set 0",
            "3: jmp 5",
            "4: set 1",
            "5: save 1",
            "6: match",
        ],
    );
    // Greedy: the body first; reluctant: the exit first.
    expect(
        "a*",
        &a,
        &[
            "0: save 0",
            "1: split 2 4",
            "2: set 0",
            "3: jmp 1",
            "4: save 1",
            "5: match",
        ],
    );
    expect(
        "a*?",
        &a,
        &[
            "0: save 0",
            "1: split 4 2",
            "2: set 0",
            "3: jmp 1",
            "4: save 1",
            "5: match",
        ],
    );
    // Counted: the minimum copies, then a split per optional copy, one set.
    expect(
        "a{2,3}",
        &a,
        &[
            "0: save 0",
            "1: set 0",
            "2: set 0",
            "3: split 4 5",
            "4: set 0",
            "5: save 1",
            "6: match",
        ],
    );
    let g = ["slots 4 registers 0", "set 0 = [61]"];
    // A quantifier that takes no iteration unsets the groups inside it.
    expect(
        "(a)?",
        &g,
        &[
            "0: save 0",
            "1: split 2 6",
            "2: save 2",
            "3: set 0",
            "4: save 3",
            "5: jmp 7",
            "6: clear 1 1",
            "7: save 1",
            "8: match",
        ],
    );
    // The first split enters the body or takes the clearing exit; the loop's
    // own split never clears.
    expect(
        "(a)*",
        &g,
        &[
            "0: save 0",
            "1: split 3 7",
            "2: split 3 8",
            "3: save 2",
            "4: set 0",
            "5: save 3",
            "6: jmp 2",
            "7: clear 1 1",
            "8: save 1",
            "9: match",
        ],
    );
    // Only the first optional copy's exit clears; the later ones keep the
    // groups an earlier copy set.
    expect(
        "(a){0,3}",
        &g,
        &[
            "0: save 0",
            "1: split 2 14",
            "2: save 2",
            "3: set 0",
            "4: save 3",
            "5: split 6 15",
            "6: save 2",
            "7: set 0",
            "8: save 3",
            "9: split 10 15",
            "10: save 2",
            "11: set 0",
            "12: save 3",
            "13: jmp 15",
            "14: clear 1 1",
            "15: save 1",
            "16: match",
        ],
    );
    // A quantifier that must iterate does not clear.
    expect(
        "(a){1,2}",
        &g,
        &[
            "0: save 0",
            "1: save 2",
            "2: set 0",
            "3: save 3",
            "4: split 5 8",
            "5: save 2",
            "6: set 0",
            "7: save 3",
            "8: save 1",
            "9: match",
        ],
    );
    // Every group inside is cleared.
    expect(
        "(?:(a)|(b))?",
        &["slots 6 registers 0", "set 0 = [61]", "set 1 = [62]"],
        &[
            "0: save 0",
            "1: split 2 11",
            "2: split 3 7",
            "3: save 2",
            "4: set 0",
            "5: save 3",
            "6: jmp 10",
            "7: save 4",
            "8: set 1",
            "9: save 5",
            "10: jmp 12",
            "11: clear 1 2",
            "12: save 1",
            "13: match",
        ],
    );
    // A body that can match the empty string: an iteration that does ends
    // the loop, past the clearing exit, which only no iteration takes.
    expect(
        "(?:)*",
        &["slots 2 registers 1"],
        &[
            "0: save 0",
            "1: split 2 5",
            "2: progress-start 0",
            "3: progress-check 0 5",
            "4: jmp 1",
            "5: save 1",
            "6: match",
        ],
    );
    expect(
        "(a?)*",
        &["slots 4 registers 1", "set 0 = [61]"],
        &[
            "0: save 0",
            "1: split 3 10",
            "2: split 3 11",
            "3: progress-start 0",
            "4: save 2",
            "5: split 6 7",
            "6: set 0",
            "7: save 3",
            "8: progress-check 0 11",
            "9: jmp 2",
            "10: clear 1 1",
            "11: save 1",
            "12: match",
        ],
    );
    // So does each optional copy of a counted repetition but the last,
    // sharing one register, its check leaving past the clearing exit:
    // `X{0,n}` then matches nothing `X*` does not (with a back-reference
    // the check decides the match: `^(?:(a|)|\1b){0,4}$` on `ab`, as `*`).
    // The last copy needs none, the repetition ending after it anyway.
    let a_opt = ["slots 4 registers 1", "set 0 = [61]"];
    expect(
        "(a?){0,3}",
        &a_opt,
        &[
            "0: save 0",
            "1: split 2 21",
            "2: progress-start 0",
            "3: save 2",
            "4: split 5 6",
            "5: set 0",
            "6: save 3",
            "7: progress-check 0 22",
            "8: split 9 22",
            "9: progress-start 0",
            "10: save 2",
            "11: split 12 13",
            "12: set 0",
            "13: save 3",
            "14: progress-check 0 22",
            "15: split 16 22",
            "16: save 2",
            "17: split 18 19",
            "18: set 0",
            "19: save 3",
            "20: jmp 22",
            "21: clear 1 1",
            "22: save 1",
            "23: match",
        ],
    );
    expect(
        "(a?){0,3}?",
        &a_opt,
        &[
            "0: save 0",
            "1: split 21 2",
            "2: progress-start 0",
            "3: save 2",
            "4: split 5 6",
            "5: set 0",
            "6: save 3",
            "7: progress-check 0 22",
            "8: split 22 9",
            "9: progress-start 0",
            "10: save 2",
            "11: split 12 13",
            "12: set 0",
            "13: save 3",
            "14: progress-check 0 22",
            "15: split 22 16",
            "16: save 2",
            "17: split 18 19",
            "18: set 0",
            "19: save 3",
            "20: jmp 22",
            "21: clear 1 1",
            "22: save 1",
            "23: match",
        ],
    );
    // A required copy runs regardless; each check leaves for the end, not
    // the next copy's split it would otherwise reach.
    expect(
        "(a?){1,4}",
        &a_opt,
        &[
            "0: save 0",
            "1: save 2",
            "2: split 3 4",
            "3: set 0",
            "4: save 3",
            "5: split 6 24",
            "6: progress-start 0",
            "7: save 2",
            "8: split 9 10",
            "9: set 0",
            "10: save 3",
            "11: progress-check 0 24",
            "12: split 13 24",
            "13: progress-start 0",
            "14: save 2",
            "15: split 16 17",
            "16: set 0",
            "17: save 3",
            "18: progress-check 0 24",
            "19: split 20 24",
            "20: save 2",
            "21: split 22 23",
            "22: set 0",
            "23: save 3",
            "24: save 1",
            "25: match",
        ],
    );
    // One optional copy, the last: no check, no register.
    assert!(listing("(a?)?").starts_with("slots 4 registers 0\n"));
    // Nested: a repetition inside a checked copy takes the next register,
    // one inside the last copy the copies' own, no longer live; the count
    // covers the deepest.
    let nested = lines("(?:(?:a?)*){0,3}");
    assert_eq!(nested[0], "slots 2 registers 2");
    for l in [
        "2: progress-start 0",
        "4: progress-start 1",
        "11: progress-start 0",
        "13: progress-start 1",
        "21: progress-start 0",
    ] {
        assert!(nested.iter().any(|x| x == l), "{l}: {nested:?}");
    }
    let nested = lines("(?:(?:a?){0,2}){0,2}");
    for l in [
        "2: progress-start 0",
        "4: progress-start 1",
        "14: progress-start 0",
    ] {
        assert!(nested.iter().any(|x| x == l), "{l}: {nested:?}");
    }
    expect(
        "(a)\\1",
        &["slots 4 registers 0 back-references", "set 0 = [61]"],
        &[
            "0: save 0",
            "1: save 2",
            "2: set 0",
            "3: save 3",
            "4: backref 1",
            "5: save 1",
            "6: match",
        ],
    );
    expect(
        "^$",
        &["slots 2 registers 0"],
        &["0: save 0", "1: bol", "2: eol", "3: save 1", "4: match"],
    );
    // Nothing for a body that never runs, nor a split for an optional body
    // that emits nothing.
    let nothing = ["0: save 0", "1: save 1", "2: match"];
    expect("(a){0}", &["slots 4 registers 0"], &nothing);
    expect("(a){0,0}", &["slots 4 registers 0"], &nothing);
    expect("(?:)?", &["slots 2 registers 0"], &nothing);
    expect("(?:){0,3}", &["slots 2 registers 0"], &nothing);
    expect("(?:(?:)?){5}", &["slots 2 registers 0"], &nothing);
    // The groups a body clears: every one of a sequence, and those of a
    // repetition inside it.
    assert!(listing("(?:(a)(b))?").contains("\n9: clear 1 2\n"));
    assert!(listing("(?:(a)+)?").contains(": clear 1 1\n"));
}

#[test]
fn group_parents() {
    let parsed = parse::parse("(a(b)(c(d)))(e)", &Flags::default()).unwrap();
    assert_eq!(parsed.group_parent, vec![0, 0, 1, 1, 3, 0]);
}
