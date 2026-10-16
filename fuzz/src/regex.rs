//! The checks behind the `regex_parse` and `regex_match` targets:
//! `xee_regex::Regex::new` returns for any pattern under any flags, and
//! `xee_regex::Regex::xsd` for any pattern, without a panic and without
//! overflowing the stack, since the nesting ceiling bounds every recursive
//! pass over the parsed pattern; and the PikeVM and the backtracker give
//! the same slots for every match.

/// The flags a selector byte chooses: bit k for the k-th of `smixq`.
pub fn flags_from_selector(selector: u8) -> String {
    "smixq"
        .chars()
        .enumerate()
        .filter(|(k, _)| selector & (1 << k) != 0)
        .map(|(_, c)| c)
        .collect()
}

/// Bit 5 of a selector byte chooses the XSD dialect, which has no flags,
/// so that bits 0 to 4 then choose nothing.
pub const XSD: u8 = 1 << 5;

/// Compiles `pattern` in the dialect the selector chooses: the XSD
/// dialect, or the XPath dialect under the flags it chooses.
pub fn compile(selector: u8, pattern: &str) -> Result<xee_regex::Regex, xee_regex::Error> {
    if selector & XSD != 0 {
        xee_regex::Regex::xsd(pattern)
    } else {
        xee_regex::Regex::new(pattern, &flags_from_selector(selector))
    }
}

/// Compiles the input after its first byte, a UTF-8 pattern, in the
/// dialect and under the flags the first byte selects ([`compile`]). Input
/// that is not UTF-8 is skipped: a pattern is an `xs:string`.
pub fn regex_parse(data: &[u8]) {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let Ok(pattern) = std::str::from_utf8(rest) else {
        return;
    };
    let xsd = selector & XSD != 0;
    let flags = if xsd {
        String::new()
    } else {
        flags_from_selector(selector)
    };
    match compile(selector, pattern) {
        Ok(r) => {
            // A group opens at a '(' of the pattern, and none with `q`.
            assert!(r.group_count() <= pattern.matches('(').count());
            assert!(!flags.contains('q') || r.group_count() == 0);
            // Unknown block names only in the XSD dialect, each once, in
            // order, and each written in the pattern.
            let names = r.unknown_blocks();
            assert!(xsd || names.is_empty());
            assert!(names.windows(2).all(|w| w[0] < w[1]));
            for name in names {
                assert!(pattern.contains(&format!("{{Is{name}}}")), "{name:?}");
            }
            // Anchored at both ends, an XSD pattern matches the empty
            // input exactly when its minimum length is 0.
            if xsd && r.is_compiled() {
                if let Ok(m) = r.is_match("", &mut xee_regex::Budget::new(STEPS)) {
                    assert_eq!(m, r.min_len() == 0, "{pattern:?}");
                }
            }
        }
        // Only the flags this harness never writes are FORX0001.
        Err(e) => assert!(
            !matches!(e, xee_regex::Error::InvalidFlags(_)),
            "{flags:?}: {e}"
        ),
    }
}

/// The steps and bytes one matcher may take on one input; a call refused
/// for want of them is skipped.
const STEPS: u64 = 200_000;
const BYTES: usize = 1 << 22;

/// Compiles a pattern as `regex_parse` does, from the input up to its
/// first zero byte after the selector, and matches the rest, a UTF-8
/// input, on both matchers: the slots of every successive match agree,
/// and so does `fn:matches`.
pub fn regex_match(data: &[u8]) {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let mut parts = rest.splitn(2, |&b| b == 0);
    let (Some(pattern), Some(hay)) = (parts.next(), parts.next()) else {
        return;
    };
    let (Ok(pattern), Ok(hay)) = (std::str::from_utf8(pattern), std::str::from_utf8(hay)) else {
        return;
    };
    let Ok(mut r) = compile(selector, pattern) else {
        return;
    };
    let budget = || xee_regex::Budget::with_memory(STEPS, BYTES);
    let pike = (
        r.match_slots(hay, &mut budget()),
        r.is_match(hay, &mut budget()),
    );
    r.set_matcher(xee_regex::Matcher::Backtrack);
    let back = (
        r.match_slots(hay, &mut budget()),
        r.is_match(hay, &mut budget()),
    );
    match (pike, back) {
        ((Ok(a), Ok(x)), (Ok(b), Ok(y))) => {
            assert_eq!(a, b, "{pattern:?} on {hay:?}");
            assert_eq!(x, y, "{pattern:?} on {hay:?}");
            // An XSD pattern matches the whole input or nothing.
            if selector & XSD != 0 {
                assert!(a.len() <= 1, "{pattern:?} on {hay:?}");
                if let Some(slots) = a.first() {
                    assert_eq!((slots[0], slots[1]), (Some(0), Some(hay.len())));
                }
            }
        }
        (p, b) => {
            for e in [p.0.err(), p.1.err(), b.0.err(), b.1.err()]
                .into_iter()
                .flatten()
            {
                assert!(matches!(e, xee_regex::Error::Limit(_)), "{pattern:?}: {e}");
            }
        }
    }
}
