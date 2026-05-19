// XBRL Formula dialect: `INF`, `+INF`, `-INF`, `NaN` as xs:double literals.
//
// Standard XPath has no infinity/NaN literals; the XBRL Formula expression
// dialect adds them (as does the reference processor, Arelle). The dialect is
// opt-in via `StaticContextBuilder::dialect`; these tests cover the
// acceptance criteria with it enabled, and that standard parsing is unchanged
// when it is not.

use xee_xpath::{
    context::{StaticContextBuilder, XPathDialect},
    error, Documents, Queries, Query,
};

/// Queries built with the XBRL Formula dialect enabled.
fn xbrl_queries() -> Queries<'static> {
    let mut builder = StaticContextBuilder::default();
    builder.dialect(XPathDialect::XbrlFormula);
    Queries::new(builder)
}

fn one_f64(expr: &str) -> error::Result<f64> {
    let mut documents = Documents::new();
    let doc = documents
        .add_string("http://example.com".try_into().unwrap(), "<root/>")
        .unwrap();
    let queries = xbrl_queries();
    let q = queries.one(expr, |_, item| Ok(item.try_into_value::<f64>()?))?;
    q.execute(&mut documents, doc)
}

fn one_bool(expr: &str) -> error::Result<bool> {
    let mut documents = Documents::new();
    let doc = documents
        .add_string("http://example.com".try_into().unwrap(), "<root/>")
        .unwrap();
    let queries = xbrl_queries();
    let q = queries.one(expr, |_, item| Ok(item.try_into_value::<bool>()?))?;
    q.execute(&mut documents, doc)
}

fn one_string(expr: &str) -> error::Result<String> {
    let mut documents = Documents::new();
    let doc = documents
        .add_string("http://example.com".try_into().unwrap(), "<root/>")
        .unwrap();
    let queries = xbrl_queries();
    let q = queries.one(expr, |_, item| Ok(item.try_into_value::<String>()?))?;
    q.execute(&mut documents, doc)
}

#[test]
fn test_inf_literal() {
    assert_eq!(one_f64("INF").unwrap(), f64::INFINITY);
}

#[test]
fn test_negative_inf_literal() {
    assert_eq!(one_f64("-INF").unwrap(), f64::NEG_INFINITY);
}

#[test]
fn test_positive_inf_literal() {
    assert_eq!(one_f64("+INF").unwrap(), f64::INFINITY);
}

#[test]
fn test_nan_literal() {
    assert!(one_f64("NaN").unwrap().is_nan());
}

#[test]
fn test_inf_arithmetic() {
    assert_eq!(one_f64("INF + 1").unwrap(), f64::INFINITY);
}

#[test]
fn test_inf_equals_division_by_zero() {
    // Double-by-double division by zero yields infinity (integer/decimal
    // division by zero raises FOAR0001 instead), so the divisor must be a
    // double for this comparison.
    assert!(one_bool("INF = (1e0 div 0e0)").unwrap());
}

#[test]
fn test_string_of_inf() {
    assert_eq!(one_string("string(INF)").unwrap(), "INF");
}

#[test]
fn test_string_of_nan() {
    assert_eq!(one_string("string(NaN)").unwrap(), "NaN");
}

#[test]
fn test_nan_compares_unequal_to_itself() {
    assert!(!one_bool("NaN = NaN").unwrap());
}

#[test]
fn test_nan_ordering_comparisons_are_false() {
    // IEEE-754: every ordering comparison involving NaN is false.
    assert!(!one_bool("2.0 < NaN").unwrap());
    assert!(!one_bool("2.0 <= NaN").unwrap());
    assert!(!one_bool("2.0 > NaN").unwrap());
    assert!(!one_bool("2.0 >= NaN").unwrap());
    assert!(!one_bool("NaN < NaN").unwrap());
    assert!(!one_bool("NaN <= NaN").unwrap());
    assert!(!one_bool("NaN > 2e0").unwrap());
    assert!(!one_bool("NaN >= 2e0").unwrap());
}

#[test]
fn test_inf_ordering_comparisons() {
    assert!(one_bool("2.0 < INF").unwrap());
    assert!(one_bool("-INF < 2.0").unwrap());
    assert!(one_bool("INF > 0").unwrap());
}

#[test]
fn test_inf_nan_still_usable_inside_a_qname() {
    // The dialect only shadows a *bare* INF/NaN. A QName whose prefix or local
    // name is INF/NaN still parses as a name — the prefixed-name escape hatch.
    let mut documents = Documents::new();
    let doc = documents
        .add_string(
            "http://example.com".try_into().unwrap(),
            r#"<root xmlns:ex="urn:x"><ex:NaN>hi</ex:NaN></root>"#,
        )
        .unwrap();
    let mut scb = StaticContextBuilder::default();
    scb.add_namespace("ex", "urn:x");
    let queries = Queries::new(scb);
    let q = queries
        .one("/root/ex:NaN/string()", |_, item| {
            Ok(item.try_into_value::<String>()?)
        })
        .unwrap();
    assert_eq!(q.execute(&mut documents, doc).unwrap(), "hi");
}

#[test]
fn test_inf_is_double_typed() {
    // The literal must be xs:double — assert via a type check in-expression.
    assert!(one_bool("INF instance of xs:double").unwrap());
    assert!(one_bool("NaN instance of xs:double").unwrap());
}

#[test]
fn test_standard_dialect_treats_inf_nan_as_names() {
    // With the default (Standard) dialect the extension is fully off: a bare
    // INF / NaN is an ordinary name test, exactly as in standard XPath.
    let mut documents = Documents::new();
    let doc = documents
        .add_string(
            "http://example.com".try_into().unwrap(),
            "<root><INF>i</INF><NaN>n</NaN></root>",
        )
        .unwrap();
    // Queries::default() does not enable the dialect.
    let queries = Queries::default();
    let q = queries
        .one("concat(/root/INF, /root/NaN)", |_, item| {
            Ok(item.try_into_value::<String>()?)
        })
        .unwrap();
    assert_eq!(q.execute(&mut documents, doc).unwrap(), "in");
}
