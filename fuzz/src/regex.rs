//! The checks behind the `regex_parse` and `regex_match` targets:
//! `xee_regex::Regex::new` returns for any pattern under any flags, without
//! a panic and without overflowing the stack, since the nesting ceiling
//! bounds every recursive pass over the parsed pattern; and the PikeVM and
//! the backtracker give the same slots for every match.

/// The flags a selector byte chooses: bit k for the k-th of `smixq`.
pub fn flags_from_selector(selector: u8) -> String {
    "smixq"
        .chars()
        .enumerate()
        .filter(|(k, _)| selector & (1 << k) != 0)
        .map(|(_, c)| c)
        .collect()
}

/// Compiles the input after its first byte, a UTF-8 pattern, under the
/// flags the first byte selects ([`flags_from_selector`]). Input that is
/// not UTF-8 is skipped: a pattern is an `xs:string`.
pub fn regex_parse(data: &[u8]) {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let Ok(pattern) = std::str::from_utf8(rest) else {
        return;
    };
    let flags = flags_from_selector(selector);
    match xee_regex::Regex::new(pattern, &flags) {
        Ok(r) => {
            // A group opens at a '(' of the pattern, and none with `q`.
            assert!(r.group_count() <= pattern.matches('(').count());
            assert!(!flags.contains('q') || r.group_count() == 0);
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
    let Ok(mut r) = xee_regex::Regex::new(pattern, &flags_from_selector(selector)) else {
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
