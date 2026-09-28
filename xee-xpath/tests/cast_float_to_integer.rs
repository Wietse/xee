//! Casting xs:double and xs:float to xs:integer (F&O 3.1 19.1.2.4: the
//! fractional part discarded) and `idiv` on doubles and floats (4.2.5).
//! xs:integer has no upper limit in Xee, so every finite value converts
//! exactly. The expected integers were computed independently of Xee.

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

const LARGEST_DOUBLE: &str = "179769313486231570814527423731704356798070567525844996598917476803157260780028538760589558632766878171540458953514382464234321326889464182768467546703537516986049910576551282076245490090389328944075868508455133942304583236903222948165808559332123348274797826204144723168738177180919299881250404026184124858368";

#[test]
fn test_large_values_convert_exactly() {
    for (expr, text) in [
        ("xs:integer(1e30)", "1000000000000000019884624838656"),
        ("xs:integer(-1e30)", "-1000000000000000019884624838656"),
        ("xs:integer(1.7976931348623157e308)", LARGEST_DOUBLE),
        (
            "xs:integer(xs:float('3.4028235e38'))",
            "340282346638528859811704183484516925440",
        ),
        (
            "xs:integer(xs:float('1e30'))",
            "1000000015047466219876688855040",
        ),
        ("xs:integer(9007199254740993e0)", "9007199254740992"),
        (
            "xs:nonNegativeInteger(1e30)",
            "1000000000000000019884624838656",
        ),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

#[test]
fn test_the_fraction_is_discarded() {
    for (expr, text) in [
        ("xs:integer(2.9e0)", "2"),
        ("xs:integer(-2.9e0)", "-2"),
        ("xs:integer(-0.5e0)", "0"),
        ("xs:integer(-0e0)", "0"),
        ("xs:integer(5e-324)", "0"),
        ("xs:integer(xs:float('-7.5'))", "-7"),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
}

#[test]
fn test_bounded_integer_types_keep_their_limits() {
    assert_eq!(string_of("xs:long(9.2e18)"), "9200000000000000000");
    assert_eq!(string_of("xs:byte(127.9e0)"), "127");
    assert_eq!(string_of("xs:unsignedByte(-0.5e0)"), "0");
    // 19.3.3 makes these FORG0001; Xee raises FOCA0003 for the bounded
    // types (open), so only the failure is pinned here.
    for expr in ["xs:long(9.3e18)", "xs:byte(128e0)", "xs:unsignedLong(-1e0)"] {
        assert!(run(expr).is_err(), "{expr}");
    }
    for expr in [
        "xs:integer(xs:double('NaN'))",
        "xs:integer(xs:double('INF'))",
        "xs:integer(xs:float('-INF'))",
    ] {
        assert_eq!(error_of(expr), ErrorValue::FOCA0002, "{expr}");
    }
}

#[test]
fn test_idiv_on_doubles_and_floats_is_exact() {
    for (expr, text) in [
        ("9.3e18 idiv 1", "9300000000000000000"),
        ("1e30 idiv 1", "1000000000000000019884624838656"),
        ("-1e30 idiv 3e0", "-333333333333333316505293553664"),
        (
            "xs:float('1e30') idiv xs:float(1)",
            "1000000015047466219876688855040",
        ),
        ("7 idiv 2e0", "3"),
        ("-7e0 idiv 2", "-3"),
        ("1e30 idiv xs:double('INF')", "0"),
    ] {
        assert_eq!(string_of(expr), text, "{expr}");
    }
    // A quotient beyond the doubles overflows.
    assert_eq!(error_of("1e308 idiv 1e-10"), ErrorValue::FOAR0002);
    assert_eq!(error_of("-1e308 idiv 1e-10"), ErrorValue::FOAR0002);
    assert_eq!(error_of("xs:double('NaN') idiv 1"), ErrorValue::FOAR0002);
    assert_eq!(error_of("1 idiv 0e0"), ErrorValue::FOAR0001);
}
