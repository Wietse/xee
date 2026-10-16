//! The regex benchmark's workloads: each an XPath expression over one
//! input bound to `$s`, with the answer it must give.
//!
//! Defined once and shared by two includers: `benches/regex.rs`, which
//! times the workloads, and `tests/regex_bench_inputs.rs`, which checks
//! every workload's answer under `cargo test`. A generator, an expression
//! or an answer that breaks therefore fails the test suite, not only a
//! bench run, and a workload is never timed as a fast error.
//!
//! The inputs are synthetic and generated deterministically:
//!
//! - short value checks, where compiling the pattern costs as much as
//!   matching it ([`SHORT_CHECKS`]); each evaluation compiles its pattern,
//!   since the cache of compiled patterns lasts one evaluation, and the
//!   same checks repeated a thousand times in one evaluation compile it
//!   once ([`repeated`]);
//! - text blocks of words, numbers, punctuation, newlines, HTML-like tags
//!   and non-ASCII letters, from about 1 KB to about 1 MB ([`text`],
//!   [`TEXT_SIZES`]), through all four functions ([`TEXT_WORKLOADS`]);
//! - a few shapes whose cost grows with the input faster than its length
//!   on some matchers ([`SHAPES`]), at sizes a backtracking matcher still
//!   answers in well under a second.
//!
//! The answers of the text workloads are computed in Rust from the text,
//! by scans written for these patterns ([`count_tokens`] and the others),
//! not by a regular expression.

#![allow(dead_code)]

use xee_xpath::context::{StaticContextBuilder, Variables};
use xee_xpath::query::SequenceQuery;
use xee_xpath::{Documents, Item, Queries, Query, Sequence};

/// The sizes of the text blocks, in bytes: each block stops growing at the
/// first word that takes it to this length or past it. The largest is
/// within a call's budget of steps for every text workload; the costliest
/// a byte, `analyze_pairs`, is refused with XPDY0130 from about 1.5 MB
/// (`tests/regex_bench_inputs.rs`).
pub const TEXT_SIZES: [usize; 4] = [1_000, 10_000, 100_000, 1_000_000];

/// One workload: `expr` evaluated with `$s` bound to `input`, whose string
/// value must be `answer`.
pub struct Workload {
    pub expr: String,
    pub input: String,
    pub answer: String,
}

/// A workload compiled, ready to be evaluated again and again, each time
/// in an arena of documents the caller gives it ([`Prepared::run`]).
pub struct Prepared {
    query: SequenceQuery,
    variables: Variables,
}

impl Workload {
    pub fn new(expr: &str, input: String, answer: impl ToString) -> Workload {
        Workload {
            expr: expr.to_string(),
            input,
            answer: answer.to_string(),
        }
    }

    /// Compiles the expression; `$s` is its one variable.
    pub fn prepare(&self) -> Prepared {
        let name = xot::xmlname::OwnedName::new("s".to_string(), "".to_string(), "".to_string());
        let item: Item = self.input.as_str().into();
        let variables = Variables::from([(name, Sequence::from(item))]);
        let mut builder = StaticContextBuilder::default();
        builder.variable_names(variables.keys().cloned());
        let static_context = builder.build().unwrap();
        let query = Queries::default()
            .sequence_with_context(&self.expr, static_context)
            .unwrap_or_else(|e| panic!("{}: {e:?}", self.expr));
        Prepared { query, variables }
    }

    /// Evaluates the workload once: the string value of its result, or the
    /// error it raises.
    pub fn evaluate(&self) -> (Prepared, xee_xpath::error::ValueResult<String>) {
        let prepared = self.prepare();
        let mut documents = Documents::new();
        let got = prepared
            .run(&mut documents)
            .map_err(|e| e.error)
            .and_then(|result| result.string_value(documents.xot()));
        (prepared, got)
    }

