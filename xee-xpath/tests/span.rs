mod common;

use common::run;
use xee_xpath::{error, Sequence};

fn span(result: error::Result<Sequence>) -> error::SourceSpan {
    result.err().unwrap().span.unwrap()
}

#[test]
fn test_left_side() {
    let expr = "0 + (2, 3, 4)";
    //          0123456789012
    //          0           12
    //  So from 0, 13 is expected
    let r = run(expr);
    assert_eq!(span(r), (0..13).into());
}

#[test]
fn test_right_side() {
    let expr = "(2, 3, 4) + 1";
    //          0123456789012
    //          0           12
    //  So from 0, 13 is expected
    let r = run(expr);
    assert_eq!(span(r), (0..13).into());
}

#[test]
fn test_left_right_side() {
    let expr = "0 + (2, 3, 4) + (12 + 1)";
    //          012345678901234567890123
    //          0           12
    //  So from 0, 13 is expected
    let r = run(expr);
    assert_eq!(span(r), (0..13).into());
}

#[test]
fn test_right_left_side() {
    let expr = "0 + 12 + ((2, 3, 4) + 1)";
    //          012345678901234567890123
    //                    10          22
    assert_eq!(span(run(expr)), (10..23).into());
}

#[test]
fn test_right_right_side() {
    let expr = "0 + 12 + (1 + (2, 3, 4))";
    //          012345678901234567890123
    //                    10          22
    assert_eq!(span(run(expr)), (10..23).into());
}

// Every step of a union, intersect or except chain carries the whole
// chain's span: the steps are joined in an order the source does not show.
#[test]
fn test_node_set_chain() {
    let expr = "1 | 2 | 3";
    //          012345678
    assert_eq!(span(run(expr)), (0..9).into());
    let expr = "1 except 2 except 3";
    //          0123456789012345678
    assert_eq!(span(run(expr)), (0..19).into());
    // the first two are joined as they come, the third at the end
    let expr = "() | () | 1";
    //          01234567890
    assert_eq!(span(run(expr)), (0..11).into());
}
