//! Numeric map keys (F&O 3.1 §17.1.1, op:same-key): xs:decimal, xs:double
//! and xs:float keys are the same key when their exact decimal values are
//! equal, with no rounding.

mod common;

use common::run;
use xee_xpath::error::ErrorValue;

fn assert_true(expr: &str) {
    let result = run(expr).unwrap_or_else(|e| panic!("{expr}: {e:?}"));
    assert!(
        result.effective_boolean_value().unwrap(),
        "{expr} should be true"
    );
}

#[test]
fn test_doubles_that_differ_in_the_last_bit_are_different_keys() {
    assert_true("map:size(map{1.0000000000000002e0: 1, 1e0: 2}) eq 2");
    assert_true("empty(map{1.0000000000000002e0: 'a'}(1))");
    assert_true("map{1.0000000000000002e0: 'a'}(1.0000000000000002e0) eq 'a'");
}

// The double 0.1 is 0.1000000000000000055511151231257827..., and the float
// 0.1 is 0.100000001490116119384765625; neither is the decimal 0.1.
#[test]
fn test_a_double_is_keyed_by_its_exact_value() {
    assert_true("map:size(map{0.1e0: 1, 0.1: 2}) eq 2");
    assert_true("empty(map{xs:float('0.1'): 1}(0.1e0))");
    assert_true("empty(map{0.1: 1}(0.1e0))");
    assert_true("empty(map{xs:float('0.1'): 1}(0.1))");
    // The float 0.1 is 13421773 / 2^27.
    assert_true("map{xs:float('0.1'): 'a'}(0.100000001490116119384765625) eq 'a'");
    // Negative fractions keep their sign.
    assert_true("map{-2.75: 'a'}(-2.75e0) eq 'a'");
    assert_true("empty(map{2.75: 'a'}(-2.75e0))");
    // A decimal key finds a decimal entry, not the double of the same
    // lexical form, and the other way round.
    assert_true("map{0.1: 'a'}(0.1) eq 'a'");
    assert_true("empty(map{0.1e0: 'a'}(0.1))");
    // 2^-20 fits an xs:decimal exactly.
    assert_true("map{0.00000095367431640625: 'a'}(9.5367431640625e-7) eq 'a'");
    assert_true("map{xs:float(1.5): 'a'}(1.50) eq 'a'");
    assert_true("map{1.5e0: 'a'}(xs:float(1.5)) eq 'a'");
}

#[test]
fn test_integral_numbers_of_every_type_are_one_key() {
    assert_true("map{1e0: 'a'}(1) eq 'a'");
    assert_true("map{1: 'a'}(1.0) eq 'a'");
    assert_true("map{xs:float(3): 'a'}(3e0) eq 'a'");
    assert_true("map{-0e0: 'a'}(0) eq 'a'");
    assert_true("map{xs:float('-0'): 'a'}(0e0) eq 'a'");
    assert_true(
        "map:size(map:put(map:put(map:put(map{1e0: 1}, 1, 2), 1.0, 3), xs:float(1), 4)) eq 1",
    );
    assert_true("map{1: 'a', 2e0: 'b'}(xs:float(2)) eq 'b'");
    assert_true("map{1.50: 'a', 2: 'b'}(1.5e0) eq 'a'");
    // Integral decimals keep their sign.
    assert_true("map{-1.0: 'a'}(-1) eq 'a'");
    assert_true("empty(map{-1.0: 'a'}(1))");
    assert_true("map:size(map{-1.0: 1, 1: 2}) eq 2");
    // put replaces the value of the same key.
    assert_true("map:put(map{1e0: 1}, 1, 2)(1) eq 2");
}

// Doubles beyond xs:decimal's range are still keys, and never an error.
#[test]
fn test_doubles_beyond_the_decimal_range_are_keys() {
    assert_true("map{1e30: 'a'}(1e30) eq 'a'");
    assert_true("map{1e30: 'a'}(1000000000000000019884624838656) eq 'a'");
    assert_true("empty(map{1e30: 'a'}(1000000000000000000000000000000))");
    assert_true("map:size(map{1e308: 1, -1e308: 2, 5e-324: 3, 1.7976931348623157e308: 4}) eq 4");
    assert_true("map{5e-324: 'a'}(4.9406564584124654e-324) eq 'a'");
    // A subnormal is not zero.
    assert_true("empty(map{0: 'a'}(5e-324))");
    assert_true("map:size(map{0e0: 1, 5e-324: 2, -5e-324: 3}) eq 3");
    assert_true("map{xs:float('1e30'): 'a'}(1000000015047466219876688855040) eq 'a'");
    assert_true("map:contains(map:put(map{}, 1e300, 1), 1e300)");
    assert_true("not(map:contains(map:remove(map{1e300: 1}, 1e300), 1e300))");
    // Maps of more than one entry take another path through put and remove.
    assert_true("map:contains(map:put(map{1: 1, 2: 2}, 1e300, 3), 1e300)");
    assert_true("map:size(map:put(map{1: 1, 1e300: 2}, 1e300, 3)) eq 2");
    assert_true("map:put(map{1: 1, 1e300: 2}, 1e300, 3)(1e300) eq 3");
    assert_true("not(map:contains(map:remove(map{1e300: 1, 2: 2}, 1e300), 1e300))");
    assert_true("map:size(map:remove(map{0.1e0: 1, 0.1: 2}, 0.1)) eq 1");
    assert_true("map:contains(map:remove(map{0.1e0: 1, 0.1: 2}, 0.1), 0.1e0)");
    assert_true("array:size(map:find([map{1e30: 1}], 1e30)) eq 1");
    assert_true("array:size(map:find([], 1e30)) eq 0");
}

#[test]
fn test_special_values_stay_keys() {
    assert_true("map{xs:double('NaN'): 'a'}(xs:float('NaN')) eq 'a'");
    assert_true("map{xs:double('INF'): 'a'}(xs:float('INF')) eq 'a'");
    assert_true("map{xs:double('-INF'): 'a'}(xs:float('-INF')) eq 'a'");
    assert_true("map:size(map{xs:double('INF'): 1, xs:double('-INF'): 2, 1e308: 3}) eq 3");
}

// Keys that are the same key are still a duplicate in one constructor.
#[test]
fn test_equal_numeric_keys_are_duplicates() {
    for expr in [
        "map{1e0: 1, 1: 2}",
        "map{1.5: 1, xs:float(1.5): 2}",
        "map{1e30: 1, 1000000000000000019884624838656: 2}",
        "map{0e0: 1, -0e0: 2}",
        "map{1.5: 1, 1.50: 2}",
        "map{-0.25: 1, -0.250: 2, 3: 3}",
    ] {
        assert_eq!(run(expr).expect_err(expr).error, ErrorValue::XQDY0137);
    }
}
