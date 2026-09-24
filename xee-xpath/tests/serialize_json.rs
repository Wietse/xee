mod common;

use common::run;
use xee_xpath::error::ErrorValue;

// json-node-output-method values other than xml and html used to hit a
// todo!() and panic. They are unsupported, as for the top-level `method`.
#[test]
fn test_unsupported_json_node_output_method_is_sepm0016() {
    for method in [
        "'text'",
        "'xhtml'",
        "'json'",
        "'adaptive'",
        "QName('http://example.com/ns', 'xml')",
    ] {
        let expr = format!(
            "serialize(parse-xml('<a/>'), map{{'method':'json','json-node-output-method':{method}}})"
        );
        let err = run(&expr).expect_err(&expr);
        assert_eq!(err.error, ErrorValue::SEPM0016, "{expr}");
    }
}

#[test]
fn test_supported_json_node_output_methods_still_serialize() {
    // The exact escaping (e.g. of `/`) is the JSON-core plan's S3; here we
    // only check that xml and html keep serializing the node.
    for method in ["xml", "html"] {
        let expr = format!(
            "serialize(parse-xml('<a/>'), map{{'method':'json','json-node-output-method':'{method}'}}) => contains('<a')"
        );
        let result = run(&expr).unwrap();
        assert!(result.effective_boolean_value().unwrap(), "{expr}");
    }
}
