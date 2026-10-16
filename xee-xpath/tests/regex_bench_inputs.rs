//! Checks the regex benchmark's workloads under `cargo test`.
//!
//! `benches/regex.rs` checks each workload's answer before timing it, but
//! `cargo test` does not run benches. This binary includes the same module
//! (inputs, expressions and the scans that compute their answers, defined
//! once) and checks every workload, so that an input, an expression or an
//! answer that breaks fails the test suite. The text workloads are checked
//! on the sizes up to 100 KB here, and the size of every text block; the
//! bench checks the workloads on the largest itself.

#[path = "../benches/regex_inputs/mod.rs"]
mod regex_inputs;

use regex_inputs::*;
use xee_xpath::error::ErrorValue;
use xee_xpath::Documents;

#[test]
fn the_short_checks_match() {
    for (name, _, _) in SHORT_CHECKS {
        short_check(name).check();
        repeated(name).check();
    }
}

#[test]
fn the_code_list_is_an_anchored_alternation_of_60_codes() {
    let list = code_list();
    assert!(list.starts_with("^(?:AA|AB|"));
    assert!(list.ends_with("|CG|CH)$"));
    assert_eq!(list.matches('|').count(), 59);
}

#[test]
fn the_text_workloads_give_their_answers() {
    for size in TEXT_SIZES.into_iter().filter(|&size| size <= 100_000) {
        for (name, _, _) in TEXT_WORKLOADS {
            text_workload(name, size).check();
        }
    }
}

/// Each text block is as long as its size says: it stops at the first
/// separator and word, at most 16 bytes, that take it to its size, and
/// ends with ` value`.
#[test]
fn the_text_blocks_have_their_sizes() {
    for size in TEXT_SIZES {
        let len = text(size).len();
        assert!(len >= size && len < size + 32, "{size}: {len} bytes");
    }
}

/// The text sizes stop within a call's budget of steps: past it a text
/// workload is refused with XPDY0130 (an implementation limit), and the one
/// that costs the most steps a byte of the text, `analyze_pairs`, about 34,
/// is refused from about 1.5 MB, half again the largest size.
#[test]
fn a_text_block_past_the_budget_is_an_implementation_limit() {
    let answered = text_workload("analyze_pairs", 1_400_000);
    answered.check();
    let refused = text_workload("analyze_pairs", 1_500_000);
    assert_eq!(refused.evaluate().1, Err(ErrorValue::XPDY0130));
}

#[test]
fn the_shapes_give_their_answers() {
    for (name, _, _, _) in SHAPES {
        shape(name).check();
    }
}

/// The text holds what its workloads look for, so that none of them is
/// timed doing nothing.
#[test]
fn the_text_holds_each_kind_of_content() {
    let s = text(10_000);
    assert!(s.len() >= 10_000 && s.len() < 10_100);
    assert_eq!(s, text(10_000));
    assert!(!s.starts_with(char::is_whitespace) && !s.ends_with(char::is_whitespace));
    for needle in [
        "\n",
        "  \n",
        "<p>",
        "<a href=\"#n\">",
        "Größe",
        "Ελλάδα",
        "running",
        "Closing",
    ] {
        assert!(s.contains(needle), "{needle:?}");
    }
    // Flag `i` decides `replace_ing_i`'s answer: without it `[a-z]+ing`
    // leaves the `C` of `Closing`.
    let (_, with_i, _) = TEXT_WORKLOADS
        .into_iter()
        .find(|w| w.0 == "replace_ing_i")
        .unwrap();
    let without_i = with_i.replace(", 'i')", ")");
    assert_ne!(without_i, with_i);
    let answer = ing_replaced_length(&s).to_string();
    assert_ne!(
        Workload::new(&without_i, s.clone(), "").evaluate().1,
        Ok(answer)
    );
    assert_ne!(swapped(&s), s);
    assert!(ing_replaced_length(&s) < s.chars().count());
    assert!(stripped_length(&s) < s.chars().count());
    assert!(collapsed_length(&s) < s.chars().count());
    assert!(word_pairs(&s) > 100);
}

/// The scans that compute the text workloads' answers, on texts small
/// enough to work out by hand.
#[test]
fn the_scans_answer_small_texts() {
    assert_eq!(count_tokens("a  b\nc"), 3);
    assert_eq!(collapsed_length("a  b \n c"), 5);
    assert_eq!(stripped_length("x<p>y</p><br/>z"), 3);
    // `# # ing Größe`: `running` and `Ringing` (backing off to its last
    // `ing`, the whole word) are matches; `ing` alone has no letter before
    // it, and `ö` and `ß` end the ASCII run `Gr`.
    assert_eq!(ing_replaced_length("running Ringing ing Größe"), 13);
    assert_eq!(
        swapped("Total 12345 Note 7, Monday 2024 Größe 12"),
        "1234 Total5 Note 7, 2024 Monday Größe 12"
    );
    // `of Total` and `day Note`; `a, b` is no pair.
    assert_eq!(word_pairs("of Total 12 day\nNote a, b"), 2);
}

/// Each timed evaluation builds in an arena of its own: what one run's
/// `fn:analyze-string` builds is not kept in the arena the next run uses,
/// where it would make every later sample's arena larger (about 80 MB a
/// run at 1 MB of text). Two runs that each start from an empty arena
/// build their result at the same node of it. (The tree's string value
/// is its input's.)
#[test]
fn each_run_builds_in_an_arena_of_its_own() {
    let s = text(1_000);
    let tree = r"analyze-string($s, '(\p{L}+)\s+(\p{L}+)')";
    let prepared = Workload::new(tree, s.clone(), s).check();
    let root = |documents: &mut Documents| {
        let result = prepared.run(documents).unwrap();
        result.one().unwrap().to_node().unwrap()
    };
    assert_eq!(root(&mut Documents::new()), root(&mut Documents::new()));
}