    /// Evaluates the workload once and panics unless it gives its answer.
    pub fn check(&self) -> Prepared {
        let (prepared, got) = self.evaluate();
        let got = got.unwrap_or_else(|e| panic!("{}: {e:?}", self.expr));
        assert!(
            got == self.answer,
            "{} on {} bytes: expected {:?}, got {:?}",
            self.expr,
            self.input.len(),
            preview(&self.answer),
            preview(&got)
        );
        prepared
    }
}

impl Prepared {
    /// Evaluates the workload once in `documents`, which keeps what the
    /// evaluation builds (the tree `fn:analyze-string` returns) until it is
    /// dropped: a bench gives each evaluation an empty arena of its own, so
    /// that no sample is timed against the trees of the ones before it.
    pub fn run(&self, documents: &mut Documents) -> xee_xpath::error::Result<Sequence> {
        let variables = self.variables.clone();
        self.query.execute_build_context(documents, move |builder| {
            builder.variables(variables);
        })
    }
}

fn preview(s: &str) -> String {
    s.chars().take(80).collect()
}

// ============================================================================
// Short value checks.
// ============================================================================

/// A list of 60 two-letter codes, `AA` to `CH`, as one anchored
/// alternation.
pub fn code_list() -> String {
    let codes: Vec<String> = (0..60u8)
        .map(|i| format!("{}{}", (b'A' + i / 26) as char, (b'A' + i % 26) as char))
        .collect();
    format!("^(?:{})$", codes.join("|"))
}

/// The short checks: a name, the pattern, a value it matches.
pub const SHORT_CHECKS: [(&str, &str, &str); 5] = [
    ("date", r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$", "2026-10-15"),
    // The pattern is `code_list()`; see `short_check`.
    ("code_list", "", "CF"),
    (
        "identifier",
        r"^[A-Z0-9]{18}[0-9]{2}$",
        "ABCDEFGHIJ0123456789",
    ),
    ("prefix", r"^[A-Z]{4}/.*", "ABCD/0001-X"),
    (
        "url",
        r"^https?://([^./]{1,63}\.)+[a-z]{2,}(/.*)?$",
        "https://www.example.org/a/b?c=d",
    ),
];

fn short_pattern(index: usize) -> String {
    match SHORT_CHECKS[index] {
        ("code_list", _, _) => code_list(),
        (_, pattern, _) => pattern.to_string(),
    }
}

/// The short check named `name`, one call of `fn:matches`.
pub fn short_check(name: &str) -> Workload {
    let index = SHORT_CHECKS.iter().position(|c| c.0 == name).unwrap();
    let expr = format!("matches($s, '{}')", short_pattern(index));
    Workload::new(&expr, SHORT_CHECKS[index].2.to_string(), true)
}

/// The short check named `name` made 1,000 times in one evaluation, which
/// compiles its pattern once.
pub fn repeated(name: &str) -> Workload {
    let index = SHORT_CHECKS.iter().position(|c| c.0 == name).unwrap();
    let expr = format!(
        "count((1 to 1000)[matches($s, '{}')])",
        short_pattern(index)
    );
    Workload::new(&expr, SHORT_CHECKS[index].2.to_string(), 1000)
}

// ============================================================================
// Text blocks.
// ============================================================================

/// Words, capitalized or not, some ending in `ing`, one of those
/// capitalized, so that flag `i` changes what `[a-z]+ing` matches, and some
/// with non-ASCII letters. None holds a character that flag `i` makes a case variant of
/// an ASCII letter (the Kelvin sign, the long s, the dotless i, the dotted
/// capital I), so that the scans below can treat ASCII letters alone.
const WORDS: [&str; 24] = [
    "the",
    "value",
    "of",
    "report",
    "running",
    "during",
    "Closing",
    "table",
    "amount",
    "year",
    "period",
    "opening",
    "Monday",
    "Total",
    "Note",
    "Section",
    "Größe",
    "café",
    "naïve",
    "Ελλάδα",
    "данные",
    "Straße",
    "Ångström",
    "être",
];

const TAGS: [&str; 7] = [
    "<p>",
    "</p>",
    "<b>",
    "</b>",
    "<a href=\"#n\">",
    "</a>",
    "<br/>",
];

const SEPARATORS: [&str; 8] = [" ", " ", " ", " ", "\n", ", ", ". ", "  \n"];

/// A text block of at least `len` bytes, the same for the same `len`: words,
/// numbers of one to five digits, HTML-like tags, separated by spaces,
/// newlines and punctuation. It begins and ends with a word, so it neither
/// begins nor ends with whitespace.
pub fn text(len: usize) -> String {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut out = String::with_capacity(len + 32);
    out.push_str(WORDS[0]);
    while out.len() < len {
        let r = next();
        out.push_str(SEPARATORS[(r % 8) as usize]);
        match (r >> 8) % 10 {
            0 => out.push_str(TAGS[((r >> 16) % 7) as usize]),
            1 | 2 => {
                let digits = 1 + (r >> 16) % 5;
                for d in 0..digits {
                    out.push((b'0' + ((r >> (24 + 4 * d)) % 10) as u8) as char);
                }
            }
            _ => out.push_str(WORDS[((r >> 16) % 24) as usize]),
        }
    }
    out.push(' ');
    out.push_str(WORDS[1]);
    out
}

/// XSD's `\s`: space, tab, newline and carriage return.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// `count(tokenize($s, '\s+'))` for a text that neither begins nor ends
/// with whitespace.
pub fn count_tokens(s: &str) -> usize {
    s.split(is_space).filter(|t| !t.is_empty()).count()
}

/// `string-length(replace($s, '\s+', ' '))`.
pub fn collapsed_length(s: &str) -> usize {
    let mut n = 0;
    let mut in_space = false;
    for c in s.chars() {
        if is_space(c) {
            if !in_space {
                n += 1;
            }
            in_space = true;
        } else {
            n += 1;
            in_space = false;
        }
    }
    n
}

/// `string-length(replace($s, '<[^>]+>', ''))`: every `<` in the text
/// opens a tag that a later `>` closes.
pub fn stripped_length(s: &str) -> usize {
    let mut n = 0;
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => n += 1,
            _ => {}
        }
    }
    n
}

