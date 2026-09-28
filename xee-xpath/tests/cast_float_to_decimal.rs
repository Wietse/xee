//! Casting xs:double and xs:float to xs:decimal (F&O 3.1 19.1.2.3): the
//! result is the decimal Xee can hold (a 96-bit coefficient, up to 28
//! fraction digits) that is numerically closest to the value, the one
//! closer to zero on a tie. The expected strings were computed from the
//! exact binary values, independently of Xee.

mod common;

use common::run;
use xee_xpath::{error::ErrorValue, Documents, Queries, Query};

fn string_of(expr: &str) -> String {
    let expr = format!("string({expr})");
    let mut documents = Documents::new();
    let queries = Queries::default();
    let q = queries
        .one(&expr, |_, item| Ok(item.try_into_value::<String>()?))
        .unwrap_or_else(|e| panic!("{expr}: {e:?}"));
    q.execute_build_context(&mut documents, |_| ())
        .unwrap_or_else(|e| panic!("{expr}: {e:?}"))
}

fn error_of(expr: &str) -> ErrorValue {
    run(expr).expect_err(expr).error
}

#[test]
fn test_fractions_keep_every_digit_that_fits() {
    for (expr, text) in [
        (
            "xs:decimal(1.0000000000000002e0)",
            "1.000000000000000222044604925",
        ),
        ("xs:decimal(0.1e0)", "0.1000000000000000055511151231"),
        (
            "xs:decimal(0.30000000000000004e0)",
            "0.300000000000000044408920985",
        ),
        (
            "xs:decimal(123456789.12345679e0)",
            "123456789.12345679104328155518",
        ),
        (
            "xs:decimal(0.7922816251426433e0)",
            "0.7922816251426433309390517934",
        ),
        (
            "xs:decimal(7.922816251426433e0)",
            "7.9228162514264326432567031588",
        ),
        ("xs:decimal(-2.75e0)", "-2.75"),
        // xs:float values are exact in fewer digits.
        (
            "xs:decimal(xs:float('0.1'))",
            "0.100000001490116119384765625",
        ),
        ("xs:decimal(xs:float('1.1'))", "1.10000002384185791015625"),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

// At scale 28 these need a coefficient above 2^96 - 1, so the nearest
// decimal has fewer fraction digits.
#[test]
fn test_the_coefficient_cap_moves_to_a_coarser_scale() {
    for (expr, text) in [
        (
            "xs:decimal(7.922816251426434e0)",
            "7.922816251426434419613542559",
        ),
        (
            "xs:decimal(9.999999999999998e0)",
            "9.9999999999999982236431606",
        ),
        (
            "xs:decimal(79.22816251426434e0)",
            "79.22816251426434064342174679",
        ),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

#[test]
fn test_integral_values_are_exact() {
    for (expr, text) in [
        ("xs:decimal(1e28)", "9999999999999999583119736832"),
        ("xs:decimal(7.9e28)", "78999999999999996926548246528"),
        (
            "xs:decimal(7.922816251426433e28)",
            "79228162514264328797450928128",
        ),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

// 2^-29 is 0.00000000186264514923095703125 exactly, one digit more than
// 28 fraction digits hold, and the dropped digit is a lone 5.
#[test]
fn test_a_tie_goes_toward_zero() {
    for (expr, text) in [
        (
            "xs:decimal(1.862645149230957e-9)",
            "0.0000000018626451492309570312",
        ),
        (
            "xs:decimal(-1.862645149230957e-9)",
            "-0.0000000018626451492309570312",
        ),
        (
            "xs:decimal(5.587935447692871e-9)",
            "0.0000000055879354476928710937",
        ),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

#[test]
fn test_values_below_the_smallest_fraction_round() {
    for (expr, text) in [
        ("xs:decimal(1.5e-28)", "0.0000000000000000000000000002"),
        ("xs:decimal(2.5e-28)", "0.0000000000000000000000000003"),
        ("xs:decimal(-2.5e-28)", "-0.0000000000000000000000000003"),
        ("xs:decimal(0.5e-28)", "0"),
        ("xs:decimal(5e-324)", "0"),
        ("xs:decimal(-5e-324)", "0"),
        ("xs:decimal(-0e0)", "0"),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

#[test]
fn test_values_beyond_the_largest_are_foca0001() {
    for expr in [
        // 2^96, one more than the largest coefficient
        "xs:decimal(7.922816251426434e28)",
        "xs:decimal(-7.922816251426434e28)",
        "xs:decimal(8e28)",
        "xs:decimal(1e300)",
        "xs:decimal(xs:float('3.4028235e38'))",
    ] {
        assert_eq!(error_of(expr), ErrorValue::FOCA0001, "{expr}");
    }
    for expr in [
        "xs:decimal(xs:double('NaN'))",
        "xs:decimal(xs:double('INF'))",
        "xs:decimal(xs:float('-INF'))",
    ] {
        assert_eq!(error_of(expr), ErrorValue::FOCA0002, "{expr}");
    }
}
