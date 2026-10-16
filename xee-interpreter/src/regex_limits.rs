//! What one call of the regular-expression engine (`xee-regex`) may spend.

/// Ceiling: the steps one call may take, a call being one of `fn:matches`,
/// `fn:replace`, `fn:tokenize` or `fn:analyze-string` (F&O 3.1 §5.6). A
/// step is the engine's unit of work (`xee_regex::Budget`): an instruction
/// visited, a position searched, a byte of the result built. A call that
/// would take more is refused with XPDY0130, an implementation limit. Each
/// call has a budget of its own, so that one cannot spend another's, and
/// the memory a call holds has the engine's own ceiling, 2^27 bytes, which
/// `Budget::new` sets.
pub(crate) const STEPS_PER_CALL: u64 = 50_000_000;

/// The steps a test of a value of `bytes` bytes against one of the fixed
/// patterns may take: the patterns of a cast to `xs:language`,
/// `xs:NMTOKEN`, `xs:Name`, `xs:NCName` and the types derived from it, and
/// the NMTOKEN test of `fn:serialize`'s `normalization-form`. Those
/// patterns are this crate's own, anchored and linear: a test reads the
/// value once, at most 13 steps a byte for the name types and 16.25 for
/// `xs:language` (subtags of seven characters), and a few hundred more for
/// its set-up. Their budget grows with the value, at more than twice that
/// (40 steps a byte, 2.46 times `xs:language`'s dearest), so that a cast
/// answers whatever the value's length, and `castable as` with it, as a function that reads a
/// string, such as `fn:string-length`, answers whatever its length. A
/// test that took more would be a defect of the engine, refused with
/// XPDY0130 rather than run on; `the_fixed_patterns_stay_within_half_their_budget`
/// fails first.
pub(crate) fn fixed_pattern_steps(bytes: usize) -> u64 {
    (bytes as u64)
        .saturating_mul(FIXED_PATTERN_STEPS_PER_BYTE)
        .saturating_add(FIXED_PATTERN_STEPS_BASE)
}

/// The steps a byte of [`fixed_pattern_steps`].
pub(crate) const FIXED_PATTERN_STEPS_PER_BYTE: u64 = 40;

/// The steps a test of a fixed pattern may take beyond a step a byte's
/// [`FIXED_PATTERN_STEPS_PER_BYTE`].
pub(crate) const FIXED_PATTERN_STEPS_BASE: u64 = 4_096;
