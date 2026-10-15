//! The character data a pattern refers to: the category escapes `\p{X}`
//! (XSD 1.1 Part 2 §G.4.2.2), the block escapes `\p{IsX}` (§G.4.2.3),
//! the XML name characters of `\i` and `\c` (§G.4.2.5), and the case
//! variants of flag `i` (F&O 3.1 §5.6.2), all from the generated tables
//! (`tables.rs`).

use std::sync::LazyLock;

use crate::charset::CharSet;
use crate::tables::{BLOCKS, CASE_VARIANTS, CATEGORIES};

/// `\p{X}` for a category name of XSD's list, or `None`, each set built
/// once. A one-letter name is the union of the two-letter categories that
/// start with it.
pub(crate) fn category(name: &str) -> Option<&'static CharSet> {
    static SETS: LazyLock<Vec<(&str, CharSet)>> = LazyLock::new(|| {
        let mut sets: Vec<(&str, CharSet)> = CATEGORIES
            .iter()
            .map(|(name, ranges)| (*name, CharSet::from_ranges(ranges.to_vec())))
            .collect();
        for letter in ["L", "M", "N", "P", "Z", "S", "C"] {
            let ranges = CATEGORIES
                .iter()
                .filter(|(c, _)| c.starts_with(letter))
                .flat_map(|(_, r)| r.iter().copied())
                .collect();
            sets.push((letter, CharSet::from_ranges(ranges)));
        }
        sets
    });
    SETS.iter().find(|(n, _)| *n == name).map(|(_, set)| set)
}

/// The block names XSD 1.1 Part 2 §G.4.2.3 lists from Unicode 3.1 that
/// the current `Blocks.txt` no longer has, with their code points.
const OLD_BLOCKS: [(&str, &[(u32, u32)]); 3] = [
    ("Greek", &[(0x370, 0x3FF)]),
    ("CombiningMarksforSymbols", &[(0x20D0, 0x20FF)]),
    (
        "PrivateUse",
        &[(0xE000, 0xF8FF), (0xF0000, 0xFFFFD), (0x100000, 0x10FFFD)],
    ),
];

/// `\p{IsX}` for a block name X, compared as written once white space
/// and underscores are removed, or `None`.
pub(crate) fn block(name: &str) -> Option<CharSet> {
    if let Some((_, a, b)) = BLOCKS.iter().find(|(n, _, _)| *n == name) {
        return Some(CharSet::range(*a, *b));
    }
    OLD_BLOCKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, r)| CharSet::from_ranges(r.to_vec()))
}

/// The sets of the class escapes `\s`, `\i`, `\c`, `\d` and `\w`
/// (XSD 1.1 Part 2 §G.4.2.5), each built once.
pub(crate) fn class_escape(c: char) -> Option<&'static CharSet> {
    static S: LazyLock<CharSet> =
        LazyLock::new(|| CharSet::from_ranges(vec![(0x9, 0xA), (0xD, 0xD), (0x20, 0x20)]));
    static I: LazyLock<CharSet> = LazyLock::new(name_start_char);
    static C: LazyLock<CharSet> = LazyLock::new(name_char);
    static D: LazyLock<CharSet> =
        LazyLock::new(|| category("Nd").expect("Nd is a category").clone());
    // [#x0000-#x10FFFF]-[\p{P}\p{Z}\p{C}]
    static W: LazyLock<CharSet> = LazyLock::new(|| {
        let not_word = ["P", "Z", "C"]
            .iter()
            .map(|n| category(n).expect("a category"))
            .fold(CharSet::empty(), |a, b| a.union(b));
        CharSet::all().difference(&not_word)
    });
    Some(match c {
        's' => &S,
        'i' => &I,
        'c' => &C,
        'd' => &D,
        'w' => &W,
        _ => return None,
    })
}

/// XML 1.0 (fifth edition) production [4], NameStartChar: `\i`.
fn name_start_char() -> CharSet {
    CharSet::from_ranges(vec![
        (':' as u32, ':' as u32),
        ('A' as u32, 'Z' as u32),
        ('_' as u32, '_' as u32),
        ('a' as u32, 'z' as u32),
        (0xC0, 0xD6),
        (0xD8, 0xF6),
        (0xF8, 0x2FF),
        (0x370, 0x37D),
        (0x37F, 0x1FFF),
        (0x200C, 0x200D),
        (0x2070, 0x218F),
        (0x2C00, 0x2FEF),
        (0x3001, 0xD7FF),
        (0xF900, 0xFDCF),
        (0xFDF0, 0xFFFD),
        (0x10000, 0xEFFFF),
    ])
}

/// XML 1.0 (fifth edition) production [4a], NameChar: `\c`.
fn name_char() -> CharSet {
    name_start_char().union(&CharSet::from_ranges(vec![
        ('-' as u32, '-' as u32),
        ('.' as u32, '.' as u32),
        ('0' as u32, '9' as u32),
        (0xB7, 0xB7),
        (0x300, 0x36F),
        (0x203F, 0x2040),
    ]))
}

