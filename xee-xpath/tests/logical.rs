use insta::assert_debug_snapshot;

mod common;

use common::{run, run_xml};

// `and` and `or` are lowered to conditionals so that evaluation
// short-circuits left to right; the tests with an error-raising right
// operand pin that behavior.

#[test]
fn test_and_true_true() {
    assert_debug_snapshot!(run("true() and true()"));
}

#[test]
fn test_and_true_false() {
    assert_debug_snapshot!(run("true() and false()"));
}

#[test]
fn test_and_false_true() {
    assert_debug_snapshot!(run("false() and true()"));
}

#[test]
fn test_or_false_false() {
    assert_debug_snapshot!(run("false() or false()"));
}

#[test]
fn test_or_false_true() {
    assert_debug_snapshot!(run("false() or true()"));
}

#[test]
fn test_or_true_false() {
    assert_debug_snapshot!(run("true() or false()"));
}

#[test]
fn test_and_effective_boolean_value() {
    // operands are reduced to their effective boolean value
    assert_debug_snapshot!(run("1 and 'a'"));
}

#[test]
fn test_and_empty_sequence() {
    assert_debug_snapshot!(run("() and true()"));
}

#[test]
fn test_or_empty_sequence() {
    assert_debug_snapshot!(run("() or 'x'"));
}

#[test]
fn test_and_false_guards_error() {
    // false guard: the erroring right operand must not be evaluated
    assert_debug_snapshot!(run(r#"false() and xs:int("") eq 1"#));
}

#[test]
fn test_or_true_guards_error() {
    // true guard: the erroring right operand must not be evaluated
    assert_debug_snapshot!(run(r#"true() or xs:int("") eq 1"#));
}

#[test]
fn test_and_true_propagates_error() {
    // no guard: the error from the right operand must surface
    assert_debug_snapshot!(run(r#"true() and xs:int("") eq 1"#));
}

#[test]
fn test_or_false_propagates_error() {
    // no guard: the error from the right operand must surface
    assert_debug_snapshot!(run(r#"false() or xs:int("") eq 1"#));
}

#[test]
fn test_and_left_error_propagates() {
    // an error in the left operand always surfaces
    assert_debug_snapshot!(run(r#"xs:int("") eq 1 and false()"#));
}

#[test]
fn test_and_guarded_division() {
    // the guarded idiom from the spec note on logical expressions
    assert_debug_snapshot!(run("0 ne 0 and 1 div 0 lt 1"));
}

#[test]
fn test_and_ebv_type_error_propagates() {
    // EBV of a multi-item non-node sequence is a type error (FORG0006);
    // with a true left operand the right operand is evaluated and its
    // EBV error must surface
    assert_debug_snapshot!(run("true() and (1, 2)"));
}

#[test]
fn test_nested_logical() {
    assert_debug_snapshot!(run("(false() and xs:int('') eq 1) or true()"));
}

#[test]
fn test_or_true_true() {
    assert_debug_snapshot!(run("true() or true()"));
}

#[test]
fn test_or_left_error_propagates() {
    // an error in the left operand always surfaces, also for `or`
    assert_debug_snapshot!(run(r#"xs:int("") eq 1 or true()"#));
}

#[test]
fn test_chained_and() {
    assert_debug_snapshot!(run("1 eq 1 and 2 eq 2 and 3 eq 3"));
}

#[test]
fn test_chained_and_guards_error() {
    // left associativity: (false and err) and err — both erroring
    // operands are guarded
    assert_debug_snapshot!(run(r#"false() and xs:int("") eq 1 and xs:int("") eq 2"#));
}

#[test]
fn test_empty_sequence_guards_type_error() {
    // the empty-sequence guard suppresses the FORG0006 the right
    // operand's EBV would raise
    assert_debug_snapshot!(run("() and (1, 2)"));
}

#[test]
fn test_nan_short_circuits() {
    // EBV of NaN is false, so the erroring right operand is guarded
    assert_debug_snapshot!(run(r#"xs:double("NaN") and xs:int("") eq 1"#));
}

#[test]
fn test_for_rebinding() {
    // the right operand re-evaluates per iteration with the rebound
    // variable: the guard takes the $i = 0 branch, the division the other
    assert_debug_snapshot!(run("for $i in (0, 1) return ($i ne 0 and 1 div $i lt 2)"));
}

#[test]
fn test_quantified() {
    assert_debug_snapshot!(run(
        "some $i in (0, 1) satisfies ($i ne 0 and 1 div $i lt 2)"
    ));
}

#[test]
fn test_inside_predicate() {
    assert_debug_snapshot!(run("(1, 2, 3)[. gt 1 and . lt 3]"));
}

#[test]
fn test_empty_node_set_guards_error() {
    // EBV of an empty node-set is false: the erroring right operand is
    // guarded
    assert_debug_snapshot!(run_xml("<root/>", r#"/root/missing and xs:int("") eq 1"#));
}

#[test]
fn test_node_set_effective_boolean_value() {
    // EBV of a non-empty node sequence is true (no FORG0006 for
    // multiple nodes)
    assert_debug_snapshot!(run_xml("<root><a/><a/></root>", "/root/a and true()"));
}
