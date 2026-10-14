//! The check behind the `regex_parse` target: `xee_regex::Regex::new`
//! returns for any pattern under any flags, without a panic and without
//! overflowing the stack, since the nesting ceiling bounds every recursive
//! pass over the parsed pattern.

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
