use std::sync::LazyLock;

use xee_regex::{Budget, Regex};

use xee_xpath_ast::parse_name;

use crate::atomic;
use crate::context;
use crate::error;
use crate::regex_limits::fixed_pattern_steps;

use super::cast::{whitespace_collapse, whitespace_replace};
use super::StringType;

// https://www.w3.org/TR/xml11/#NT-Nmtoken
// 	NameStartChar	   ::=   	":" | [A-Z] | "_" | [a-z] | [#xC0-#xD6] | [#xD8-#xF6] | [#xF8-#x2FF] | [#x370-#x37D] | [#x37F-#x1FFF] | [#x200C-#x200D] | [#x2070-#x218F] | [#x2C00-#x2FEF] | [#x3001-#xD7FF] | [#xF900-#xFDCF] | [#xFDF0-#xFFFD] | [#x10000-#xEFFFF]
// We create the NCName versions without colon (:) so we can do ncnames easily later
static NCNAME_START_CHAR: &str = "A-Z_a-z\u{c0}-\u{d6}\u{d8}-\u{f6}\u{f8}-\u{02ff}\u{0370}-\u{037d}\u{037f}-\u{1fff}\u{200c}\u{200d}\u{2070}-\u{218f}\u{2c00}-\u{2fef}\u{3001}-\u{d7ff}\u{f900}-\u{fdcf}\u{fdf0}-\u{fffd}\u{10000}-\u{effff}";
// static NCNAME_START_CHAR: &str = r"A-Z_a-z\xc0-\xd6\xd8-\xf6\xf8-\u02ff\u0370-\u037d\u037f-\u1fff\u200c\u200d\u2070-\u218f\u2c00-\u2fef\u3001-\ud7ff\uf900-\ufdcf\ufdf0-\ufffd\U00010000-\U000effff";
// NameChar	   ::=   	NameStartChar | "-" | "." | [0-9] | #xB7 | [#x0300-#x036F] | [#x203F-#x2040]
static NCNAME_CHAR_ADDITIONS: &str = "-\\.0-9\u{b7}\u{0300}-\u{036F}\u{203F}-\u{2040}";
// static NCNAME_CHAR_ADDITIONS: &str = r"-\.0-9\xb7\u0300-\u036F\u203F-\u2040";
static LANGUAGE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z]{1,8}(-[a-zA-Z0-9]{1,8})*$", "").expect("Invalid regex")
});
// Nmtoken	 ::= (NameChar)+
static NMTOKEN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // we have to add the colon for NAME_START_CHAR / NAME_CHAR
    Regex::new(
        &format!("^[:{}{}]+$", NCNAME_START_CHAR, NCNAME_CHAR_ADDITIONS),
        "",
    )
    .expect("Invalid regex")
});

/// Whether `s` is, as it stands, a string in the value space of xs:NMTOKEN
/// (XML 1.1 `Nmtoken`). No whitespace is collapsed first. The test runs
/// under the budget of a fixed pattern, which grows with the string, so
/// that it answers whatever the string's length.
pub(crate) fn is_nmtoken(s: &str) -> error::Result<bool> {
    fixed_match(&NMTOKEN_REGEX, s)
}

/// Whether `s` is in the language of `regex`, one of the fixed patterns,
/// under the budget of a fixed pattern for `s`.
fn fixed_match(regex: &Regex, s: &str) -> error::Result<bool> {
    Ok(regex.is_match(s, &mut Budget::new(fixed_pattern_steps(s.len())))?)
}

// Name	   ::=   	NameStartChar (NameChar)*
static NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // we have to add the colon for NAME_START_CHAR / NAME_CHAR
    Regex::new(
        &format!(
            "^[:{}][:{}{}]*$",
            NCNAME_START_CHAR, NCNAME_START_CHAR, NCNAME_CHAR_ADDITIONS
        ),
        "",
    )
    .expect("Invalid regex")
});
// https://www.w3.org/TR/xml-names11/#NT-NCName
// 	NCName	   ::=   	NCNameStartChar NCNameChar*
// 	NCNameChar	   ::=   	NameChar - ':'
//	NCNameStartChar	   ::=   	NameStartChar - ':'
static NC_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        &format!(
            "^[{}][{}{}]*$",
            NCNAME_START_CHAR, NCNAME_START_CHAR, NCNAME_CHAR_ADDITIONS
        ),
        "",
    )
    .expect("Invalid regex")
});

impl atomic::Atomic {
    pub(crate) fn cast_to_string(self) -> atomic::Atomic {
        atomic::Atomic::String(atomic::StringType::String, self.into_canonical().into())
    }

    pub(crate) fn cast_to_untyped_atomic(self) -> atomic::Atomic {
        atomic::Atomic::Untyped(self.into_canonical().into())
    }

    pub(crate) fn cast_to_any_uri(self) -> error::Result<atomic::Atomic> {
        // https://www.w3.org/TR/xpath-functions-31/#casting-to-anyuri
        match self {
            atomic::Atomic::String(_, s) => Ok(atomic::Atomic::String(
                StringType::AnyURI,
                whitespace_collapse(&s).into(),
            )),
            atomic::Atomic::Untyped(s) => Ok(atomic::Atomic::String(StringType::AnyURI, s.clone())),
            _ => Err(error::Error::XPTY0004),
        }
    }

