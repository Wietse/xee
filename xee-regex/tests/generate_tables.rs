//! Generates `src/tables.rs` from the Unicode Character Database, at the
//! version of the standard library on the pinned toolchain (plan
//! `plan-xpath-regex` D3):
//!
//! ```text
//! XEE_REGEX_UCD=<dir> cargo test -p xee-regex --test generate_tables -- --ignored
//! ```
//!
//! `<dir>` holds `UnicodeData.txt`, `SpecialCasing.txt`, `Blocks.txt` and
//! `DerivedGeneralCategory.txt`, fetched from
//! `https://www.unicode.org/Public/<version>/ucd/` (the last from its
//! `extracted/` directory). With the variable set, `the_tables_are_current`
//! also checks the checked-in file against them; without it, that test says
//! it skipped.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

const VERSION: (u8, u8, u8) = (17, 0, 0);
const TABLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/tables.rs");
const MAX_CP: u32 = 0x10FFFF;

/// The general categories XSD 1.1 Part 2 §G.4.2.2 lists, in its order;
/// `Cs` is not among them.
const CATEGORIES: [&str; 29] = [
    "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps", "Pe", "Pi",
    "Pf", "Po", "Zs", "Zl", "Zp", "Sm", "Sc", "Sk", "So", "Cc", "Cf", "Co", "Cn",
];

fn is_surrogate(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

fn ucd_dir() -> Option<PathBuf> {
    std::env::var_os("XEE_REGEX_UCD").map(PathBuf::from)
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.join(name).display()))
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim(), 16).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn code_points(s: &str) -> Vec<u32> {
    s.split_whitespace().map(hex).collect()
}

/// The file's first line names it with the version, as
/// `# Blocks-17.0.0.txt`.
fn check_version(text: &str, stem: &str) {
    let (a, b, c) = VERSION;
    let want = format!("# {stem}-{a}.{b}.{c}.txt");
    let first = text.lines().next().unwrap_or("");
    assert_eq!(first.trim(), want, "{stem}.txt is not version {a}.{b}.{c}");
}

/// Each line's fields before any `#` comment; blank and comment lines skipped.
fn records(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines().filter_map(|l| {
        let l = l.split('#').next().unwrap_or("").trim();
        (!l.is_empty()).then(|| l.split(';').map(str::trim).collect())
    })
}

/// `a..b` or `a`.
fn range(s: &str) -> (u32, u32) {
    match s.split_once("..") {
        Some((a, b)) => (hex(a), hex(b)),
        None => (hex(s), hex(s)),
    }
}

struct Ucd {
    /// The general category of every code point, as an index into
    /// `CATEGORIES`, or `None` for a surrogate.
    category: Vec<Option<u8>>,
    lower: HashMap<u32, Vec<u32>>,
    upper: HashMap<u32, Vec<u32>>,
    blocks: Vec<(String, u32, u32)>,
}

