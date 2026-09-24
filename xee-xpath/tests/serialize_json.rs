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

fn error_of(expr: &str) -> ErrorValue {
    run(expr).expect_err(expr).error
}

// Values outside a method parameter's domain are SEPM0016 when the
// parameters are read, even if the parameter is never used: here no node is
// serialized, so json-node-output-method never applies.
#[test]
fn test_method_values_outside_the_domain_are_sepm0016() {
    for value in ["'json'", "'adaptive'", "'foo'", "''"] {
        let expr =
            format!("serialize(1, map{{'method':'json','json-node-output-method':{value}}})");
        assert_eq!(error_of(&expr), ErrorValue::SEPM0016, "{expr}");
    }
    for value in ["'foo'", "''"] {
        let expr = format!("serialize(1, map{{'method':{value}}})");
        assert_eq!(error_of(&expr), ErrorValue::SEPM0016, "{expr}");
    }
}

#[test]
fn test_text_output_method() {
    assert_true(
        "serialize(parse-xml('<a>x<b>y</b></a>'), \
         map{'method':'json','json-node-output-method':'text'}) eq '\"xy\"'",
    );
    assert_true("serialize(parse-xml('<a>x<b>y</b></a>'), map{'method':'text'}) eq 'xy'");
    assert_true("serialize((1, 2), map{'method':'text', 'item-separator':'-'}) eq '1-2'");
}

// xhtml and adaptive are valid methods that Xee does not implement, and a
// QName in a namespace names an implementation-defined method, of which Xee
// has none. None of these is an invalid parameter value.
#[test]
fn test_valid_but_unimplemented_methods_are_unsupported() {
    for value in ["'xhtml'", "QName('http://example.com/ns', 'xml')"] {
        let expr = format!(
            "serialize(parse-xml('<a/>'), map{{'method':'json','json-node-output-method':{value}}})"
        );
        assert!(
            matches!(error_of(&expr), ErrorValue::Unsupported(_)),
            "{expr}"
        );
    }
    for value in [
        "'xhtml'",
        "'adaptive'",
        "QName('http://example.com/ns', 'x')",
    ] {
        let expr = format!("serialize(parse-xml('<a/>'), map{{'method':{value}}})");
        assert!(
            matches!(error_of(&expr), ErrorValue::Unsupported(_)),
            "{expr}"
        );
    }
}

#[test]
fn test_xml_and_html_json_node_output_methods_still_serialize() {
    // The exact escaping (e.g. of `/`) is the JSON-core plan's S3; here we
    // only check that xml and html keep serializing the node.
    for method in ["xml", "html"] {
        assert_true(&format!(
            "serialize(parse-xml('<a/>'), map{{'method':'json','json-node-output-method':'{method}'}}) => contains('<a')"
        ));
    }
}
