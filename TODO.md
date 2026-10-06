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