fn load(dir: &Path) -> Ucd {
    let data = read(dir, "UnicodeData.txt");
    let special = read(dir, "SpecialCasing.txt");
    let blocks_txt = read(dir, "Blocks.txt");
    let derived = read(dir, "DerivedGeneralCategory.txt");
    check_version(&special, "SpecialCasing");
    check_version(&blocks_txt, "Blocks");
    check_version(&derived, "DerivedGeneralCategory");

    let index = |gc: &str| -> Option<u8> {
        if gc == "Cs" {
            return None;
        }
        let k = CATEGORIES
            .iter()
            .position(|c| *c == gc)
            .unwrap_or_else(|| panic!("unknown general category {gc}"));
        Some(k as u8)
    };
    let cn = index("Cn");
    let mut category: Vec<Option<u8>> = (0..=MAX_CP)
        .map(|cp| if is_surrogate(cp) { None } else { cn })
        .collect();
    let mut lower = HashMap::new();
    let mut upper = HashMap::new();
    let mut first: Option<u32> = None;
    for f in records(&data) {
        let cp = hex(f[0]);
        let gc = index(f[2]);
        if f[1].ends_with(", First>") {
            first = Some(cp);
            continue;
        }
        let start = if f[1].ends_with(", Last>") {
            first.take().expect("a Last line follows its First line")
        } else {
            cp
        };
        for c in start..=cp {
            category[c as usize] = gc;
        }
        // Simple mappings, fields 12 (upper) and 13 (lower).
        if !f[12].is_empty() {
            upper.insert(cp, vec![hex(f[12])]);
        }
        if !f[13].is_empty() {
            lower.insert(cp, vec![hex(f[13])]);
        }
    }
    // Full mappings without a condition replace the simple ones, as
    // `fn:lower-case` and `fn:upper-case` apply them (F&O 3.1 §5.4.7, §5.4.8).
    for f in records(&special) {
        let conditional = f.get(4).is_some_and(|c| !c.is_empty());
        if conditional {
            continue;
        }
        let cp = hex(f[0]);
        lower.insert(cp, code_points(f[1]));
        upper.insert(cp, code_points(f[3]));
    }
    // The derived file lists every code point's category, Cn included:
    // the two readings agree.
    let mut derived_category: Vec<Option<u8>> = vec![cn; (MAX_CP + 1) as usize];
    for f in records(&derived) {
        let (a, b) = range(f[0]);
        for c in a..=b {
            derived_category[c as usize] = index(f[1]);
        }
    }
    assert!(
        category == derived_category,
        "UnicodeData.txt and DerivedGeneralCategory.txt disagree"
    );
    let blocks = records(&blocks_txt)
        .map(|f| {
            let (a, b) = range(f[0]);
            // XSD 1.1 Part 2 §G.4.2.3: white space and underscores are
            // removed from the name; hyphens and case are kept.
            let name: String = f[1]
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '_')
                .collect();
            (name, a, b)
        })
        .collect();
    Ucd {
        category,
        lower,
        upper,
        blocks,
    }
}

/// F&O 3.1 §5.6.2: C2 is a case-variant of C1 when `lower-case(C1) eq
/// lower-case(C2)` or `upper-case(C1) eq upper-case(C2)`.
fn case_variants(ucd: &Ucd) -> Vec<(u32, Vec<u32>)> {
    let map = |m: &HashMap<u32, Vec<u32>>, cp: u32| m.get(&cp).cloned().unwrap_or(vec![cp]);
    let mut by_lower: HashMap<Vec<u32>, Vec<u32>> = HashMap::new();
    let mut by_upper: HashMap<Vec<u32>, Vec<u32>> = HashMap::new();
    for cp in (0..=MAX_CP).filter(|&c| !is_surrogate(c)) {
        by_lower.entry(map(&ucd.lower, cp)).or_default().push(cp);
        by_upper.entry(map(&ucd.upper, cp)).or_default().push(cp);
    }
    let mut out = Vec::new();
    for cp in (0..=MAX_CP).filter(|&c| !is_surrogate(c)) {
        let mut v: Vec<u32> = by_lower[&map(&ucd.lower, cp)]
            .iter()
            .chain(&by_upper[&map(&ucd.upper, cp)])
            .copied()
            .filter(|&c| c != cp)
            .collect();
        v.sort_unstable();
        v.dedup();
        if !v.is_empty() {
            out.push((cp, v));
        }
    }
    out
}

/// Writes `items` separated by commas, wrapping lines at about 100
/// columns, each line indented by four spaces.
fn wrapped(out: &mut String, items: impl IntoIterator<Item = String>) {
    let mut line = String::new();
    for item in items {
        if !line.is_empty() && line.len() + item.len() + 1 > 96 {
            writeln!(out, "    {}", line.trim_end()).unwrap();
            line.clear();
        }
        line.push_str(&item);
        line.push_str(", ");
    }
    if !line.is_empty() {
        writeln!(out, "    {}", line.trim_end()).unwrap();
    }
}

