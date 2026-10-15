//! Replays the `regex_parse` check on stable over hand-written seeds, each
//! under every combination of flags: the shapes the parser's ceilings
//! exist for, and the grammar's corners.

use xee_json_fuzz::regex::{flags_from_selector, regex_match, regex_parse};

#[test]
fn the_selector_covers_every_flag() {
    assert_eq!(flags_from_selector(0), "");
    assert_eq!(flags_from_selector(0b11111), "smixq");
    assert_eq!(flags_from_selector(0b100_10010), "mq");
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
    ];
    for seed in &seeds {
        for selector in 0..32u8 {
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
    // The shapes that shaped the PikeVM's closure, under every flag.
    let seeds: [(&str, &str); 6] = [
        ("(\\s*|)*a.{0,2}", "B aaaa"),
        ("((a?)*)*", "aab"),
        ("(?:(?:(?:(?:(a?)*)*)*)*)*", "aaa"),
        ("(^|a)*b", "b\nab"),
        ("(?:x(a)?)+", "xax"),
        ("(a|b)*c", "ababxc"),
    ];
    for (pattern, hay) in seeds {
        for selector in 0..32u8 {
            let mut data = vec![selector];
            data.extend_from_slice(pattern.as_bytes());
            data.push(0);
            data.extend_from_slice(hay.as_bytes());
            regex_match(&data);
        }
    }
}