    pub(crate) fn cast_to_normalized_string(self) -> atomic::Atomic {
        let s = whitespace_replace(&self.into_canonical());
        atomic::Atomic::String(atomic::StringType::NormalizedString, s.into())
    }

    pub(crate) fn cast_to_token(self) -> atomic::Atomic {
        let s = whitespace_collapse(&self.into_canonical());
        atomic::Atomic::String(atomic::StringType::Token, s.into())
    }

    fn cast_to_regex(
        self,
        string_type: atomic::StringType,
        regex: &LazyLock<Regex>,
    ) -> error::Result<atomic::Atomic> {
        let s = whitespace_collapse(&self.into_canonical());
        // The budget of a fixed pattern, which grows with the value, so that
        // the cast answers whatever the value's length.
        if fixed_match(regex, &s)? {
            Ok(atomic::Atomic::String(string_type, s.into()))
        } else {
            Err(error::Error::FORG0001)
        }
    }

    pub(crate) fn cast_to_language(self) -> error::Result<atomic::Atomic> {
        self.cast_to_regex(atomic::StringType::Language, &LANGUAGE_REGEX)
    }

    pub(crate) fn cast_to_nmtoken(self) -> error::Result<atomic::Atomic> {
        self.cast_to_regex(atomic::StringType::NMTOKEN, &NMTOKEN_REGEX)
    }

    pub(crate) fn cast_to_name(self) -> error::Result<atomic::Atomic> {
        self.cast_to_regex(atomic::StringType::Name, &NAME_REGEX)
    }

    fn cast_to_ncname_helper(
        self,
        string_type: atomic::StringType,
    ) -> error::Result<atomic::Atomic> {
        self.cast_to_regex(string_type, &NC_NAME_REGEX)
    }

    pub(crate) fn cast_to_ncname(self) -> error::Result<atomic::Atomic> {
        self.cast_to_ncname_helper(atomic::StringType::NCName)
    }

    pub(crate) fn cast_to_id(self) -> error::Result<atomic::Atomic> {
        self.cast_to_ncname_helper(atomic::StringType::ID)
    }

    pub(crate) fn cast_to_idref(self) -> error::Result<atomic::Atomic> {
        self.cast_to_ncname_helper(atomic::StringType::IDREF)
    }

    pub(crate) fn cast_to_entity(self) -> error::Result<atomic::Atomic> {
        // https://www.w3.org/TR/xpath-functions-31/#casting-to-ENTITY
        // we don't need to check whether it matches unparsed entities
        self.cast_to_ncname_helper(atomic::StringType::ENTITY)
    }