fn generate(ucd: &Ucd) -> String {
    let (a, b, c) = VERSION;
    let mut out = String::new();
    writeln!(
        out,
        "// @generated by `tests/generate_tables.rs` from the Unicode Character
// Database {a}.{b}.{c} (UnicodeData.txt, SpecialCasing.txt, Blocks.txt,
// DerivedGeneralCategory.txt). Do not edit; regenerate it.
//
// The data is (c) Unicode, Inc., used under the Unicode License v3, whose
// copyright and permission notice is in LICENSE-UNICODE beside this crate's
// Cargo.toml.

/// The Unicode version the tables were generated from.
pub(crate) const UNICODE_VERSION: (u8, u8, u8) = ({a}, {b}, {c});
"
    )
    .unwrap();

    writeln!(
        out,
        "/// Each general category XSD 1.1 Part 2 §G.4.2.2 lists, with its code
/// points as sorted, disjoint, non-adjacent inclusive ranges.
pub(crate) static CATEGORIES: [(&str, &[(u32, u32)]); {}] = [",
        CATEGORIES.len()
    )
    .unwrap();
    for (k, name) in CATEGORIES.iter().enumerate() {
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for cp in 0..=MAX_CP {
            if ucd.category[cp as usize] != Some(k as u8) {
                continue;
            }
            match ranges.last_mut() {
                Some(r) if r.1 + 1 == cp => r.1 = cp,
                _ => ranges.push((cp, cp)),
            }
        }
        writeln!(out, "    (\"{name}\", &[").unwrap();
        let mut inner = String::new();
        wrapped(
            &mut inner,
            ranges.iter().map(|(a, b)| format!("(0x{a:X}, 0x{b:X})")),
        );
        for l in inner.lines() {
            writeln!(out, "    {l}").unwrap();
        }
        writeln!(out, "    ]),").unwrap();
    }
    writeln!(out, "];\n").unwrap();

    writeln!(
        out,
        "/// Every block, by its name with white space and underscores removed
/// (XSD 1.1 Part 2 §G.4.2.3), and its first and last code points.
pub(crate) static BLOCKS: [(&str, u32, u32); {}] = [",
        ucd.blocks.len()
    )
    .unwrap();
    for (name, a, b) in &ucd.blocks {
        writeln!(out, "    (\"{name}\", 0x{a:X}, 0x{b:X}),").unwrap();
    }
    writeln!(out, "];\n").unwrap();

    let variants = case_variants(ucd);
    writeln!(
        out,
        "/// Every code point that has case variants other than itself (F&O 3.1
/// §5.6.2), in order, with its variants in order.
pub(crate) static CASE_VARIANTS: [(u32, &[u32]); {}] = [",
        variants.len()
    )
    .unwrap();
    wrapped(
        &mut out,
        variants.iter().map(|(cp, v)| {
            let list: Vec<String> = v.iter().map(|x| format!("0x{x:X}")).collect();
            format!("(0x{cp:X}, &[{}])", list.join(", "))
        }),
    );
    writeln!(out, "];").unwrap();
    out
}

#[test]
#[ignore = "writes src/tables.rs from the UCD in XEE_REGEX_UCD"]
fn generate_tables() {
    let dir = ucd_dir().expect("XEE_REGEX_UCD names the UCD directory");
    std::fs::write(TABLES, generate(&load(&dir))).unwrap();
}

#[test]
fn the_tables_are_current() {
    let Some(dir) = ucd_dir() else {
        eprintln!("skipping: XEE_REGEX_UCD is not set");
        return;
    };
    let checked_in = std::fs::read_to_string(TABLES).unwrap();
    assert!(
        checked_in == generate(&load(&dir)),
        "src/tables.rs differs from what the UCD generates"
    );
}