/// Whether `b` is a case variant of `a` (F&O 3.1 §5.6.2), a relation
/// that holds both ways.
pub(crate) fn is_case_variant(a: char, b: char) -> bool {
    CASE_VARIANTS
        .binary_search_by_key(&(a as u32), |(c, _)| *c)
        .is_ok_and(|k| CASE_VARIANTS[k].1.contains(&(b as u32)))
}

/// The set with the case variants of each of its characters, and the
/// number of table entries it visited, which is its work beyond the set
/// itself: a wide range visits thousands.
pub(crate) fn case_closure(set: &CharSet) -> (CharSet, usize) {
    let mut extra = Vec::new();
    let mut visited = 0;
    for &(a, b) in set.ranges() {
        let from = CASE_VARIANTS.partition_point(|(c, _)| *c < a);
        for (c, variants) in &CASE_VARIANTS[from..] {
            if *c > b {
                break;
            }
            visited += 1;
            extra.extend(variants.iter().map(|&v| (v, v)));
        }
    }
    if extra.is_empty() {
        (set.clone(), visited)
    } else {
        (set.union(&CharSet::from_ranges(extra)), visited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charset::MAX_CP;
    use std::collections::HashMap;

    fn ch(c: char) -> u32 {
        c as u32
    }

    #[test]
    fn the_case_variants_are_those_of_lower_case_and_upper_case() {
        // F&O 3.1 §5.6.2's definition, with the standard library's full
        // mappings, which `fn:lower-case` and `fn:upper-case` use.
        let chars: Vec<char> = (0..=MAX_CP).filter_map(char::from_u32).collect();
        let lower: Vec<String> = chars.iter().map(|c| c.to_lowercase().collect()).collect();
        let upper: Vec<String> = chars.iter().map(|c| c.to_uppercase().collect()).collect();
        let mut by_lower: HashMap<&str, Vec<u32>> = HashMap::new();
        let mut by_upper: HashMap<&str, Vec<u32>> = HashMap::new();
        for (k, c) in chars.iter().enumerate() {
            by_lower.entry(&lower[k]).or_default().push(*c as u32);
            by_upper.entry(&upper[k]).or_default().push(*c as u32);
        }
        let mut expected: Vec<(u32, Vec<u32>)> = Vec::new();
        for (k, c) in chars.iter().enumerate() {
            let mut v: Vec<u32> = by_lower[lower[k].as_str()]
                .iter()
                .chain(&by_upper[upper[k].as_str()])
                .copied()
                .filter(|&x| x != *c as u32)
                .collect();
            v.sort_unstable();
            v.dedup();
            if !v.is_empty() {
                expected.push((*c as u32, v));
            }
        }
        let table: Vec<(u32, Vec<u32>)> = CASE_VARIANTS
            .iter()
            .map(|(c, v)| (*c, v.to_vec()))
            .collect();
        assert_eq!(table, expected);
    }

    #[test]
    fn the_categories_partition_the_characters() {
        // Every character but a surrogate is in exactly one category.
        let mut all: Vec<(u32, u32)> = CATEGORIES
            .iter()
            .flat_map(|(_, r)| r.iter().copied())
            .collect();
        all.sort_unstable();
        let mut next = 0;
        for (a, b) in all {
            assert_eq!(a, next, "a gap or an overlap at {a:#X}");
            next = if b == 0xD7FF { 0xE000 } else { b + 1 };
        }
        assert_eq!(next, MAX_CP + 1);
    }

    #[test]
    fn the_categories_agree_with_the_standard_library() {
        // Where the standard library answers by general category: N is
        // `is_numeric` and Cc `is_control`, exactly. Where it answers by a
        // property derived from them, each category is inside it.
        let n = category("N").unwrap();
        let cc = category("Cc").unwrap();
        let inside = [
            ("Lu", char::is_uppercase as fn(char) -> bool),
            ("Ll", char::is_lowercase),
            ("L", char::is_alphabetic),
            ("Nl", char::is_alphabetic),
            ("Zs", char::is_whitespace),
        ];
        for c in (0..=MAX_CP).filter_map(char::from_u32) {
            let cp = c as u32;
            assert_eq!(n.contains(cp), c.is_numeric(), "{cp:#X}");
            assert_eq!(cc.contains(cp), c.is_control(), "{cp:#X}");
            for (name, property) in inside {
                if category(name).unwrap().contains(cp) {
                    assert!(property(c), "{name} {cp:#X}");
                }
            }
        }
    }

    #[test]
    fn blocks_are_whole_columns_in_order() {
        // Blocks.txt: a block starts at a code point that is 0 modulo 16 and
        // ends at one that is 15 modulo 16; blocks do not overlap.
        let mut previous_end = None;
        for (name, a, b) in BLOCKS.iter() {
            assert!(a % 16 == 0 && b % 16 == 15 && a < b, "{name}");
            assert!(previous_end.is_none_or(|e| e < *a), "{name}");
            previous_end = Some(*b);
        }
    }

    #[test]
    fn categories() {
        let lu = category("Lu").unwrap();
        assert!(lu.contains(ch('A')) && !lu.contains(ch('a')));
        assert!(category("Nd").unwrap().contains(0x0660)); // ARABIC-INDIC DIGIT ZERO
        assert!(category("L").unwrap().contains(ch('a')));
        // Unassigned, and a noncharacter: Cn.
        assert!(category("C").unwrap().contains(0x0378));
        assert!(category("Cn").unwrap().contains(0xFFFF));
        // Not in XSD's list.
        for name in ["Cs", "LC", "Letter", "", "X", "Lx"] {
            assert!(category(name).is_none(), "{name}");
        }
        // Every first letter of a two-letter category names their union.
        let mut letters: Vec<&str> = CATEGORIES.iter().map(|(n, _)| &n[..1]).collect();
        letters.dedup();
        assert_eq!(letters.len(), 7);
        for letter in letters {
            let union = CharSet::from_ranges(
                CATEGORIES
                    .iter()
                    .filter(|(n, _)| n.starts_with(letter))
                    .flat_map(|(_, r)| r.iter().copied())
                    .collect(),
            );
            assert_eq!(category(letter), Some(&union), "{letter}");
        }
    }

    #[test]
    fn blocks() {
        assert!(block("BasicLatin").unwrap().contains(ch('a')));
        assert!(block("Latin-1Supplement").unwrap().contains(0xE9));
        // Hyphens and case are kept: no other spelling is the name.
        assert!(block("Latin1Supplement").is_none());
        assert!(block("basiclatin").is_none());
        // XSD 1.1's Unicode 3.1 names.
        assert!(block("Greek").unwrap().contains(0x3B1));
        assert!(block("PrivateUse").unwrap().contains(0x10FFFD));
        assert!(block("CombiningMarksforSymbols").unwrap().contains(0x20D0));
        assert!(block("GreekandCoptic").unwrap().contains(0x3B1));
        // XSD 1.1 §G.4.2.3's Unicode 3.1 names, whole.
        assert_eq!(block("Greek").unwrap().ranges(), &[(0x370, 0x3FF)]);
        assert_eq!(
            block("CombiningMarksforSymbols").unwrap().ranges(),
            &[(0x20D0, 0x20FF)]
        );
        assert_eq!(
            block("PrivateUse").unwrap().ranges(),
            &[(0xE000, 0xF8FF), (0xF0000, 0xFFFFD), (0x100000, 0x10FFFD)]
        );
    }

    #[test]
    fn name_characters() {
        // XML 1.0 (fifth edition), [4] NameStartChar, whole.
        let start = [
            (0x3A, 0x3A),
            (0x41, 0x5A),
            (0x5F, 0x5F),
            (0x61, 0x7A),
            (0xC0, 0xD6),
            (0xD8, 0xF6),
            (0xF8, 0x2FF),
            (0x370, 0x37D),
            (0x37F, 0x1FFF),
            (0x200C, 0x200D),
            (0x2070, 0x218F),
            (0x2C00, 0x2FEF),
            (0x3001, 0xD7FF),
            (0xF900, 0xFDCF),
            (0xFDF0, 0xFFFD),
            (0x10000, 0xEFFFF),
        ];
        assert_eq!(class_escape('i').unwrap().ranges(), &start);
        // [4a] NameChar: those and "-", ".", [0-9], #xB7, [#x0300-#x036F],
        // [#x203F-#x2040], merged.
        let name = [
            (0x2D, 0x2E),
            (0x30, 0x3A),
            (0x41, 0x5A),
            (0x5F, 0x5F),
            (0x61, 0x7A),
            (0xB7, 0xB7),
            (0xC0, 0xD6),
            (0xD8, 0xF6),
            (0xF8, 0x37D),
            (0x37F, 0x1FFF),
            (0x200C, 0x200D),
            (0x203F, 0x2040),
            (0x2070, 0x218F),
            (0x2C00, 0x2FEF),
            (0x3001, 0xD7FF),
            (0xF900, 0xFDCF),
            (0xFDF0, 0xFFFD),
            (0x10000, 0xEFFFF),
        ];
        assert_eq!(class_escape('c').unwrap().ranges(), &name);
        assert!(class_escape('x').is_none());
    }

    #[test]
    fn case_closure_adds_the_variants() {
        // KELVIN SIGN lower-cases to k, LATIN SMALL LETTER LONG S
        // upper-cases to S.
        let (k, visited) = case_closure(&CharSet::single(ch('k')));
        assert!(k.contains(ch('K')) && k.contains(0x212A));
        assert_eq!(visited, 1);
        let (az, visited) = case_closure(&CharSet::range(ch('A'), ch('Z')));
        assert!(az.contains(ch('a')) && az.contains(0x212A) && az.contains(0x17F));
        assert_eq!(visited, 26);
        let (digits, visited) = case_closure(&CharSet::range(ch('0'), ch('9')));
        assert_eq!(digits, CharSet::range(ch('0'), ch('9')));
        assert_eq!(visited, 0);
    }
}