/// `string-length(replace($s, '[a-z]+ing', '#', 'i'))`. Under flag `i` the
/// class is the ASCII letters (the text holds none of their other case
/// variants), so a match lies inside a run of them: from the run's start,
/// the greedy repetition backs off to the run's last `ing` that has a
/// letter before it, and what follows that `ing` holds no other.
pub fn ing_replaced_length(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut n = 0;
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() {
            n += 1;
            i += 1;
            continue;
        }
        let mut end = i;
        while end < chars.len() && chars[end].is_ascii_alphabetic() {
            end += 1;
        }
        let run: String = chars[i..end].iter().collect::<String>().to_lowercase();
        match run.rfind("ing").filter(|&at| at >= 1) {
            Some(at) => n += 1 + (end - i) - (at + 3),
            None => n += end - i,
        }
        i = end;
    }
    n
}

/// `replace($s, '([A-Z][a-z]+) ([0-9]{2,4})', '$2 $1')`.
pub fn swapped(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_uppercase() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_lowercase() {
                j += 1;
            }
            if j > i + 1 && j < chars.len() && chars[j] == ' ' {
                let mut k = j + 1;
                while k < chars.len() && k - (j + 1) < 4 && chars[k].is_ascii_digit() {
                    k += 1;
                }
                if k - (j + 1) >= 2 {
                    out.extend(&chars[j + 1..k]);
                    out.push(' ');
                    out.extend(&chars[i..j]);
                    i = k;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `count(analyze-string($s, '(\p{L}+)\s+(\p{L}+)')/fn:match)`: the text's
/// letters are all in category L, and a match is a run of letters, a run
/// of whitespace and a run of letters, each run whole, the next search
/// starting after it.
pub fn word_pairs(s: &str) -> usize {
    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Letter,
        Space,
        Other,
    }
    let kind = |c: char| {
        if c.is_alphabetic() {
            Kind::Letter
        } else if is_space(c) {
            Kind::Space
        } else {
            Kind::Other
        }
    };
    // The runs of letters and of whitespace; every other character is a
    // run of its own.
    let mut runs: Vec<Kind> = Vec::new();
    for c in s.chars() {
        let k = kind(c);
        if k == Kind::Other || runs.last() != Some(&k) {
            runs.push(k);
        }
    }
    let mut n = 0;
    let mut i = 0;
    while i + 2 < runs.len() {
        if runs[i..i + 3] == [Kind::Letter, Kind::Space, Kind::Letter] {
            n += 1;
            i += 3;
        } else {
            i += 1;
        }
    }
    n
}

/// The text workloads: a name, the expression, and its answer on a text.
pub type TextWorkload = (&'static str, &'static str, fn(&str) -> String);

pub const TEXT_WORKLOADS: [TextWorkload; 8] = [
    ("tokenize_spaces", r"count(tokenize($s, '\s+'))", |s| {
        count_tokens(s).to_string()
    }),
    (
        "replace_spaces",
        r"string-length(replace($s, '\s+', ' '))",
        |s| collapsed_length(s).to_string(),
    ),
    (
        "replace_tags",
        r"string-length(replace($s, '<[^>]+>', ''))",
        |s| stripped_length(s).to_string(),
    ),
    (
        "replace_ing_i",
        r"string-length(replace($s, '[a-z]+ing', '#', 'i'))",
        |s| ing_replaced_length(s).to_string(),
    ),
    (
        "replace_groups",
        r"replace($s, '([A-Z][a-z]+) ([0-9]{2,4})', '$2 $1')",
        swapped,
    ),
    (
        "analyze_pairs",
        r"count(analyze-string($s, '(\p{L}+)\s+(\p{L}+)')/fn:match)",
        |s| word_pairs(s).to_string(),
    ),
    // Numbers in the text have at most five digits: the search reads it
    // all.
    ("matches_absent", r"matches($s, '[0-9]{6}')", |_| {
        "false".to_string()
    }),
    ("matches_blank", r"matches($s, '^\s*$')", |_| {
        "false".to_string()
    }),
];

/// The text workload named `name` on a text block of `size` bytes.
pub fn text_workload(name: &str, size: usize) -> Workload {
    let (_, expr, answer) = TEXT_WORKLOADS.iter().find(|w| w.0 == name).unwrap();
    let input = text(size);
    let answer = answer(&input);
    Workload::new(expr, input, answer)
}

// ============================================================================
// Shapes whose cost grows faster than the input on some matchers.
// ============================================================================

/// A name, the pattern, a generator of the input, and the answer of
/// `matches($s, pattern)`.
pub type Shape = (&'static str, &'static str, fn() -> String, bool);

pub const SHAPES: [Shape; 4] = [
    // A match only at the last character; a search that starts at every
    // position and runs the repetition to the `x` each time is quadratic.
    (
        "alternation_star",
        "(a|b)*c",
        || {
            let mut s: String = (0..2_000)
                .map(|i| if (i * 7) % 3 == 0 { 'b' } else { 'a' })
                .collect();
            s.push_str("xc");
            s
        },
        true,
    ),
    // A wide counted repetition, its required `x` at the end.
    (
        "wide_count",
        ".{0,20000}x",
        || format!("{}x", "a".repeat(1_000)),
        true,
    ),
    (
        "counted_class",
        r"\w{1,255}x",
        || format!("{}x", "a".repeat(10_000)),
        true,
    ),
    // Exponential on a backtracking matcher: kept to 16 `a`s.
    (
        "nested_plus",
        "^(a+)+$",
        || format!("{}b", "a".repeat(16)),
        false,
    ),
];

/// The shape named `name`.
pub fn shape(name: &str) -> Workload {
    let (_, pattern, input, answer) = SHAPES.iter().find(|w| w.0 == name).unwrap();
    Workload::new(&format!("matches($s, '{pattern}')"), input(), answer)
}
