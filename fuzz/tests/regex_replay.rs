//! Replays the `regex_parse` check on stable over hand-written seeds, each
//! under every combination of flags: the shapes the parser's ceilings
//! exist for, and the grammar's corners.

use xee_json_fuzz::regex::{compile, flags_from_selector, regex_match, regex_parse, XSD};

#[test]
fn the_selector_covers_every_flag() {
    assert_eq!(flags_from_selector(0), "");
    assert_eq!(flags_from_selector(0b11111), "smixq");
    assert_eq!(flags_from_selector(0b100_10010), "mq");
}

#[test]
fn bit_5_selects_the_xsd_dialect() {
    let budget = || xee_regex::Budget::new(1000);
    // `^` is an anchor in XPath, a character in XSD, whatever bits 0 to 4
    // say.
    assert!(compile(0, "^")
        .unwrap()
        .is_match("x", &mut budget())
        .unwrap());
    for low in 0..32u8 {
        let r = compile(XSD | low, "^").unwrap();
        assert!(r.is_match("^", &mut budget()).unwrap());
        assert!(!r.is_match("x", &mut budget()).unwrap());
    }
    // The selectors the seed tests below replay, 0 to 63: those from 32,
    // and only those, are the XSD dialect's, so that its seeds run in it
    // whatever the constant says. Under any flags an XPath pattern matches
    // a part of the input, where an XSD one matches the whole.
    for selector in 0..64u8 {
        let r = compile(selector, "a").unwrap();
        assert_eq!(
            r.is_match("ba", &mut budget()).unwrap(),
            selector < 32,
            "selector {selector}"
        );
    }
    assert!(compile(XSD, "\\p{IsNoSuchBlock}").is_ok());
    assert!(compile(0, "\\p{IsNoSuchBlock}").is_err());
}

#[test]
fn regex_parse_seeds() {
    let seeds: Vec<String> = vec![
        String::new(),
        "(".repeat(100_000),
        format!("{}a{}", "(".repeat(100_000), ")".repeat(100_000)),
        "[a-[".repeat(10_000),
        "(?:a?)*".repeat(1000),
        "a{2147483647}".into(),
        "a{99999999999999999999999999,}".into(),
        "(?:(?:(?:(?:){1000}){1000}){1000}){1000}x".into(),
        "\\p{L}{0,300000}x".into(),
        format!("[{}]", "A-\u{FF5A}".repeat(200)),
        "\\w".repeat(1000),
        "|".repeat(10_000),
        "(a)\\1\\10\\2".into(),
        "[^]".into(),
        "[--z]".into(),
        "\\p{IsGreek}\\P{Lu}\\p{Is}".into(),
        " \t\n\r [ ] \\ s".into(),
        "\u{10FFFF}\u{0}\u{FFFF}".into(),
        // The XSD dialect's corners (selectors from 32): anchors as
        // characters, F&O 3.1's additions refused, unknown block names
        // every character, many of them, and the nesting ceiling.
        "^a$|$^*".into(),
        "a??(?:b)\\1\\$".into(),
        "\\p{IsNoSuchBlock}\\P{IsaA0-a9}[^\\p{IsX}][a-[\\P{IsX}]]".into(),
        (0..2000).map(|k| format!("\\p{{Is0x{k}}}")).collect(),
        "\\p{IsX}".repeat(50_000),
        format!("{}a{}", "(".repeat(100), ")".repeat(100)),
        format!("{}a{}", "(".repeat(101), ")".repeat(101)),
    ];
    // Bits 0 to 4 the flags, bit 5 the XSD dialect.
    for seed in &seeds {
        for selector in 0..64u8 {
            let mut data = vec![selector];
            data.extend_from_slice(seed.as_bytes());
            regex_parse(&data);
        }
    }
    // Not UTF-8, and empty: skipped.
    regex_parse(&[0, 0xFF, 0xFE]);
    regex_parse(&[]);
}

#[test]
fn regex_match_seeds() {
    // The shapes that shaped the PikeVM's closure, under every flag and in
    // both dialects; then shapes of the XSD dialect.
    let seeds: [(&str, &str); 10] = [
        ("(\\s*|)*a.{0,2}", "B aaaa"),
        ("((a?)*)*", "aab"),
        ("(?:(?:(?:(?:(a?)*)*)*)*)*", "aaa"),
        ("(^|a)*b", "b\nab"),
        ("(?:x(a)?)+", "xax"),
        ("(a|b)*c", "ababxc"),
        ("a|b", "ab"),
        ("(a*)(b|)", "aab"),
        ("(^|$)*\\p{IsX}", "^$\n"),
        ("((a|)|b)*", "abab"),
    ];
    for (pattern, hay) in seeds {
        for selector in 0..64u8 {
            let mut data = vec![selector];
            data.extend_from_slice(pattern.as_bytes());
            data.push(0);
            data.extend_from_slice(hay.as_bytes());
            regex_match(&data);
        }
    }
}