    pub(crate) fn cast_to_qname(
        self,
        static_context: &context::StaticContext,
    ) -> error::Result<atomic::Atomic> {
        match self {
            atomic::Atomic::QName(_) => Ok(self.clone()),
            atomic::Atomic::String(_, s) | atomic::Atomic::Untyped(s) => {
                // https://www.w3.org/TR/xpath-functions-31/#constructor-qname-notation
                let namespaces = static_context.namespaces();
                let name = parse_name(&s, namespaces);
                match name {
                    Ok(name) => {
                        let name = name.value;
                        if name.in_default_namespace() {
                            // we deliberately do not parse Qualified names, as they are not
                            // legal for xs:QName
                            Err(error::Error::FORG0001)
                        } else {
                            Ok(atomic::Atomic::QName(
                                name.with_default_namespace(namespaces.default_element_namespace())
                                    .into(),
                            ))
                        }
                    }
                    // TODO: We really want to distinguish between parse errors
                    // and namespace lookup errors, which should be a FONS0004 error
                    // This requires the parser to be modified so it retains that
                    // information.
                    Err(_) => Err(error::Error::FORG0001),
                }
            }
            _ => Err(error::Error::XPTY0004),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_match(regex: &Regex, s: &str) -> bool {
        fixed_match(regex, s).unwrap()
    }

    /// A small deterministic generator (xorshift), so that a failing case
    /// can be replayed from its seed.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    /// The steps a test of `s` against `regex` takes, unbounded.
    fn steps(regex: &Regex, s: &str) -> u64 {
        let mut budget = Budget::new(u64::MAX);
        regex.is_match(s, &mut budget).unwrap();
        budget.spent()
    }

    #[test]
    fn the_fixed_patterns_compile() {
        for regex in [&LANGUAGE_REGEX, &NMTOKEN_REGEX, &NAME_REGEX, &NC_NAME_REGEX] {
            assert!(LazyLock::force(regex).is_compiled());
        }
    }

    // The name-character additions start with a hyphen right after the
    // range up to U+EFFFF. A part of a character group that starts with a
    // single character not followed by a hyphen is that character (XSD 1.1
    // Part 2 §G.4.1), so the hyphen is a name character of its own and
    // ends no range.
    #[test]
    fn the_hyphen_after_a_range_is_a_name_character() {
        assert!(is_match(&NMTOKEN_REGEX, "-"));
        assert!(is_match(&NMTOKEN_REGEX, "a-.\u{B7}9:"));
        assert!(!is_match(&NMTOKEN_REGEX, ","));
        assert!(is_match(&NC_NAME_REGEX, "a-b"));
        assert!(!is_match(&NC_NAME_REGEX, "-a"));
        assert!(!is_match(&NC_NAME_REGEX, "a:b"));
        assert!(is_match(&NAME_REGEX, ":a-b"));
        assert!(is_match(&NAME_REGEX, "a\u{EFFFF}"));
        assert!(!is_match(&NAME_REGEX, "a\u{F0000}"));
        assert!(is_match(&LANGUAGE_REGEX, "en-GB-1"));
        assert!(!is_match(&LANGUAGE_REGEX, "en-"));
        assert!(!is_match(&LANGUAGE_REGEX, "abcdefghi"));
    }

    // Each fixed pattern's test takes at most half its budget, on the
    // shapes that cost the most a byte (a name of one, two, three or
    // four-byte characters, runs of name characters and hyphens, language
    // subtags of each length, short and, past where the budget's base
    // stops counting, 160 KB long) and on random values over name
    // characters, others, a space and a hyphen, so that the budget's
    // margin is pinned: a change to the engine that made a test dearer
    // fails here before a cast could meet the budget.
    #[test]
    fn the_fixed_patterns_stay_within_half_their_budget() {
        let mut shapes = Vec::new();
        for c in [
            "a",
            "\u{E9}",
            "\u{4E2D}",
            "\u{10348}",
            "9",
            "-",
            ".",
            "\u{300}",
            ":",
        ] {
            for n in [1, 2, 7, 8, 9, 100, 5_000] {
                shapes.push(c.repeat(n));
                shapes.push(format!("a{}", c.repeat(n)));
            }
        }
        for n in 1..=9 {
            shapes.push(format!("{}a", format!("{}-", "a".repeat(n)).repeat(500)));
            shapes.push(format!("a{}", format!("-{}", "9".repeat(n)).repeat(500)));
        }
        // Seven-character subtags, the dearest a byte, at 16.25 steps.
        for c in ["a", "9"] {
            shapes.push(format!("a{}", format!("-{}", c.repeat(7)).repeat(20_000)));
        }
        let alphabet: Vec<char> =
            "aZ_:-.9\u{B7}\u{E9}\u{300}\u{37F}\u{203F}\u{4E2D}\u{10348}\u{EFFFF} \u{D7}\u{2000}x0-"
                .chars()
                .collect();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..2_000 {
            let len = 1 + rng.below(300);
            shapes.push(
                (0..len)
                    .map(|_| alphabet[rng.below(alphabet.len())])
                    .collect(),
            );
        }
        for regex in [&LANGUAGE_REGEX, &NMTOKEN_REGEX, &NAME_REGEX, &NC_NAME_REGEX] {
            for s in &shapes {
                let (spent, budget) = (steps(regex, s), fixed_pattern_steps(s.len()));
                assert!(
                    2 * spent <= budget,
                    "{spent} of {budget} on {} bytes",
                    s.len()
                );
            }
        }
    }

    // The budget of a fixed pattern's test is the one for the value's
    // length, 40 steps a byte and 4,096 more: on a pattern dearer than a
    // fixed one, eight `.*` (about 66 steps a byte), the test answers on
    // every length whose cost fits that budget and is refused on every
    // other, across the length where the cost crosses it, wherever a
    // change to the engine's charges moves that length.
    #[test]
    fn the_fixed_pattern_budget_is_the_value_s() {
        let dear = Regex::new("^.*.*.*.*.*.*.*.*$", "").unwrap();
        let (mut answered, mut refused) = (0, 0);
        for n in 1..=400 {
            let s = "a".repeat(n);
            let fits = steps(&dear, &s) <= 40 * n as u64 + 4_096;
            match fixed_match(&dear, &s) {
                Ok(true) => {
                    assert!(fits, "{n} bytes answered over the budget");
                    answered += 1;
                }
                Err(error::Error::XPDY0130) => {
                    assert!(!fits, "{n} bytes refused within the budget");
                    refused += 1;
                }
                other => panic!("{n} bytes: {other:?}"),
            }
        }
        // Both sides of the crossing were tried.
        assert!(answered > 0 && refused > 0, "{answered} {refused}");
    }

    // A value as long as one a call of `fn:matches` could not test, four
    // million characters, is cast and rejected as a shorter one is.
    #[test]
    fn a_long_value_is_cast_or_rejected() {
        let long = "a".repeat(4_000_000);
        assert_eq!(is_nmtoken(&long), Ok(true));
        assert_eq!(is_nmtoken(&format!("{long} ")), Ok(false));
        assert!(atomic::Atomic::from(long.as_str())
            .cast_to_nmtoken()
            .is_ok());
        assert_eq!(
            atomic::Atomic::from(format!("{long},").as_str()).cast_to_nmtoken(),
            Err(error::Error::FORG0001)
        );
        assert_eq!(
            atomic::Atomic::from("a b").cast_to_nmtoken(),
            Err(error::Error::FORG0001)
        );
    }
}
