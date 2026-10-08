# TODO

Open xee work that is known and not scheduled. This list is a stopgap:
whether xee needs a register like `xbrlstd-rs`'s technical debt is not
decided. Each item says what is wrong, how it was measured, and what a fix
would take; an item leaves the list with the change that settles it.

## Decide: a slice for intermediate values held to the end of their `let` chain

The lowering binds every subexpression to a local of the expression's
`let` chain (the IR is in administrative normal form), and a local holds
its value until that chain ends. An expression's memory is therefore the
sum of its intermediate values, not the largest of them:
`count($c ! .) + count($c ! .) + …`, 4,000 terms over 4,000 nodes (80 KB
of source), holds 403 MB (release, measured 2026-10-05). Comma sequences
and `union`, `intersect` and `except` chains no longer pay it, as each item
or operand is an expression of its own whose bindings end with it; every
other expression does. A general fix releases a local after its last use:
a liveness pass in the bytecode compiler, or bindings scoped to their uses
in the lowering.

## Decide: a slice for string-concatenation chains

`a || b || c …` is lowered as a left fold of two-operand `Concat` steps,
each held in a local (the item above), so the chain holds every
intermediate string: 20,000 terms (160 KB) take 516 MB, quadratic in its
length (release, measured 2026-10-05). Concatenation is associative, so
the chain could be one expression of all its operands, joined as a comma
sequence's items are (`compile_joined`) or concatenated at once.

## Upgrade: thirteen breaking dependency majors

xee's dependencies have thirteen breaking majors outstanding, found by
`xbrlstd-rs`'s 2026-09 upgrade round in its survey of this fork
(`cargo update --dry-run --verbose` on `main` at `61e4fb9b`, 2026-10-02;
`docs/archive/plan-upgrade-2026-09.md` there, *Survey of the xee fork*;
`base64`, the fourteenth, has since moved) and deferred there, since a round fits between two Rust
releases. Nine reach `xbrlstd-rs`'s build: `icu` 1.5 → 2 with
`icu_provider_adapters`, `syn` 2 → 3, `logos` 0.15 → 0.16, `itertools`
0.14 → 0.15, `strum` 0.27 → 0.28, `num-derive` 0.4 → 0.5, and `rand` 0.8 →
0.10 with `rand_xoshiro` 0.6 → 0.8; four are xee's alone: `ariadne`,
`rustyline`, `crossterm` and `ron`. A fix is one major per change: the
whole changelog span read, the per-test verdicts of both conformance
suites compared before and after, and, for the nine, the `xbrlstd-rs` rev
bump that takes them, with its own gate.

## Refresh: the vendored W3C test suites

The vendored suites lag upstream: `xpath-tests/` is `w3c/qt3tests` at
`f8987877` (2024-05-16) and `xslt-tests/` is `w3c/xslt30-test` at
`1fcf15b0` (2023-11-17), as `vendor/README.md` records, and upstream had
25 and 34 commits beyond them on 2026-10-03, when the round recorded that
provenance. A refresh moves
verdicts by construction, which is why `xbrlstd-rs`'s 2026-09 round
deferred it. A fix moves each suite to a recorded upstream commit,
updates `vendor/README.md`'s provenance, and explains every verdict that
moves, before the `xbrlstd-rs` rev bump that would carry it.
